/* Copyright 2024 The TensorFlow Authors. All Rights Reserved.

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

#include <memory>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include "absl/log/check.h"
#include "absl/log/log.h"
#include "absl/status/status.h"
#include "absl/strings/str_cat.h"
#include "absl/strings/string_view.h"
#include "absl/types/span.h"
#include "tensorflow/compiler/mlir/tfrt/transforms/ifrt/ifrt_types.h"
#include "xla/hlo/ir/hlo_sharding.h"
#include "xla/python/ifrt/array.h"
#include "xla/python/ifrt/future.h"
#include "xla/xla_data.pb.h"
#include "tensorflow/core/framework/attr_value.pb.h"
#include "tensorflow/core/framework/device_base.h"
#include "tensorflow/core/framework/node_def_util.h"
#include "tensorflow/core/framework/op_kernel.h"
#include "tensorflow/core/framework/resource_handle.h"
#include "tensorflow/core/framework/tensor.h"
#include "tensorflow/core/framework/tensor_shape.h"
#include "tensorflow/core/framework/types.pb.h"
#include "tensorflow/core/platform/protobuf.h"  // IWYU pragma: keep
#include "tensorflow/core/tfrt/fallback/op_kernel_runner.h"
#include "tensorflow/core/tfrt/ifrt/ifrt_config.pb.h"
#include "tensorflow/core/tfrt/ifrt/ifrt_loaded_variable_registry.h"
#include "tensorflow/core/tfrt/ifrt/ifrt_model_context.h"
#include "tensorflow/core/tfrt/ifrt/ifrt_restore_tensor_registry.h"
#include "tensorflow/core/tfrt/ifrt/sharding_utils.h"
#include "tensorflow/core/tfrt/mlrt/bytecode/bytecode.h"
#include "tensorflow/core/tfrt/mlrt/interpreter/context.h"
#include "tensorflow/core/tfrt/mlrt/kernel/context.h"
#include "tensorflow/core/tfrt/mlrt/kernel/kernel.h"
#include "tensorflow/core/tfrt/mlrt/kernel/kernel_runner_utils.h"
#include "tensorflow/core/tfrt/utils/fallback_tensor.h"
#include "tsl/concurrency/ref_count.h"
#include "tsl/platform/errors.h"
#include "tsl/platform/statusor.h"
#include "tsl/platform/tstring.h"

namespace tensorflow {
namespace tf_mlrt {

namespace {

absl::StatusOr<tsl::RCReference<xla::ifrt::Array>> LoadIfrtVariable(
    tensorflow::ifrt_serving::IfrtModelContext& ifrt_model_context,
    const tensorflow::Tensor& variable,
    absl::string_view sharding_config_proto_text, absl::string_view name) {
  tensorflow::ifrt_serving::VariableDeviceShardingConfigProto sharding_config;

  if (!tensorflow::protobuf::TextFormat::ParseFromString(
          sharding_config_proto_text, &sharding_config)) {
    return absl::InvalidArgumentError(absl::StrCat(
        "Attribute: ", sharding_config_proto_text, " cannot be parsed"));
  }

  std::vector<int> device_ids{sharding_config.device_ids().begin(),
                              sharding_config.device_ids().end()};
  TF_ASSIGN_OR_RETURN(xla::HloSharding hlo_sharding,
                      xla::HloSharding::FromProto(sharding_config.sharding()));
  TF_ASSIGN_OR_RETURN(
      auto result_array,
      tensorflow::ifrt_serving::MakeArrayFromTensor(
          *ifrt_model_context.GetClient(), variable, absl::MakeSpan(device_ids),
          hlo_sharding, ifrt_model_context.GetThreadPool()));

  return result_array;
}

std::string GetRuntimeNameFromVarHandle(const ResourceHandle& handle) {
  return absl::StrCat(handle.container(), "__", handle.name());
}

absl::StatusOr<ifrt_serving::DtypeAndShape> GetDtypeAndShape(
    const ResourceHandle& variable) {
  std::vector<DtypeAndPartialTensorShape> dtype_and_partial_shapes =
      variable.dtypes_and_shapes();

  if (dtype_and_partial_shapes.size() != 1) {
    return absl::InvalidArgumentError(absl::StrCat(
        "Expected 1 dtype and shape, got ", dtype_and_partial_shapes.size()));
  }
  ifrt_serving::DtypeAndShape dtype_and_shape;
  if (!dtype_and_partial_shapes.front().shape.AsTensorShape(
          &dtype_and_shape.shape)) {
    return absl::InvalidArgumentError(
        absl::StrCat("Failed to convert partial shape to full tensor shape: ",
                     dtype_and_partial_shapes.front().shape.DebugString()));
  }

  dtype_and_shape.dtype = dtype_and_partial_shapes.front().dtype;
  return dtype_and_shape;
}

struct MlrtIfrtRestoreVariableKernel : mlrt::KernelFrame {
  using KernelFrame::KernelFrame;

  static constexpr char kName[] = "tf_mlrt.ifrt_restore_variable";

  tensorflow::tfrt_stub::FallbackTensor prefix() const {
    DCHECK_GT(arguments().size(), 3);
    return arguments()[0].Get<tensorflow::tfrt_stub::FallbackTensor>();
  }
  tensorflow::tfrt_stub::FallbackTensor tensor_names() const {
    DCHECK_GT(arguments().size(), 3);
    return arguments()[1].Get<tensorflow::tfrt_stub::FallbackTensor>();
  }
  tensorflow::tfrt_stub::FallbackTensor shape_and_slices() const {
    DCHECK_GT(arguments().size(), 3);
    return arguments()[2].Get<tensorflow::tfrt_stub::FallbackTensor>();
  }

  mlrt::bc::Vector<tensorflow::DataType> restored_dtypes() const {
    return attributes().GetAs<mlrt::bc::Vector<tensorflow::DataType>>(0);
  }

  std::vector<tensorflow::tfrt_stub::FallbackTensor> var_handles() const {
    DCHECK_GT(arguments().size(), 3);
    std::vector<tensorflow::tfrt_stub::FallbackTensor> result;
    result.reserve(arguments().size() - 3);
    for (int i = 3; i < arguments().size(); ++i) {
      result.push_back(
          arguments()[i].Get<tensorflow::tfrt_stub::FallbackTensor>());
    }
    return result;
  }

  Context& context() { return execution_context().GetUserContext<Context>(); }
  void Invoke();
};

void MlrtIfrtRestoreVariableKernel::Invoke() {
  std::optional<tensorflow::ifrt_serving::IfrtModelContext*>
      ifrt_model_context =
          context()
              .resource_context()
              .GetResource<tensorflow::ifrt_serving::IfrtModelContext>(
                  "IfrtModelContext");
  if (!ifrt_model_context.has_value()) {
    execution_context().Fail(absl::FailedPreconditionError(
        "RestoreVariableOp: failed to fetch IfrtModelContext"));
    return;
  }
  const int num_outputs = var_handles().size();
  DCHECK_EQ(num_outputs, tensor_names().tensor().NumElements());
  auto& fallback_request_state = context().fallback_request_state();
  tensorflow::AttrValue dtypes_attr_value;
  for (const auto& dtype : restored_dtypes()) {
    dtypes_attr_value.mutable_list()->mutable_type()->Add(dtype);
  }
  // Use `tf.RestoreV2` to restore tensor. This will also populate
  // tensorflow::ResourceManager.
  // TODO(b/319045348): avoid populating tensorflow::ResourceManager if the
  // variable is only used by device/IFRT.
  // TODO(b/319045348): consider directly calling restore function such as that
  // in /tensorflow/core/kernels/save_restore_v2_ops.cc
  auto runner = tfrt_stub::OpKernelRunner::Create(
                    /*op_name=*/
                    "RestoreV2", /*node_name=*/"RestoreV2",
                    context().params().device->name(),
                    /*num_args=*/3,
                    [&](tensorflow::AttrValueMap* attr_value_map) {
                      attr_value_map->insert({"dtypes", dtypes_attr_value});
                      return absl::OkStatus();
                    },
                    fallback_request_state.device_manager(),
                    fallback_request_state.process_function_library_runtime())
                    .value();

  // Prepare the input tensors.
  std::vector<tensorflow::TensorValue> input_tf_tensor_values;
  input_tf_tensor_values.resize(arguments().size());
  for (int i = 0; i < arguments().size(); ++i) {
    auto& fallback_tensor =
        arguments()[i].Get<tensorflow::tfrt_stub::FallbackTensor>();
    input_tf_tensor_values[i].tensor = &fallback_tensor.tensor();
  }

  auto& params = context().params();
  SetUpParams(runner, input_tf_tensor_values, params);

  struct AsyncState {
    explicit AsyncState(
        const std::vector<tensorflow::TensorValue>& input_tf_tensor_values,
        const OpKernelContext::Params& params, int num_outputs)
        : run_state(input_tf_tensor_values, params),
          context(&run_state.params, num_outputs) {}

    tfrt_stub::OpKernelRunState run_state;
    OpKernelContext context;
    std::vector<xla::ifrt::Promise<absl::StatusOr<tensorflow::Tensor>>> results;
  };
  auto async_state =
      std::make_unique<AsyncState>(input_tf_tensor_values, params, num_outputs);

  async_state->results.reserve(num_outputs);

  ifrt_serving::IfrtRestoreTensorRegistry& ifrt_restore_tensor_registry =
      (*ifrt_model_context)->GetRestoreTensorRegistry();
  for (int i = 0; i < num_outputs; ++i) {
    auto promise =
        xla::ifrt::Future<absl::StatusOr<tensorflow::Tensor>>::CreatePromise();
    auto future =
        xla::ifrt::Future<absl::StatusOr<tensorflow::Tensor>>(promise);

    std::string runtime_name = GetRuntimeNameFromVarHandle(
        var_handles()[i].tensor().scalar<ResourceHandle>()());
    if (auto status =
            ifrt_restore_tensor_registry.TryRegister(runtime_name, future);
        !status.ok()) {
      // Propagate errors so that if already-registered futures are being waited
      // on, they can be unblocked.
      for (auto& result : async_state->results) {
        std::move(result).Set(status);
      }
      execution_context().Fail(std::move(status));
      return;
    }
    async_state->results.push_back(std::move(promise));
  }

  // Use dedicated work queue for restore operation.
  DCHECK((*ifrt_model_context)->checkpoint_loader_queue() != nullptr);
  (*ifrt_model_context)
      ->checkpoint_loader_queue()
      ->AddTask(
          [runner = std::move(runner), async_state = std::move(async_state)]() {
            auto* op_kernel_context_ptr = &async_state->context;
            runner.Run(op_kernel_context_ptr);

            auto& op_kernel_context = async_state->context;
            if (!op_kernel_context.status().ok()) {
              for (auto& result : async_state->results) {
                std::move(result).Set(op_kernel_context.status());
              }
              return;
            }
            for (int i = 0; i < op_kernel_context.num_outputs(); ++i) {
              DCHECK(op_kernel_context.mutable_output(i));
              std::move(async_state->results[i])
                  .Set(std::move(*op_kernel_context.mutable_output(i)));
            }
          });
}

class MlrtIfrtLoadVariableKernel : public mlrt::KernelFrame {
 public:
  using KernelFrame::KernelFrame;

  static constexpr char kName[] = "tf_mlrt.ifrt_load_variable";

  const ResourceHandle& variable() const {
    DCHECK_GE(arguments().size(), 1);
    const auto& tensor =
        arguments()[0].Get<tensorflow::tfrt_stub::FallbackTensor>().tensor();

    DCHECK_EQ(tensor.NumElements(), 1);
    return tensor.scalar<ResourceHandle>()();
  }
  absl::string_view sharding_config_proto_text() const {
    DCHECK_EQ(attributes().size(), 2);
    return attributes().GetAs<mlrt::bc::String>(0).Get();
  }

  Context& context() { return execution_context().GetUserContext<Context>(); }
  void Invoke();

 private:
  absl::Status InvokeHelper();
};

void MlrtIfrtLoadVariableKernel::Invoke() {
  absl::Status status = InvokeHelper();
  if (!status.ok()) {
    execution_context().Fail(std::move(status));
    return;
  }
}

absl::Status MlrtIfrtLoadVariableKernel::InvokeHelper() {
  DCHECK_EQ(1, results().size());
  std::optional<tensorflow::ifrt_serving::IfrtModelContext*>
      ifrt_model_context =
          context()
              .resource_context()
              .GetResource<tensorflow::ifrt_serving::IfrtModelContext>(
                  "IfrtModelContext");
  if (!ifrt_model_context.has_value()) {
    return absl::FailedPreconditionError(
        "LoadVariableOp: failed to fetch IfrtModelContext: ");
  }

  // TODO(b/319045348): remove name() attribute. we now gets name from variable
  // handle.
  std::string runtime_name = GetRuntimeNameFromVarHandle(variable());
  xla::ifrt::Future<absl::StatusOr<tensorflow::Tensor>> restored_tensor_future =
      (*ifrt_model_context)->GetRestoreTensorRegistry().Get(runtime_name);
  if (!restored_tensor_future.IsValid()) {
    return absl::InternalError(absl::StrCat(
        "LoadVariableOp: failed to fetch variable tensor: ", runtime_name));
  }

  auto loaded_variable_promise = xla::ifrt::Future<
      absl::StatusOr<tsl::RCReference<xla::ifrt::Array>>>::CreatePromise();
  auto loaded_variable_future =
      xla::ifrt::Future<absl::StatusOr<tsl::RCReference<xla::ifrt::Array>>>(
          loaded_variable_promise);

  TF_ASSIGN_OR_RETURN(ifrt_serving::DtypeAndShape dtype_and_shape,
                      GetDtypeAndShape(variable()));

  TF_RETURN_IF_ERROR(
      (*ifrt_model_context)
          ->GetLoadedVariableRegistry()
          .TryRegisterLoadedVariable(
              runtime_name,
              [&]() -> absl::StatusOr<ifrt_serving::IfrtLoadedVariableRegistry::
                                          LoadedVariable> {
                return ifrt_serving::IfrtLoadedVariableRegistry::LoadedVariable(
                    {.dtype_and_shape = dtype_and_shape,
                     .array = loaded_variable_future});
              }));

  restored_tensor_future.OnReady(
      [ifrt_model_context = *ifrt_model_context,
       sharding_config = std::string(sharding_config_proto_text()),
       runtime_name = runtime_name,
       loaded_variable_promise = std::move(loaded_variable_promise)](
          absl::StatusOr<tensorflow::Tensor> restored_tensor) mutable {
        if (!restored_tensor.ok()) {
          loaded_variable_promise.Set(std::move(restored_tensor).status());
          return;
        }

        // Transfer tensor to array in a separate thread.
        ifrt_model_context->checkpoint_loader_queue()->AddTask(
            [ifrt_model_context, runtime_name = std::move(runtime_name),
             sharding_config = std::move(sharding_config),
             restored_tensor = std::move(*restored_tensor),
             loaded_variable_promise =
                 std::move(loaded_variable_promise)]() mutable {
              absl::StatusOr<tsl::RCReference<xla::ifrt::Array>>
                  variable_array =
                      LoadIfrtVariable(*ifrt_model_context, restored_tensor,
                                       sharding_config, runtime_name);
              loaded_variable_promise.Set(std::move(variable_array));
            });
      });
  // Return the name as the key
  tensorflow::Tensor key_tensor(tensorflow::DT_STRING, {});
  key_tensor.scalar<tsl::tstring>()() = runtime_name;
  results()[0].Set(tensorflow::tfrt_stub::FallbackTensor(key_tensor));

  return absl::OkStatus();
}

void RegisterTfMlrtIfrtKernels(mlrt::KernelRegistry& registry) {
  registry.Register<MlrtIfrtLoadVariableKernel>();
  registry.Register<MlrtIfrtRestoreVariableKernel>();
}

}  // namespace

const bool kUnused = [] {
  RegisterTfMlrtIfrtKernels(GetTfMlrtOptionalKernelRegistry());
  return true;
}();

}  // namespace tf_mlrt
}  // namespace tensorflow
