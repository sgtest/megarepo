/* Copyright 2023 The TensorFlow Authors. All Rights Reserved.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
==============================================================================*/

#include "tensorflow/compiler/mlir/tfrt/transforms/ifrt/tf2hlo.h"

#include <memory>
#include <string>
#include <utility>
#include <vector>

#include "absl/log/check.h"
#include "absl/log/log.h"
#include "absl/status/status.h"
#include "absl/status/statusor.h"
#include "absl/strings/str_cat.h"
#include "absl/strings/string_view.h"
#include "absl/types/span.h"
#include "llvm/ADT/StringRef.h"
#include "mlir/Dialect/Func/IR/FuncOps.h"  // from @llvm-project
#include "mlir/IR/Attributes.h"  // from @llvm-project
#include "mlir/IR/BuiltinAttributes.h"  // from @llvm-project
#include "mlir/IR/BuiltinOps.h"  // from @llvm-project
#include "mlir/IR/OperationSupport.h"  // from @llvm-project
#include "mlir/IR/OwningOpRef.h"  // from @llvm-project
#include "mlir/IR/Visitors.h"  // from @llvm-project
#include "mlir/Pass/PassManager.h"  // from @llvm-project
#include "tensorflow/compiler/mlir/tensorflow/utils/dump_mlir_util.h"
#include "tensorflow/compiler/mlir/tensorflow/utils/serialize_mlir_module_utils.h"
#include "tensorflow/compiler/mlir/tf2xla/api/v2/legalize_tf.h"
#include "tensorflow/compiler/tf2xla/layout_util.h"
#include "tensorflow/compiler/tf2xla/xla_compiler.h"
#include "tensorflow/compiler/tf2xla/xla_helpers.h"
#include "xla/client/client_library.h"
#include "xla/python/ifrt/client.h"
#include "xla/service/computation_placer.h"
#include "xla/service/llvm_ir/llvm_util.h"
#include "xla/shape.h"
#include "xla/stream_executor/multi_platform_manager.h"
#include "xla/translate/hlo_to_mhlo/hlo_to_mlir_hlo.h"
#include "xla/xla_data.pb.h"
#include "tensorflow/core/framework/function.pb.h"
#include "tensorflow/core/framework/tensor_shape.h"
#include "tensorflow/core/framework/tensor_shape.pb.h"
#include "tensorflow/core/framework/types.pb.h"
#include "tensorflow/core/protobuf/tpu/compile_metadata.pb.h"
#include "tensorflow/core/protobuf/tpu/topology.pb.h"
#include "tensorflow/core/tpu/kernels/tpu_compile_op_support.h"
#include "tsl/platform/errors.h"
#include "tsl/platform/protobuf.h"
#include "tsl/platform/statusor.h"

namespace tensorflow {
namespace ifrt_serving {
namespace {
static constexpr absl::string_view kEntryFuncName = "main";

absl::StatusOr<tensorflow::tpu::TPUCompileMetadataProto> GetCompileMetadata(
    mlir::func::FuncOp op, absl::Span<const tensorflow::Tensor> inputs,
    const xla::ifrt::Client& ifrt_client) {
  tensorflow::tpu::TPUCompileMetadataProto metadata;

  static constexpr absl::string_view kMetadataAttrName = "tpu_compile_metadata";
  // This is not backward compatible and only used for debug.
  static constexpr absl::string_view kMetadataTextAttrName =
      "__tpu_compile_metadata_text";
  auto metadata_attr = op->getAttrOfType<mlir::StringAttr>(kMetadataAttrName);
  auto metadata_text_attr =
      op->getAttrOfType<mlir::StringAttr>(kMetadataTextAttrName);

  if (metadata_attr && !metadata_attr.getValue().empty()) {
    // tpu_compile_metadata takes priority if exists.
    VLOG(1) << "Parsing from attribute " << kMetadataAttrName << " : "
            << metadata_attr.getValue().str();
    if (!metadata.ParseFromString(metadata_attr.getValue())) {
      return absl::InternalError(
          absl::StrCat("Failed to parse tpu_compile_metadata attribute:",
                       metadata_attr.getValue().str()));
    }
  } else if (metadata_text_attr && !metadata_text_attr.getValue().empty()) {
    // Try __tpu_compile_metadata_text attribute. This only for debugging
    // purpose.
    VLOG(1) << "Parsing from attribute " << kMetadataTextAttrName
            << metadata_text_attr.getValue().str();
    if (!tsl::protobuf::TextFormat::ParseFromString(
            metadata_text_attr.getValue(), &metadata)) {
      return absl::InvalidArgumentError(absl::StrCat(
          "Attribute ", kMetadataTextAttrName, ":",
          metadata_text_attr.getValue().str(), " cannot be parsed"));
    }
  } else {
    return absl::InvalidArgumentError(absl::StrCat(
        "Missing ", kMetadataAttrName, " and ", kMetadataTextAttrName));
  }

  VLOG(3) << "TpuCompileMetadata before shape is populated " << metadata;
  if (metadata.num_replicas() < 1 || metadata.num_cores_per_replica() < 1) {
    return absl::InternalError(
        absl::StrCat("Number of replicas ", metadata.num_replicas(),
                     " and number of cores per replica ",
                     metadata.num_cores_per_replica(), " must be >= 1"));
  }
  if (op.getNumResults() != metadata.retvals_size()) {
    return absl::InternalError(
        absl::StrCat("Number of retvals mismatched! Expected ",
                     op.getNumResults(), " got ", metadata.retvals_size()));
  }
  if (metadata.args_size() != inputs.size()) {
    return absl::InternalError(
        absl::StrCat("Number of inputs mismatched! Expected ",
                     metadata.args_size(), " got ", inputs.size()));
  }

  for (int i = 0; i < metadata.args_size(); ++i) {
    if (metadata.args(i).kind() !=
        tensorflow::tpu::TPUCompileMetadataProto::Arg::PARAMETER) {
      return absl::InternalError(absl::StrCat(
          "Only support PARAMETER, but got ", metadata.args(i).kind()));
    }

    if (metadata.args(i).dtype() != inputs[i].dtype()) {
      return absl::InternalError(absl::StrCat("Dtype mismatched! Expected ",
                                              metadata.args(i).dtype(), " got ",
                                              inputs[i].dtype()));
    }

    // Update shape.
    *metadata.mutable_args(i)->mutable_shape() = inputs[i].shape().AsProto();
  }

  // Create a default device assignment if one is not given by the model.
  if (!metadata.has_device_assignment()) {
    TF_ASSIGN_OR_RETURN(
        auto device_assignment,
        ifrt_client.GetDefaultDeviceAssignment(
            metadata.num_replicas(), metadata.num_cores_per_replica()));

    xla::DeviceAssignmentProto device_assignment_proto;
    TF_RETURN_IF_ERROR(device_assignment.Serialize(&device_assignment_proto));

    *metadata.mutable_device_assignment() = device_assignment_proto;
  }

  return metadata;
}
}  // namespace

absl::StatusOr<Tf2HloResult> CompileTfToHlo(
    mlir::ModuleOp module, absl::Span<const tensorflow::Tensor> inputs,
    absl::string_view entry_function_name, const xla::ifrt::Client& ifrt_client,
    tensorflow::XlaHelpers::ShapeRepresentationFn shape_representation_fn) {
  if (VLOG_IS_ON(1)) {
    tensorflow::DumpMlirOpToFile("ifrt_before_bridge_phase2", module);
  }

  tpu::MlirToHloArgs mlir_to_hlo_args;
  std::string module_str = tensorflow::SerializeMlirModule(module);
  mlir_to_hlo_args.mlir_module = module_str;
  // Use fallback bridge as other modes may get deprecated.
  mlir_to_hlo_args.rollout_state =
      ConfigProto::Experimental::MLIR_BRIDGE_ROLLOUT_DISABLED;

  TF_ASSIGN_OR_RETURN(
      auto* platform,
      stream_executor::MultiPlatformManager::PlatformWithName("Host"));
  TF_ASSIGN_OR_RETURN(
      auto* client, xla::ClientLibrary::GetOrCreateCompileOnlyClient(platform));

  auto entry_fn = module.lookupSymbol<mlir::func::FuncOp>(kEntryFuncName);
  if (!entry_fn) {
    return absl::InternalError("Could not find entry function in MLIR Module.");
  }

  if (inputs.size() != entry_fn.getNumArguments()) {
    return absl::InternalError(
        absl::StrCat("Entry function arguments mismatched! Expected ",
                     entry_fn.getNumArguments(), " got", inputs.size()));
  }

  TF_ASSIGN_OR_RETURN(tensorflow::tpu::TPUCompileMetadataProto compile_metadata,
                      GetCompileMetadata(entry_fn, inputs, ifrt_client));

  VLOG(1) << "Compilation metadata: " << compile_metadata;

  std::vector<TensorShape> arg_shapes;
  for (const auto& input : inputs) {
    arg_shapes.push_back(input.shape());
  }

  bool use_tuple_args = false;
  std::vector<tpu::ShardingAndIndex> arg_core_mapping;
  std::vector<std::vector<xla::Shape>> per_core_arg_shapes;
  std::vector<std::unique_ptr<mlir::Pass>> custom_legalization_passes;

  TF_ASSIGN_OR_RETURN(
      tensorflow::XlaCompiler::CompilationResult compilation_result,
      tensorflow::tf2xla::v2::LegalizeMlirToHlo(
          mlir_to_hlo_args, compile_metadata, use_tuple_args,
          /*device_type=*/"XLA_TPU_JIT", custom_legalization_passes,
          /*shape_determination_fns=*/
          tensorflow::XlaShapeLayoutHelpers::ShapeDeterminationFns(
              tensorflow::UseNoPreferenceLayoutFn(), shape_representation_fn),
          arg_shapes, &arg_core_mapping, &per_core_arg_shapes, client));

  for (auto arg_shapes_iter = per_core_arg_shapes.begin() + 1;
       arg_shapes_iter != per_core_arg_shapes.end(); ++arg_shapes_iter) {
    if (per_core_arg_shapes.front() != *arg_shapes_iter) {
      return absl::UnimplementedError(
          "Only support even sharding SPMD, but get "
          "different shapes across cores");
    }
  }

  Tf2HloResult result;
  result.mlir_hlo_module = xla::llvm_ir::CreateMlirModuleOp(module->getLoc());
  result.compile_metadata = std::move(compile_metadata);

  TF_RETURN_IF_ERROR(xla::ConvertHloToMlirHlo(
      *result.mlir_hlo_module, &compilation_result.computation->proto()));

  if (VLOG_IS_ON(1)) {
    tensorflow::DumpMlirOpToFile("ifrt_after_bridge_phase2",
                                 result.mlir_hlo_module.get());
  }

  return result;
}

}  // namespace ifrt_serving
}  // namespace tensorflow
