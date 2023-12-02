/* Copyright 2020 The TensorFlow Authors. All Rights Reserved.

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

#include "xla/translate/mhlo_to_lhlo_with_xla/mhlo_to_lhlo_with_xla.h"

#include <algorithm>
#include <array>
#include <cstdint>
#include <functional>
#include <memory>
#include <optional>
#include <string>
#include <tuple>
#include <utility>
#include <vector>

#include "absl/algorithm/container.h"
#include "absl/cleanup/cleanup.h"
#include "absl/container/flat_hash_map.h"
#include "absl/status/status.h"
#include "absl/strings/match.h"
#include "absl/types/optional.h"
#include "llvm/ADT/STLExtras.h"
#include "llvm/ADT/SmallVector.h"
#include "mlir/AsmParser/AsmParser.h"  // from @llvm-project
#include "mlir/Dialect/Arith/IR/Arith.h"  // from @llvm-project
#include "mlir/Dialect/Bufferization/IR/Bufferization.h"  // from @llvm-project
#include "mlir/Dialect/Func/IR/FuncOps.h"  // from @llvm-project
#include "mlir/Dialect/MemRef/IR/MemRef.h"  // from @llvm-project
#include "mlir/IR/Attributes.h"  // from @llvm-project
#include "mlir/IR/Builders.h"  // from @llvm-project
#include "mlir/IR/BuiltinAttributes.h"  // from @llvm-project
#include "mlir/IR/BuiltinOps.h"  // from @llvm-project
#include "mlir/IR/BuiltinTypes.h"  // from @llvm-project
#include "mlir/IR/Location.h"  // from @llvm-project
#include "mlir/IR/MLIRContext.h"  // from @llvm-project
#include "mlir/IR/OpDefinition.h"  // from @llvm-project
#include "mlir/IR/Operation.h"  // from @llvm-project
#include "mlir/IR/SymbolTable.h"  // from @llvm-project
#include "mlir/IR/Value.h"  // from @llvm-project
#include "mlir/IR/ValueRange.h"  // from @llvm-project
#include "mlir/IR/Verifier.h"  // from @llvm-project
#include "mlir/Pass/Pass.h"  // from @llvm-project
#include "mlir/Support/LLVM.h"  // from @llvm-project
#include "mlir/Support/LogicalResult.h"  // from @llvm-project
#include "mlir/Tools/mlir-translate/Translation.h"  // from @llvm-project
#include "xla/debug_options_flags.h"
#include "xla/hlo/ir/hlo_casting_utils.h"
#include "xla/hlo/ir/hlo_opcode.h"
#include "xla/mlir/utils/error_util.h"
#include "xla/mlir_hlo/lhlo/IR/lhlo_ops.h"
#include "xla/mlir_hlo/lhlo_gpu/IR/lhlo_gpu_ops.h"
#include "xla/service/backend.h"
#include "xla/service/buffer_assignment.h"
#include "xla/service/gpu/backend_configs.pb.h"
#include "xla/service/gpu/cublas_cudnn.h"
#include "xla/service/gpu/ir_emission_utils.h"
#include "xla/service/gpu/matmul_utils.h"
#include "xla/service/hlo_parser.h"
#include "xla/service/llvm_ir/buffer_assignment_util.h"
#include "xla/service/llvm_ir/llvm_util.h"
#include "xla/shape_util.h"
#include "xla/statusor.h"
#include "xla/translate/hlo_to_mhlo/attribute_importer.h"
#include "xla/translate/hlo_to_mhlo/hlo_function_importer.h"
#include "xla/translate/hlo_to_mhlo/hlo_utils.h"
#include "xla/translate/mhlo_to_hlo/mlir_hlo_to_hlo.h"
#include "xla/translate/mhlo_to_hlo/type_to_shape.h"
#include "xla/util.h"
#include "xla/window_util.h"
#include "xla/xla_data.pb.h"
#include "tsl/platform/errors.h"
#include "tsl/platform/status.h"
#include "tsl/platform/statusor.h"

using xla::BufferAllocation;
using xla::BufferAssignment;
using xla::HloComputation;
using xla::HloCustomCallInstruction;
using xla::HloInfeedInstruction;
using xla::HloInstruction;
using xla::HloModule;
using xla::HloModuleProto;
using xla::HloOutfeedInstruction;
using xla::HloProto;
using xla::Shape;

namespace mlir {
namespace {

absl::string_view StringRefToView(llvm::StringRef ref) {
  return {ref.data(), ref.size()};
}

tsl::StatusOr<std::unique_ptr<HloModule>> HloModuleFromProto(
    const HloProto& hlo_proto) {
  const HloModuleProto& module_proto = hlo_proto.hlo_module();
  TF_ASSIGN_OR_RETURN(const xla::HloModuleConfig module_config,
                      HloModule::CreateModuleConfigFromProto(
                          module_proto, xla::GetDebugOptionsFromFlags()));
  return HloModule::CreateFromProto(module_proto, module_config);
}

bool IsSyncCollective(const HloInstruction* instr) {
  auto backend_config =
      instr->backend_config<xla::gpu::CollectiveBackendConfig>().value();
  return backend_config.is_sync();
}

bool NoParallelCustomCallCollective(const HloInstruction* instr) {
  auto backend_config =
      instr->backend_config<xla::gpu::CollectiveBackendConfig>().value();
  return backend_config.no_parallel_custom_call();
}

// Convert the MLIR `module` from HLO dialect to LHLO dialect using XLA for the
// given platform.
tsl::Status ConvertHloToLmhlo(std::unique_ptr<HloModule> hlo_module,
                              ModuleOp module, StringRef platform_name) {
  auto platform = xla::se::MultiPlatformManager::PlatformWithName(
      StringRefToView(platform_name));
  if (!platform.ok()) {
    std::string error_msg;
    llvm::raw_string_ostream os(error_msg);
    os << "failed to get platform: " << platform.status().ToString()
       << " (available Platform: ";
    std::vector<std::string> available_platforms;
    (void)xla::se::MultiPlatformManager::PlatformsWithFilter(
        [&](const stream_executor::Platform* p) {
          available_platforms.push_back(p->Name());
          return false;
        });
    llvm::interleaveComma(available_platforms, os);
    os << ")";
    return tsl::errors::InvalidArgument("%s", os.str().c_str());
  }

  xla::BackendOptions backend_options;
  backend_options.set_platform(platform.value());
  auto backend_or_err = xla::Backend::CreateBackend(backend_options);
  TF_RETURN_WITH_CONTEXT_IF_ERROR(backend_or_err.status(),
                                  "failed to create XLA Backend ");
  auto backend = std::move(backend_or_err.value());

  tsl::StatusOr<std::unique_ptr<BufferAssignment>> assignment =
      backend->compiler()->AssignBuffers(hlo_module.get(),
                                         backend->default_stream_executor());
  TF_RETURN_WITH_CONTEXT_IF_ERROR(assignment.status(),
                                  "running XLA buffer assigment");

  // Clear the module before populating it back with the result of the
  // conversion.
  module.getBody()->clear();
  OpBuilder builder(module);

  std::vector<const BufferAllocation*> ordered_allocations;
  TF_RETURN_WITH_CONTEXT_IF_ERROR(
      HloToLhloModule(**assignment, *hlo_module, module, &ordered_allocations),
      "converting HLO to LHLO");

  return ::tsl::OkStatus();
}

}  // namespace

// Creates MLIR operands corresponding to operands and results of the XLA HLO
// instruction. If `num_operands` is valid, then only the first `num_operands`
// operands of the HLO instruction will be considered.
tsl::Status LhloDialectEmitter::CreateOperands(
    const HloInstruction* instr, std::optional<int64_t> num_operands,
    TokenLoweringMode token_mode, llvm::SmallVectorImpl<Value>& operands,
    size_t& num_arguments, size_t& num_results) {
  if (num_operands.value_or(0) > instr->operand_count())
    return tsl::errors::InvalidArgument(
        "num_operands must be <= operand count");
  for (int64_t i = 0; i < num_operands.value_or(instr->operand_count()); ++i) {
    TF_RETURN_IF_ERROR(GetOrCreateView(instr->operand(i), &operands,
                                       /*result_subset=*/{}, token_mode));
  }
  num_arguments = operands.size();
  TF_RETURN_IF_ERROR(
      GetOrCreateView(instr, &operands, /*result_subset=*/{}, token_mode));
  num_results = operands.size() - num_arguments;
  return ::tsl::OkStatus();
}

template <typename OpType>
OpType LhloDialectEmitter::CreateOpWithoutAttrs(const HloInstruction* instr,
                                                ValueRange operands) {
  Location loc = getLocation(instr);
  return builder_.create<OpType>(loc, std::nullopt, operands,
                                 llvm::ArrayRef<NamedAttribute>{});
}

template <typename OpType>
tsl::StatusOr<OpType> LhloDialectEmitter::CreateOpWithoutAttrs(
    const HloInstruction* instr, size_t& num_arguments, size_t& num_results,
    std::optional<int64_t> num_operands) {
  llvm::SmallVector<Value, 4> operands;
  TF_RETURN_IF_ERROR(CreateOperands(instr, num_operands,
                                    TokenLoweringMode::kFailToLower, operands,
                                    num_arguments, num_results));
  return CreateOpWithoutAttrs<OpType>(instr, operands);
}

tsl::StatusOr<mlir::Operation*> LhloDialectEmitter::EmitOp(
    const HloInstruction* instr) {
  using xla::HloOpcode;
  switch (instr->opcode()) {
    case HloOpcode::kAddDependency:
      return nullptr;
    case HloOpcode::kAfterAll:
      // LMHLO is already ordered. This assumption may be broken after
      // introducing async regions and partial orders.
      return nullptr;
    case HloOpcode::kAllGatherStart:
      return EmitAllGatherStartOp(instr);
    case HloOpcode::kAllGatherDone:
      return EmitAllGatherDoneOp(instr);
    case HloOpcode::kAllReduceStart:
      return EmitAllReduceStartOp(instr);
    case HloOpcode::kAllReduceDone:
      return EmitAllReduceDoneOp(instr);
    case HloOpcode::kAsyncStart:
      return EmitAsyncStartOp(instr);
    case HloOpcode::kAsyncDone:
      return EmitAsyncDoneOp(instr);
    case HloOpcode::kBitcast:
      return EmitBitcast(instr);
    case HloOpcode::kCollectivePermuteStart:
      return EmitCollectivePermuteStartOp(instr);
    case HloOpcode::kCollectivePermuteDone:
      return EmitCollectivePermuteDoneOp(instr);
    case HloOpcode::kConditional:
      return EmitCaseOp(instr);
    case HloOpcode::kFft:
      return EmitFftOp(instr);
    case HloOpcode::kGetTupleElement:
      return nullptr;
    case HloOpcode::kInfeed:
      return EmitInfeedOp(instr);
    case HloOpcode::kOutfeed:
      return EmitOutfeedOp(instr);
    case HloOpcode::kPartitionId:
      return CreateOpWithoutAttrs<lmhlo::PartitionIdOp>(instr);
    case HloOpcode::kReplicaId:
      return CreateOpWithoutAttrs<lmhlo::ReplicaIdOp>(instr);
    case HloOpcode::kTriangularSolve:
      return EmitTriangularSolveOp(instr);
    case HloOpcode::kTuple:
      return nullptr;
    case HloOpcode::kSort:
      return EmitSortOp(instr);
    case HloOpcode::kFusion:
      return EmitFusionOp(instr);
    case HloOpcode::kScatter:
      return EmitScatterOp(instr);
    case HloOpcode::kSelectAndScatter:
      return EmitSelectAndScatterOp(instr);
    case HloOpcode::kCustomCall:
      return EmitCustomCallOp(instr);
    case HloOpcode::kConstant:
      return EmitConstant(instr);
    case HloOpcode::kRngGetAndUpdateState:
      return EmitRngGetAndUpdateStateOp(instr);
    case HloOpcode::kWhile:
      return EmitWhileOp(instr);
    case HloOpcode::kSend:
      return EmitSendOp(instr);
    case HloOpcode::kSendDone:
      return EmitSendDoneOp(instr);
    case HloOpcode::kRecv:
      return EmitRecvOp(instr);
    case HloOpcode::kRecvDone:
      return EmitRecvDoneOp(instr);
    // TODO(b/302038092): Currently the command buffer call is represented by
    // a kCall. We need to be able to differentiate it from a regular kCall.
    case HloOpcode::kCall:
      return EmitCommandBufferOp(instr);
    default:
      llvm::errs() << instr->ToString();
      llvm::errs() << "\n\nModule:\n"
                   << instr->GetModule()->ToString() << "\n\n";
      return tsl::errors::Internal(
          absl::StrCat("LHLO opcode ", xla::HloOpcodeString(instr->opcode()),
                       " is not supported."));
  }
}

tsl::Status LhloDialectEmitter::DefaultAction(const HloInstruction* instr) {
  TF_ASSIGN_OR_RETURN(auto* op, EmitOp(instr));
  if (op) {
    lhlo_to_hlo_[op] = instr;
  }
  return tsl::OkStatus();
}

tsl::StatusOr<lmhlo::SortOp> LhloDialectEmitter::EmitSortOp(
    const HloInstruction* instr) {
  TF_ASSIGN_OR_RETURN(auto sort, CreateOpWithoutAttrs<lmhlo::SortOp>(instr));
  auto* sort_instr = xla::Cast<xla::HloSortInstruction>(instr);
  sort.setDimensionAttr(
      builder_.getI64IntegerAttr(sort_instr->sort_dimension()));
  sort.setIsStableAttr(builder_.getBoolAttr(sort_instr->is_stable()));
  TF_RETURN_IF_ERROR(xla::HloFunctionImporter::ImportAsRegion(
      *sort_instr->called_computations()[0], symbol_table_,
      &sort.getComparator(), &builder_));
  return sort;
}

// Walks MHLO::TupleOp recursively.
tsl::Status WalkTuplePostOrder(
    Value v, const std::function<tsl::Status(Value)>& visitor) {
  if (auto* op = v.getDefiningOp()) {
    if (auto tuple = dyn_cast<mhlo::TupleOp>(op)) {
      for (Value sub_v : tuple.getVal()) {
        TF_RETURN_IF_ERROR(WalkTuplePostOrder(sub_v, visitor));
      }
      return ::tsl::OkStatus();
    }
  }
  return visitor(v);
}

tsl::StatusOr<Value> LhloDialectEmitter::RewriteFusionOperand(
    const HloInstruction* root, const Shape& shape,
    xla::ShapeIndex* shape_index, OpBuilder* b, Location loc) {
  if (shape.IsTuple()) {
    llvm::SmallVector<Value, 4> values;
    for (int i = 0; i < shape.tuple_shapes_size(); ++i) {
      shape_index->push_back(i);
      TF_ASSIGN_OR_RETURN(
          auto v, RewriteFusionOperand(root, shape.tuple_shapes(i), shape_index,
                                       b, loc));
      values.push_back(v);
      shape_index->pop_back();
    }
    return Value(b->create<mhlo::TupleOp>(loc, values));
  }
  TF_ASSIGN_OR_RETURN(Value memref,
                      GetOrCreateArrayView(root, shape, *shape_index));
  auto load = b->create<bufferization::ToTensorOp>(loc, memref);
  if (shape.layout() !=
      xla::LayoutUtil::MakeDescendingLayout(shape.dimensions().size())) {
    llvm::SmallVector<int64_t, 4> minor_to_major(
        shape.layout().minor_to_major().begin(),
        shape.layout().minor_to_major().end());
    load->setAttr("xla_shape",
                  b->getStringAttr(shape.ToString(/*print_layout=*/true)));
  }
  return load.getResult();
}

// Emit a lmhlo.fusion based on XLA HLO fusion. Structurally they are not neatly
// equivalent. Specifically, XLA HLO fusion:
//     fused_computation {
//       %p0 = parameter(0)
//       %p1 = parameter(1)
//       ...
//       ROOT %ret = ...
//     }
// will be converted to
//     lmhlo.fusion() {  // no explicit operands
//       // capturing outside buffers
//       %p0 = bufferization.to_tensor(%arg0) : memref<...> -> tensor<...>
//       %p1 = bufferization.to_tensor(%arg1) : memref<...> -> tensor<...>
//       ...
//       tensor_store ..., %ret // store a tensor to a memref
//     }
tsl::StatusOr<lmhlo::FusionOp> LhloDialectEmitter::EmitFusionOp(
    const HloInstruction* instr) {
  Location loc = getLocation(instr);

  auto* fusion_instr = xla::Cast<xla::HloFusionInstruction>(instr);

  auto fusion = builder_.create<lmhlo::FusionOp>(getLocation(instr));
  auto after_fusion = builder_.saveInsertionPoint();
  auto reverter = absl::MakeCleanup(
      [this, after_fusion] { builder_.restoreInsertionPoint(after_fusion); });
  builder_ = mlir::OpBuilder(fusion);

  auto region_builder = OpBuilder::atBlockBegin(&fusion.getRegion().front());

  llvm::SmallVector<Value, 8> arguments;
  for (int i = 0; i < instr->operands().size(); ++i) {
    const HloInstruction* operand = instr->operand(i);
    xla::ShapeIndex shape_index;
    TF_ASSIGN_OR_RETURN(
        auto arg, RewriteFusionOperand(operand, operand->shape(), &shape_index,
                                       &region_builder, loc));
    arguments.push_back(arg);
  }

  TF_ASSIGN_OR_RETURN(Value result,
                      xla::HloFunctionImporter::ImportInstructions(
                          *fusion_instr->fused_instructions_computation(),
                          arguments, symbol_table_, &region_builder));
  {
    int i = 0;
    llvm::SmallVector<Value, 4> output;
    TF_RETURN_IF_ERROR(GetOrCreateView(instr, &output));
    TF_RETURN_IF_ERROR(WalkTuplePostOrder(result, [&](Value v) mutable {
      auto materialize_op =
          region_builder.create<bufferization::MaterializeInDestinationOp>(
              loc, v, output[i++]);
      materialize_op.setWritable(true);
      return ::tsl::OkStatus();
    }));
    if (i != output.size()) {
      return xla::InternalError("output sizes don't match");
    }
  }

  // The fusion op might not have a backend-config.  But we at least want to set
  // the fusion kind, because LMHLO doesn't have this concept.
  TF_ASSIGN_OR_RETURN(auto backend_config,
                      instr->backend_config<xla::gpu::FusionBackendConfig>());
  if (backend_config.kind().empty() &&
      instr->opcode() == xla::HloOpcode::kFusion) {
    backend_config.set_kind(std::string(ToString(instr->fusion_kind())));
  }

  TF_ASSIGN_OR_RETURN(std::string backend_config_str,
                      HloInstruction::BackendConfigToRawString(backend_config));
  fusion.setBackendConfigAttr(builder_.getStringAttr(backend_config_str));

  // For custom fusion backend config we also attach serialized version of the
  // attached HLO computation.
  if (backend_config.kind() == "__custom_fusion") {
    std::string computation_str;
    fusion_instr->fused_instructions_computation()->ToProto().SerializeToString(
        &computation_str);
    fusion->setAttr("__custom_fusion_computation",
                    builder_.getStringAttr(computation_str));
  }

  // Fold GTE/Tuple pairs.
  //
  // Since the fused region refers to values in its parent region, we can't
  // call applyPatternAndFoldGreedily. We optimize it manually.
  //
  // Only walk once, because post-ordering is exactly what we need for GTE
  // optimizations.
  fusion.getRegion().walk([](mhlo::GetTupleElementOp gte) {
    SmallVector<Value, 4> folded_values;
    if (succeeded(OpBuilder(gte).tryFold(gte, folded_values))) {
      gte.replaceAllUsesWith(folded_values[0]);
    }
  });

  // Effectively a DCE on the region.
  {
    llvm::SmallVector<mlir::Operation*, 4> ops;
    fusion.getRegion().walk([&](mlir::Operation* op) { ops.push_back(op); });
    // Visit the user first.
    std::reverse(ops.begin(), ops.end());
    for (auto op : ops) {
      if (isOpTriviallyDead(op)) op->erase();
    }
  }

  return fusion;
}

tsl::StatusOr<mhlo::ScatterDimensionNumbersAttr>
LhloDialectEmitter::GetScatterDimensionNumbers(const HloInstruction* instr,
                                               mlir::MLIRContext* context) {
  auto* scatter_instr = xla::Cast<xla::HloScatterInstruction>(instr);

  const xla::ScatterDimensionNumbers& xla_scatter_dim =
      scatter_instr->scatter_dimension_numbers();

  auto get_i64_array = [](absl::Span<const int64_t> container) {
    return ArrayRef<int64_t>{container.data(),
                             static_cast<size_t>(container.size())};
  };
  auto scatter_dimension_numbers = mhlo::ScatterDimensionNumbersAttr::get(
      context, get_i64_array(xla_scatter_dim.update_window_dims()),
      get_i64_array(xla_scatter_dim.inserted_window_dims()),
      get_i64_array(xla_scatter_dim.scatter_dims_to_operand_dims()),
      xla_scatter_dim.index_vector_dim());
  return scatter_dimension_numbers;
}

tsl::StatusOr<lmhlo::ScatterOp> LhloDialectEmitter::EmitScatterOp(
    const HloInstruction* instr) {
  TF_ASSIGN_OR_RETURN(auto scatter,
                      CreateOpWithoutAttrs<lmhlo::ScatterOp>(instr));

  // copy attributes
  auto* scatter_instr = xla::Cast<xla::HloScatterInstruction>(instr);

  TF_ASSIGN_OR_RETURN(auto scatter_dimension_numbers,
                      GetScatterDimensionNumbers(instr, builder_.getContext()));
  scatter.setScatterDimensionNumbersAttr(scatter_dimension_numbers);
  scatter.setIndicesAreSortedAttr(
      builder_.getBoolAttr(scatter_instr->indices_are_sorted()));
  scatter.setUniqueIndicesAttr(
      builder_.getBoolAttr(scatter_instr->unique_indices()));

  // import update computation as region
  TF_RETURN_IF_ERROR(xla::HloFunctionImporter::ImportAsRegion(
      *scatter_instr->called_computations()[0], symbol_table_,
      &scatter.getUpdateComputation(), &builder_));

  return scatter;
}

tsl::StatusOr<lmhlo::SelectAndScatterOp>
LhloDialectEmitter::EmitSelectAndScatterOp(const HloInstruction* instr) {
  TF_ASSIGN_OR_RETURN(auto select_and_scatter,
                      CreateOpWithoutAttrs<lmhlo::SelectAndScatterOp>(instr));

  // copy attributes
  auto* select_and_scatter_instr =
      xla::Cast<xla::HloSelectAndScatterInstruction>(instr);
  const xla::Window& window = select_and_scatter_instr->window();

  if (xla::window_util::HasDilation(window)) {
    return tsl::errors::Unimplemented(
        "Dilation for SelectAndScatter is not supported");
  }

  select_and_scatter.setWindowDimensionsAttr(
      GetWindowElements(window, [](const xla::WindowDimension& dim) {
        return static_cast<int64_t>(dim.size());
      }));
  select_and_scatter.setWindowStridesAttr(
      GetWindowElements(window, [](const xla::WindowDimension& dim) {
        return static_cast<int64_t>(dim.stride());
      }));
  select_and_scatter.setPaddingAttr(
      GetWindowElements(window, [](const xla::WindowDimension& dim) {
        return static_cast<int64_t>(dim.padding_low());
      }));

  // import select and scatter computation as region
  TF_RETURN_IF_ERROR(xla::HloFunctionImporter::ImportAsRegion(
      *select_and_scatter_instr->select(), symbol_table_,
      &select_and_scatter.getSelect(), &builder_));
  TF_RETURN_IF_ERROR(xla::HloFunctionImporter::ImportAsRegion(
      *select_and_scatter_instr->scatter(), symbol_table_,
      &select_and_scatter.getScatter(), &builder_));
  return select_and_scatter;
}

tsl::StatusOr<mlir::Operation*> LhloDialectEmitter::EmitCustomCallOp(
    const HloInstruction* instr) {
  auto* custom_call_instr = xla::Cast<xla::HloCustomCallInstruction>(instr);

  if (xla::gpu::IsCustomCallToCusolver(*instr)) {
    return EmitCholesky(custom_call_instr);
  }

  if (xla::gpu::IsLegacyCublasMatmul(*instr)) {
    return EmitGemm(custom_call_instr);
  }

  if (xla::gpu::IsCublasLtMatmul(*instr)) {
    return EmitCublasLtMatmul(custom_call_instr);
  }

  if (xla::gpu::IsCublasLtMatmulF8(*instr)) {
    return EmitCublasLtMatmulF8(custom_call_instr);
  }

  if (xla::gpu::IsCustomCallToDnnConvolution(*instr)) {
    return EmitDnnConvolution(custom_call_instr);
  }

  if (xla::gpu::IsCudnnConvolutionReorder(*instr)) {
    return EmitDnnConvolutionReorderVectorized(custom_call_instr);
  }

  if (xla::gpu::IsCustomCallToDnnNorm(*instr)) {
    return EmitDnnNorm(custom_call_instr);
  }

  if (xla::gpu::IsFwdCustomCallTofMHA(*instr)) {
    return EmitDnnfMHA(custom_call_instr);
  }
  if (xla::gpu::IsBwdCustomCallTofMHA(*instr)) {
    return EmitDnnfMHABackward(custom_call_instr);
  }
  if (xla::gpu::IsCubDeviceRadixSort(*instr)) {
    return EmitCubDeviceRadixSort(custom_call_instr);
  }

  // For custom call, if there are any token operands or results, they will not
  // be represented in LHLO so we need to remember the mapping. First create
  // operands where each token is replaced with a null Value.
  llvm::SmallVector<Value, 4> operands;
  size_t num_arguments, num_results;
  TF_RETURN_IF_ERROR(CreateOperands(instr, /*num_operands=*/std::nullopt,
                                    TokenLoweringMode::kUseNull, operands,
                                    num_arguments, num_results));

  // Now check if any of the operands is Null, which would indicate the presence
  // of a token in the input or output.
  bool has_token = llvm::any_of(operands, [](Value v) { return !v; });

  lmhlo::CustomCallTargetArgMappingAttr target_mapping;
  if (has_token) {
    // If there was a token, squeeze all the non-token arguments and results
    // (in-place) and remember the mapping.
    int next_index = 0;
    llvm::SmallVector<int64_t> arg_to_target_arg_mapping;
    for (int i = 0; i < num_arguments; ++i) {
      if (operands[i]) {
        arg_to_target_arg_mapping.push_back(i);
        operands[next_index++] = operands[i];
      }
    }
    // Size of arg_to_target_arg_mapping is the number of arguments in LHLO.
    llvm::SmallVector<int64_t> result_to_target_result_mapping;
    for (int i = num_arguments; i < operands.size(); ++i) {
      if (operands[i]) {
        result_to_target_result_mapping.push_back(i - num_arguments);
        operands[next_index++] = operands[i];
      }
    }

    // Build the mapping attribute.
    target_mapping = lmhlo::CustomCallTargetArgMappingAttr::get(
        builder_.getContext(), num_arguments, num_results,
        arg_to_target_arg_mapping, result_to_target_result_mapping);

    // Drop the remaining operands and adjust num_arguments and num_results
    // for LMHLO creation.
    operands.resize(next_index);
    num_arguments = arg_to_target_arg_mapping.size();
    num_results = result_to_target_result_mapping.size();
  }

  auto custom_call = CreateOpWithoutAttrs<lmhlo::CustomCallOp>(instr, operands);
  TF_ASSIGN_OR_RETURN(
      auto mlir_api_version,
      ConvertCustomCallApiVersion(custom_call_instr->api_version()));
  custom_call.setCallTargetNameAttr(
      builder_.getStringAttr(custom_call_instr->custom_call_target()));
  custom_call.setApiVersionAttr(mhlo::CustomCallApiVersionAttr::get(
      builder_.getContext(), mlir_api_version));

  // For typed custom calls we need to parse user-defined attributes back to the
  // dictionary attribute, and then add them back to the custom call op.
  if (mlir_api_version == mhlo::CustomCallApiVersion::API_VERSION_TYPED_FFI) {
    if (custom_call_instr->opaque().empty()) {
      auto empty = mlir::DictionaryAttr::get(builder_.getContext());
      custom_call.setBackendConfigAttr(empty);
    } else {
      mlir::Attribute attr = mlir::parseAttribute(custom_call_instr->opaque(),
                                                  builder_.getContext());
      TF_RET_CHECK(attr.isa<mlir::DictionaryAttr>())
          << "Couldn't parse backend config into a dictionary attribute";
      custom_call.setBackendConfigAttr(attr);
    }
  } else {
    custom_call.setBackendConfigAttr(
        builder_.getStringAttr(custom_call_instr->opaque()));
  }

  const int32_t segments[2] = {static_cast<int32_t>(num_arguments),
                               static_cast<int32_t>(num_results)};
  custom_call->setAttr(lmhlo::CustomCallOp::getOperandSegmentSizeAttr(),
                       builder_.getDenseI32ArrayAttr(segments));
  if (target_mapping) custom_call.setTargetArgMappingAttr(target_mapping);

  for (int i = 0; i < custom_call_instr->called_computations().size(); ++i) {
    auto& region = custom_call->getRegion(i);
    TF_RETURN_IF_ERROR(xla::HloFunctionImporter::ImportAsRegion(
        *custom_call_instr->called_computation(), symbol_table_, &region,
        &builder_));
  }

  return custom_call.getOperation();
}

tsl::StatusOr<lmhlo_gpu::CholeskyOp> LhloDialectEmitter::EmitCholesky(
    const HloCustomCallInstruction* custom_call) {
  TF_ASSIGN_OR_RETURN(auto cholesky_op,
                      CreateOpWithoutAttrs<lmhlo_gpu::CholeskyOp>(custom_call));
  TF_ASSIGN_OR_RETURN(xla::CholeskyOptions options,
                      custom_call->backend_config<xla::CholeskyOptions>());
  cholesky_op.setIsLowerAttr(builder_.getBoolAttr(options.lower()));
  return cholesky_op;
}

namespace {

mhlo::DotDimensionNumbersAttr GetDotDimensionNumbersAttr(
    const OpBuilder& builder, const xla::DotDimensionNumbers& hlo_dims) {
  auto arrayref = [](absl::Span<const int64_t> array) {
    return llvm::ArrayRef<int64_t>{array.data(), array.size()};
  };
  return mhlo::DotDimensionNumbersAttr::get(
      builder.getContext(), arrayref(hlo_dims.lhs_batch_dimensions()),
      arrayref(hlo_dims.rhs_batch_dimensions()),
      arrayref(hlo_dims.lhs_contracting_dimensions()),
      arrayref(hlo_dims.rhs_contracting_dimensions()));
}

template <typename OpT>
void SetMatmulAttributes(OpT op, const xla::gpu::GemmBackendConfig& config,
                         OpBuilder& builder) {
  op.setDotDimensionNumbersAttr(
      GetDotDimensionNumbersAttr(builder, config.dot_dimension_numbers()));
  op.setAlphaRealAttr(builder.getF64FloatAttr(config.alpha_real()));
  op.setAlphaImagAttr(builder.getF64FloatAttr(config.alpha_imag()));
  op.setBetaAttr(builder.getF64FloatAttr(config.beta()));
  if (config.algorithm_case() ==
      xla::gpu::GemmBackendConfig::kSelectedAlgorithm) {
    op.setAlgorithmAttr(builder.getI64IntegerAttr(config.selected_algorithm()));
  }
  op.setPrecisionConfigAttr(
      xla::ConvertPrecisionConfig(&config.precision_config(), &builder));
  op.setGradXAttr(builder.getBoolAttr(config.grad_x()));
  op.setGradYAttr(builder.getBoolAttr(config.grad_y()));
}

tsl::StatusOr<lmhlo_gpu::CublasLtMatmulEpilogue> AsLhloEpilogue(
    xla::gpu::GemmBackendConfig_Epilogue epilogue) {
  switch (epilogue) {
    case xla::gpu::GemmBackendConfig::DEFAULT:
      return lmhlo_gpu::CublasLtMatmulEpilogue::Default;
    case xla::gpu::GemmBackendConfig::RELU:
      return lmhlo_gpu::CublasLtMatmulEpilogue::Relu;
    case xla::gpu::GemmBackendConfig::GELU:
      return lmhlo_gpu::CublasLtMatmulEpilogue::Gelu;
    case xla::gpu::GemmBackendConfig::GELU_AUX:
      return lmhlo_gpu::CublasLtMatmulEpilogue::GeluAux;
    case xla::gpu::GemmBackendConfig::BIAS:
      return lmhlo_gpu::CublasLtMatmulEpilogue::Bias;
    case xla::gpu::GemmBackendConfig::BIAS_RELU:
      return lmhlo_gpu::CublasLtMatmulEpilogue::BiasRelu;
    case xla::gpu::GemmBackendConfig::BIAS_GELU:
      return lmhlo_gpu::CublasLtMatmulEpilogue::BiasGelu;
    case xla::gpu::GemmBackendConfig::BIAS_GELU_AUX:
      return lmhlo_gpu::CublasLtMatmulEpilogue::BiasGeluAux;
    default:
      return xla::InternalError("unknown epilogue");
  }
}

tsl::StatusOr<lmhlo_gpu::FusedMhaDagSignature> AsLhloFusedMhaDagSignature(
    xla::gpu::CudnnfMHAKind kind) {
  switch (kind) {
    case xla::gpu::CudnnfMHAKind::kBmmBmm:
      return lmhlo_gpu::FusedMhaDagSignature::Default;
    case xla::gpu::CudnnfMHAKind::kScaleBiasMaskSoftmax:
      return lmhlo_gpu::FusedMhaDagSignature::ScaleBiasMaskSoftmax;
    case xla::gpu::CudnnfMHAKind::kScaleBiasMaskSoftmaxDropout:
      return lmhlo_gpu::FusedMhaDagSignature::ScaleBiasMaskSoftmaxDropout;
    case xla::gpu::CudnnfMHAKind::kScaleMaskSoftmax:
      return lmhlo_gpu::FusedMhaDagSignature::ScaleMaskSoftmax;
    case xla::gpu::CudnnfMHAKind::kScaleMaskSoftmaxDropout:
      return lmhlo_gpu::FusedMhaDagSignature::ScaleMaskSoftmaxDropout;
    case xla::gpu::CudnnfMHAKind::kSoftmaxDropout:
      return lmhlo_gpu::FusedMhaDagSignature::SoftmaxDropout;
    case xla::gpu::CudnnfMHAKind::kSoftmax:
      return lmhlo_gpu::FusedMhaDagSignature::Softmax;
    case xla::gpu::CudnnfMHAKind::kScaleBiasSoftmax:
      return lmhlo_gpu::FusedMhaDagSignature::ScaleBiasSoftmax;
    case xla::gpu::CudnnfMHAKind::kScaleBiasSoftmaxDropout:
      return lmhlo_gpu::FusedMhaDagSignature::ScaleBiasSoftmaxDropout;
    default:
      return xla::InternalError("unknown cudnn fmha fwd kind");
  }
}
tsl::StatusOr<lmhlo_gpu::FusedMhaBackwardDagSignature>
AsLhloFusedMhaBackwardDagSignature(xla::gpu::CudnnfMHAKind kind) {
  switch (kind) {
    case xla::gpu::CudnnfMHAKind::kBackwardScaleBiasSoftmax:
      return lmhlo_gpu::FusedMhaBackwardDagSignature::BackwardScaleBiasSoftmax;
      break;
    case xla::gpu::CudnnfMHAKind::kBackwardScaleBiasSoftmaxDropout:
      return lmhlo_gpu::FusedMhaBackwardDagSignature::
          BackwardScaleBiasSoftmaxDropout;
      break;
    case xla::gpu::CudnnfMHAKind::kBackwardScaleBiasMaskSoftmax:
      return lmhlo_gpu::FusedMhaBackwardDagSignature::
          BackwardScaleBiasMaskSoftmax;
      break;
    case xla::gpu::CudnnfMHAKind::kBackwardScaleBiasMaskSoftmaxDropout:
      return lmhlo_gpu::FusedMhaBackwardDagSignature::
          BackwardScaleBiasMaskSoftmaxDropout;
      break;
    case xla::gpu::CudnnfMHAKind::kBackwardSoftmax:
      return lmhlo_gpu::FusedMhaBackwardDagSignature::BackwardSoftmax;
      break;
    case xla::gpu::CudnnfMHAKind::kBackwardSoftmaxDropout:
      return lmhlo_gpu::FusedMhaBackwardDagSignature::BackwardSoftmaxDropout;
      break;
    default:
      return xla::InternalError("unknown cudnn fmha bwd kind");
  }
}
}  // namespace

tsl::StatusOr<Operation*> LhloDialectEmitter::EmitGemm(
    const HloCustomCallInstruction* custom_call) {
  TF_ASSIGN_OR_RETURN(
      auto const config,
      custom_call->backend_config<xla::gpu::GemmBackendConfig>());

  if (custom_call->operand_count() == 2) {
    TF_RET_CHECK(config.beta() == 0.);
  } else if (custom_call->operand_count() != 3) {
    return xla::InvalidArgument("GEMM custom call should have 2 or 3 operands");
  }

  // GEMM may have two or three operands. However, in the three operand case,
  // the third operand is updated in-place, so we treat that as an output here.
  TF_ASSIGN_OR_RETURN(
      lmhlo_gpu::GEMMOp op,
      CreateOpWithoutAttrs<lmhlo_gpu::GEMMOp>(custom_call,
                                              /*num_operands=*/2));

  SetMatmulAttributes(op, config, builder_);
  return op.getOperation();
}

tsl::StatusOr<Operation*> LhloDialectEmitter::EmitCublasLtMatmul(
    const HloCustomCallInstruction* custom_call) {
  TF_ASSIGN_OR_RETURN(
      auto const config,
      custom_call->backend_config<xla::gpu::GemmBackendConfig>());

  bool has_matrix_bias = config.beta() != 0.;

  TF_ASSIGN_OR_RETURN(
      bool has_vector_bias,
      xla::gpu::gpublas_lt::EpilogueAddsVectorBias(config.epilogue()));

  TF_ASSIGN_OR_RETURN(
      bool has_aux_output,
      xla::gpu::gpublas_lt::EpilogueHasAuxiliaryOutput(config.epilogue()));

  TF_RET_CHECK(custom_call->operand_count() ==
               2 + int{has_matrix_bias} + int{has_vector_bias});

  xla::ShapeIndex output_index =
      has_aux_output ? xla::ShapeIndex{0} : xla::ShapeIndex{};

  llvm::SmallVector<Value, 6> operands;
  TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(0), &operands));
  TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(1), &operands));
  if (has_matrix_bias) {
    TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(2), &operands));
  } else {
    TF_RETURN_IF_ERROR(GetOrCreateView(custom_call, &operands, output_index));
  }
  TF_RETURN_IF_ERROR(GetOrCreateView(custom_call, &operands, output_index));

  if (has_vector_bias) {
    TF_RETURN_IF_ERROR(GetOrCreateView(
        custom_call->operand(has_matrix_bias ? 3 : 2), &operands));
  }

  if (has_aux_output) {
    TF_RETURN_IF_ERROR(GetOrCreateView(custom_call, &operands, {1}));
  }

  auto op =
      CreateOpWithoutAttrs<lmhlo_gpu::CublasLtMatmulOp>(custom_call, operands);
  SetMatmulAttributes(op, config, builder_);

  int32_t operand_sizes[] = {
      1, 1, 1, 1, has_vector_bias ? 1 : 0, has_aux_output ? 1 : 0};
  op->setAttr(op.getOperandSegmentSizeAttr(),
              builder_.getDenseI32ArrayAttr(operand_sizes));

  TF_ASSIGN_OR_RETURN(lmhlo_gpu::CublasLtMatmulEpilogue epilogue,
                      AsLhloEpilogue(config.epilogue()));
  op.setEpilogueAttr(lmhlo_gpu::CublasLtMatmulEpilogueAttr::get(
      builder_.getContext(), epilogue));

  // Use the first algorithm by default (i.e. fastest according to heuristics).
  if (config.algorithm_case() !=
      xla::gpu::GemmBackendConfig::kSelectedAlgorithm) {
    op.setAlgorithmAttr(builder_.getI64IntegerAttr(0));
  }

  return op.getOperation();
}

tsl::StatusOr<Operation*> LhloDialectEmitter::EmitCublasLtMatmulF8(
    const HloCustomCallInstruction* custom_call) {
  TF_ASSIGN_OR_RETURN(
      auto const config,
      custom_call->backend_config<xla::gpu::GemmBackendConfig>());

  int ops_num = custom_call->operand_count();
  TF_RET_CHECK(ops_num == 6 || ops_num == 7 || ops_num == 8);
  TF_ASSIGN_OR_RETURN(
      bool has_vector_bias,
      xla::gpu::gpublas_lt::EpilogueAddsVectorBias(config.epilogue()));

  bool has_damax = custom_call->shape().IsTuple();
  bool has_matrix_bias = config.beta() != 0.;
  xla::ShapeIndex output_index =
      has_damax ? xla::ShapeIndex{0} : xla::ShapeIndex{};

  llvm::SmallVector<Value, 10> operands;
  TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(0), &operands));
  TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(1), &operands));

  int a_scale_index = has_matrix_bias ? 3 : 2;
  if (has_matrix_bias) {
    TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(2), &operands));
  } else {
    TF_RETURN_IF_ERROR(GetOrCreateView(custom_call, &operands, output_index));
  }

  TF_RETURN_IF_ERROR(
      GetOrCreateView(custom_call->operand(a_scale_index), &operands));
  TF_RETURN_IF_ERROR(
      GetOrCreateView(custom_call->operand(a_scale_index + 1), &operands));
  TF_RETURN_IF_ERROR(
      GetOrCreateView(custom_call->operand(a_scale_index + 2), &operands));
  TF_RETURN_IF_ERROR(
      GetOrCreateView(custom_call->operand(a_scale_index + 3), &operands));
  TF_RETURN_IF_ERROR(GetOrCreateView(custom_call, &operands, output_index));

  if (has_vector_bias) {
    TF_RETURN_IF_ERROR(
        GetOrCreateView(custom_call->operand(a_scale_index + 4), &operands));
  }
  if (has_damax) {
    TF_RETURN_IF_ERROR(GetOrCreateView(custom_call, &operands, {1}));
  }
  auto op = CreateOpWithoutAttrs<lmhlo_gpu::CublasLtMatmulF8Op>(custom_call,
                                                                operands);

  SetMatmulAttributes(op, config, builder_);
  int32_t operand_sizes[] = {
      1, 1, 1, 1, 1, 1, 1, 1, has_vector_bias ? 1 : 0, has_damax ? 1 : 0};
  op->setAttr(op.getOperandSegmentSizeAttr(),
              builder_.getDenseI32ArrayAttr(operand_sizes));
  TF_ASSIGN_OR_RETURN(lmhlo_gpu::CublasLtMatmulEpilogue epilogue,
                      AsLhloEpilogue(config.epilogue()));
  op.setEpilogueAttr(lmhlo_gpu::CublasLtMatmulEpilogueAttr::get(
      builder_.getContext(), epilogue));

  // Use the first algorithm by default (i.e. fastest according to heuristics).
  if (config.algorithm_case() !=
      xla::gpu::GemmBackendConfig::kSelectedAlgorithm) {
    op.setAlgorithmAttr(builder_.getI64IntegerAttr(0));
  }

  return op.getOperation();
}

static tsl::StatusOr<mlir::lmhlo_gpu::Activation> GetLHLOActivation(
    stream_executor::dnn::ActivationMode activation) {
  switch (activation) {
    case stream_executor::dnn::kNone:
      return mlir::lmhlo_gpu::Activation::None;
    case stream_executor::dnn::kSigmoid:
      return mlir::lmhlo_gpu::Activation::Sigmoid;
    case stream_executor::dnn::kRelu:
      return mlir::lmhlo_gpu::Activation::Relu;
    case stream_executor::dnn::kRelu6:
      return mlir::lmhlo_gpu::Activation::Relu6;
    case stream_executor::dnn::kReluX:
      return mlir::lmhlo_gpu::Activation::ReluX;
    case stream_executor::dnn::kTanh:
      return mlir::lmhlo_gpu::Activation::Tanh;
    case stream_executor::dnn::kBandPass:
      return mlir::lmhlo_gpu::Activation::BandPass;
    case stream_executor::dnn::kElu:
      return mlir::lmhlo_gpu::Activation::Elu;
    case stream_executor::dnn::kLeakyRelu:
      return mlir::lmhlo_gpu::Activation::LeakyRelu;
    default:
      return xla::InternalError("Unknown activation");
  }
}

tsl::StatusOr<Operation*> LhloDialectEmitter::EmitDnnConvolution(
    const HloCustomCallInstruction* custom_call) {
  TF_ASSIGN_OR_RETURN(
      auto const backend_config,
      custom_call->backend_config<xla::gpu::CudnnConvBackendConfig>());

  TF_ASSIGN_OR_RETURN(const xla::gpu::CudnnConvKind kind,
                      xla::gpu::GetCudnnConvKind(custom_call));

  auto get_layout_attribute = [&](const xla::Layout& layout) {
    std::vector<int64_t> minor_to_major(layout.minor_to_major_size());
    absl::c_transform(layout.minor_to_major(), minor_to_major.begin(),
                      [](int64_t x) { return static_cast<int64_t>(x); });
    return minor_to_major;
  };

  auto set_common_conv_attributes = [&, this](auto op) -> Operation* {
    const xla::Window& window = custom_call->window();
    // Window size for Cudnn Conv is same as the kernel size.
    NamedAttrList attrs(op->getAttrDictionary());
    DenseIntElementsAttr window_strides;
    attrs.set(op.getWindowStridesAttrName(),
              window_strides = GetWindowElements(
                  window, [](const xla::WindowDimension& dim) {
                    return static_cast<int64_t>(dim.stride());
                  }));
    // Cudnn Conv requires low and high padding to be equal.
    attrs.set(op.getPaddingAttrName(),
              GetWindowElements(window, [](const xla::WindowDimension& dim) {
                return static_cast<int64_t>(dim.padding_low());
              }));
    // LHS dilation is encoded in base_dilation of the backend config.
    // RHS dilation is encoded in window_dilation of the backend config.
    attrs.set(op.getLhsDilationAttrName(),
              GetWindowElements(window, [](const xla::WindowDimension& dim) {
                return static_cast<int64_t>(dim.base_dilation());
              }));
    attrs.set(op.getRhsDilationAttrName(),
              GetWindowElements(window, [](const xla::WindowDimension& dim) {
                return static_cast<int64_t>(dim.window_dilation());
              }));
    // Setup window reversal.
    auto window_reversal = llvm::to_vector<4>(llvm::map_range(
        window.dimensions(),
        [](const xla::WindowDimension& dim) { return dim.window_reversal(); }));
    auto type = RankedTensorType::get(window_strides.getType().getShape(),
                                      builder_.getIntegerType(/*width=*/1));
    attrs.set(op.getWindowReversalAttrName(),
              DenseElementsAttr::get(type, window_reversal));

    attrs.set(op.getDimensionNumbersAttrName(),
              xla::ConvertConvDimensionNumbers(
                  custom_call->convolution_dimension_numbers(), &builder_));
    attrs.set(op.getFeatureGroupCountAttrName(),
              builder_.getI64IntegerAttr(custom_call->feature_group_count()));
    attrs.set(op.getBatchGroupCountAttrName(),
              builder_.getI64IntegerAttr(custom_call->batch_group_count()));
    attrs.set(op.getPrecisionConfigAttrName(),
              xla::ConvertPrecisionConfig(&custom_call->precision_config(),
                                          &builder_));
    attrs.set(op.getResultScaleAttrName(),
              builder_.getF64FloatAttr(backend_config.conv_result_scale()));

    const auto& algorithm = backend_config.algorithm();
    std::vector<int64_t> knob_ids;
    std::vector<int64_t> knob_values;
    for (const auto& entry : algorithm.tuning_knobs()) {
      knob_ids.push_back(entry.first);
      knob_values.push_back(entry.second);
    }

    auto config = mlir::lmhlo_gpu::ConvolutionBackendConfigAttr::get(
        builder_.getContext(), algorithm.algo_id(),

        algorithm.math_type() ==
            stream_executor::dnn::AlgorithmProto::TENSOR_OP_MATH,
        knob_ids, knob_values, algorithm.is_cudnn_frontend(),
        backend_config.reordered_int8_nchw_vect(),
        algorithm.has_workspace_size() ? algorithm.workspace_size().value()
                                       : -1,
        get_layout_attribute(custom_call->operand(0)->shape().layout()),
        get_layout_attribute(custom_call->operand(1)->shape().layout()),
        get_layout_attribute(custom_call->shape().tuple_shapes(0).layout()));
    attrs.set(op.getBackendConfigAttrName(), config);
    op->setAttrs(attrs.getDictionary(op->getContext()));

    return op.getOperation();
  };

  auto set_activation = [&, this](auto op) -> tsl::Status {
    auto se_activation = static_cast<stream_executor::dnn::ActivationMode>(
        backend_config.activation_mode());
    TF_ASSIGN_OR_RETURN(mlir::lmhlo_gpu::Activation activation,
                        GetLHLOActivation(se_activation));
    auto activation_attr = ::mlir::lmhlo_gpu::ActivationAttr::get(
        getLocation(custom_call).getContext(), activation);
    op.setActivationModeAttr(activation_attr);
    return ::tsl::OkStatus();
  };

  switch (kind) {
    case xla::gpu::CudnnConvKind::kForward: {
      TF_ASSIGN_OR_RETURN(
          auto cnn_forward,
          CreateOpWithoutAttrs<lmhlo_gpu::ConvForwardOp>(custom_call));
      return set_common_conv_attributes(cnn_forward);
    }
    case xla::gpu::CudnnConvKind::kBackwardInput: {
      TF_ASSIGN_OR_RETURN(
          auto cnn_backward,
          CreateOpWithoutAttrs<lmhlo_gpu::ConvBackwardInputOp>(custom_call));
      return set_common_conv_attributes(cnn_backward);
    }
    case xla::gpu::CudnnConvKind::kBackwardFilter: {
      TF_ASSIGN_OR_RETURN(
          auto cnn_backward,
          CreateOpWithoutAttrs<lmhlo_gpu::ConvBackwardFilterOp>(custom_call));
      return set_common_conv_attributes(cnn_backward);
    }
    case xla::gpu::CudnnConvKind::kForwardActivation: {
      // Fused conv can be either with side input or without.
      if (custom_call->operand_count() == 3) {
        TF_ASSIGN_OR_RETURN(
            auto cnn_fused,
            CreateOpWithoutAttrs<lmhlo_gpu::ConvForwardFusedOp>(custom_call));
        TF_RETURN_IF_ERROR(set_activation(cnn_fused));
        cnn_fused.setLeakyreluAlphaAttr(
            builder_.getF64FloatAttr(backend_config.leakyrelu_alpha()));
        return set_common_conv_attributes(cnn_fused);
      }

      TF_RET_CHECK(custom_call->operand_count() == 4);
      TF_ASSIGN_OR_RETURN(
          auto cnn_fused_side_input,
          CreateOpWithoutAttrs<lmhlo_gpu::ConvForwardFusedSideInputOp>(
              custom_call));
      cnn_fused_side_input.setSideInputScaleAttr(
          builder_.getF64FloatAttr(backend_config.side_input_scale()));
      TF_RETURN_IF_ERROR(set_activation(cnn_fused_side_input));
      return set_common_conv_attributes(cnn_fused_side_input);
    }
    case xla::gpu::CudnnConvKind::kForwardGraph: {
      const int32_t n_binary_operands = custom_call->operand_count() - 2;
      const int32_t n_aux_outputs =
          custom_call->shape().tuple_shapes_size() - 2;
      TF_ASSIGN_OR_RETURN(
          auto cnn_graph,
          CreateOpWithoutAttrs<lmhlo_gpu::ConvForwardGraphOp>(custom_call));
      cnn_graph.setSerializedGraph(backend_config.serialized_graph());
      cnn_graph.setNAuxOutputs(n_aux_outputs);
      int32_t operand_sizes[] = {1, 1, n_binary_operands, 1, n_aux_outputs, 1};
      cnn_graph->setAttr(cnn_graph.getOperandSegmentSizeAttr(),
                         builder_.getDenseI32ArrayAttr(operand_sizes));
      return set_common_conv_attributes(cnn_graph);
    }
  }
}

tsl::StatusOr<Operation*>
LhloDialectEmitter::EmitDnnConvolutionReorderVectorized(
    const HloCustomCallInstruction* custom_call) {
  auto set_common_attributes = [&, this](auto op) -> Operation* {
    // Output shape defines the filter, it must have NCHW_VECT_C layout.
    Shape shape = custom_call->shape();
    if (shape.IsTuple()) {
      shape = shape.tuple_shapes(0);
    }

    CHECK_EQ(shape.rank(), 5);
    CHECK_EQ(shape.dimensions(4), 32);
    llvm::SmallVector<int64_t, 4> nchw = {
        shape.dimensions(0), shape.dimensions(1) * 32, shape.dimensions(2),
        shape.dimensions(3)};
    op->setAttr("filter_dims", GetI64DenseElementsAttr(nchw));

    return op.getOperation();
  };

  if (custom_call->operand_count() > 1) {
    TF_ASSIGN_OR_RETURN(
        auto reorder_filter_and_bias,
        CreateOpWithoutAttrs<lmhlo_gpu::CudnnConvReorderFilterAndBiasOp>(
            custom_call));
    return set_common_attributes(reorder_filter_and_bias);
  } else {
    TF_ASSIGN_OR_RETURN(
        auto reorder_filter,
        CreateOpWithoutAttrs<lmhlo_gpu::CudnnConvReorderFilterOp>(custom_call));
    return set_common_attributes(reorder_filter);
  }
}

tsl::StatusOr<Operation*> LhloDialectEmitter::EmitDnnNorm(
    const HloCustomCallInstruction* custom_call) {
  TF_ASSIGN_OR_RETURN(
      auto const backend_config,
      custom_call->backend_config<xla::gpu::CudnnNormBackendConfig>());

  llvm::SmallVector<Value, 7> operands;
  TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(0), &operands));
  TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(1), &operands));
  TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(2), &operands));
  TF_RETURN_IF_ERROR(GetOrCreateView(custom_call, &operands));

  auto norm =
      CreateOpWithoutAttrs<lmhlo_gpu::CudnnNormOp>(custom_call, operands);
  norm.setEpsilonAttr(builder_.getF64FloatAttr(backend_config.epsilon()));

  const auto& algorithm = backend_config.algorithm();
  auto norm_algo_config = mlir::lmhlo_gpu::NormAlgorithmConfigAttr::get(
      builder_.getContext(), algorithm.algo_id(),
      algorithm.has_workspace_size() ? algorithm.workspace_size().value() : -1);
  norm.setAlgorithmConfigAttr(norm_algo_config);

  std::vector<int64_t> operand_minor_to_major;

  auto get_minor_to_major =
      [&operand_minor_to_major](const xla::Layout& layout) -> void {
    std::vector<int64_t> minor_to_major(layout.minor_to_major_size());
    absl::c_transform(layout.minor_to_major(), minor_to_major.begin(),
                      [](int64_t x) { return static_cast<int64_t>(x); });
    operand_minor_to_major.insert(operand_minor_to_major.end(),
                                  minor_to_major.begin(), minor_to_major.end());
  };

  // Store the layout information of all operands and outputs.
  for (HloInstruction* operand : custom_call->operands()) {
    get_minor_to_major(operand->shape().layout());
  }
  for (int i = 0; i < custom_call->shape().tuple_shapes_size() - 1; ++i) {
    get_minor_to_major(custom_call->shape().tuple_shapes(i).layout());
  }

  norm.setOperandLayoutsAttr(builder_.getI64ArrayAttr(llvm::ArrayRef<int64_t>{
      operand_minor_to_major.data(), operand_minor_to_major.size()}));

  bool has_aux_outputs = custom_call->shape().tuple_shapes_size() == 4;
  int32_t operand_sizes[] = {1, 1, 1, 1, has_aux_outputs, has_aux_outputs, 1};
  norm->setAttr(norm.getOperandSegmentSizeAttr(),
                builder_.getDenseI32ArrayAttr(operand_sizes));

  return norm.getOperation();
}

tsl::StatusOr<Operation*> LhloDialectEmitter::EmitDnnfMHA(
    const HloCustomCallInstruction* custom_call) {
  TF_ASSIGN_OR_RETURN(
      auto const config,
      custom_call->backend_config<xla::gpu::CudnnfMHABackendConfig>());

  TF_ASSIGN_OR_RETURN(const xla::gpu::CudnnfMHAKind kind,
                      xla::gpu::GetCudnnfMHAKind(custom_call));

  bool has_activation =
      xla::ShapeUtil::TupleElementCount(custom_call->shape()) == 3;
  bool has_mask = false;
  bool has_bias = false;

  auto set_common_fmha_attributes =
      [&, this](auto op) -> tsl::StatusOr<Operation*> {
    TF_ASSIGN_OR_RETURN(lmhlo_gpu::FusedMhaDagSignature fused_mha_dag_signature,
                        AsLhloFusedMhaDagSignature(kind));
    op.setFusedMhaDagAttr(lmhlo_gpu::FusedMhaDagSignatureAttr::get(
        builder_.getContext(), fused_mha_dag_signature));
    op.setBmm1DotDimensionNumbersAttr(GetDotDimensionNumbersAttr(
        builder_, config.bmm1_dot_dimension_numbers()));
    op.setBmm2DotDimensionNumbersAttr(GetDotDimensionNumbersAttr(
        builder_, config.bmm2_dot_dimension_numbers()));

    const auto& algorithm = config.algorithm();
    std::vector<int64_t> knob_ids;
    std::vector<int64_t> knob_values;
    for (const auto& entry : algorithm.tuning_knobs()) {
      knob_ids.push_back(entry.first);
      knob_values.push_back(entry.second);
    }
    auto fmha_algo_config = mlir::lmhlo_gpu::FusedMHAAlgorithmConfigAttr::get(
        builder_.getContext(), algorithm.algo_id(), knob_ids, knob_values,
        algorithm.has_workspace_size() ? algorithm.workspace_size().value()
                                       : -1);
    op.setAlgorithmConfigAttr(fmha_algo_config);

    auto intermediate_tensor_shape = Shape(config.intermediate_tensor_shape());
    auto arrayref = [](absl::Span<const int64_t> array) {
      return llvm::ArrayRef<int64_t>{array.data(), array.size()};
    };
    auto intermediate_tensor_dims = builder_.getI64ArrayAttr(
        arrayref(intermediate_tensor_shape.dimensions()));
    op.setIntermediateTensorDimensionsAttr(intermediate_tensor_dims);

    auto intermediate_tensor_layout = builder_.getI64ArrayAttr(
        arrayref(intermediate_tensor_shape.layout().minor_to_major()));
    op.setIntermediateTensorLayoutAttr(intermediate_tensor_layout);
    op.setFmhaScaleAttr(builder_.getF64FloatAttr(config.fmha_scale()));
    int32_t operand_sizes[] = {1,
                               1,
                               1,
                               has_mask ? 1 : 0,
                               has_bias ? 1 : 0,
                               1,
                               1,
                               has_activation ? 1 : 0};
    op->setAttr(op.getOperandSegmentSizeAttr(),
                builder_.getDenseI32ArrayAttr(operand_sizes));
    // set is flash attention here
    op.setIsFlashAttentionAttr(
        builder_.getBoolAttr(config.is_flash_attention()));
    // set is causal mask here
    op.setIsCausalMaskAttr(builder_.getBoolAttr(config.is_causal_mask()));
    return op.getOperation();
  };
  llvm::SmallVector<Value, 8> operands;
  TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(0), &operands));
  TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(1), &operands));
  TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(2), &operands));
  switch (kind) {
    case xla::gpu::CudnnfMHAKind::kBmmBmm:
    case xla::gpu::CudnnfMHAKind::kSoftmax: {
      TF_RETURN_IF_ERROR(GetOrCreateView(custom_call, &operands));
      auto fmha =
          CreateOpWithoutAttrs<lmhlo_gpu::fusedMHAOp>(custom_call, operands);
      return set_common_fmha_attributes(fmha);
    }
    case xla::gpu::CudnnfMHAKind::kSoftmaxDropout: {
      TF_RETURN_IF_ERROR(GetOrCreateView(custom_call, &operands));
      auto fmha =
          CreateOpWithoutAttrs<lmhlo_gpu::fusedMHAOp>(custom_call, operands);
      fmha.setDropoutRateAttr(builder_.getF64FloatAttr(config.dropout_rate()));
      fmha.setSeedAttr(builder_.getI64IntegerAttr(config.seed()));
      return set_common_fmha_attributes(fmha);
    }
    case xla::gpu::CudnnfMHAKind::kScaleMaskSoftmax: {
      TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(3), &operands));
      TF_RETURN_IF_ERROR(GetOrCreateView(custom_call, &operands));
      auto fmha =
          CreateOpWithoutAttrs<lmhlo_gpu::fusedMHAOp>(custom_call, operands);
      has_mask = true;
      return set_common_fmha_attributes(fmha);
    }
    case xla::gpu::CudnnfMHAKind::kScaleMaskSoftmaxDropout: {
      TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(3), &operands));
      TF_RETURN_IF_ERROR(GetOrCreateView(custom_call, &operands));
      auto fmha =
          CreateOpWithoutAttrs<lmhlo_gpu::fusedMHAOp>(custom_call, operands);
      fmha.setDropoutRateAttr(builder_.getF64FloatAttr(config.dropout_rate()));
      fmha.setSeedAttr(builder_.getI64IntegerAttr(config.seed()));
      has_mask = true;
      return set_common_fmha_attributes(fmha);
    }
    case xla::gpu::CudnnfMHAKind::kScaleBiasMaskSoftmax: {
      TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(3), &operands));
      TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(4), &operands));
      TF_RETURN_IF_ERROR(GetOrCreateView(custom_call, &operands));
      auto fmha =
          CreateOpWithoutAttrs<lmhlo_gpu::fusedMHAOp>(custom_call, operands);
      has_mask = true;
      has_bias = true;
      return set_common_fmha_attributes(fmha);
    }
    case xla::gpu::CudnnfMHAKind::kScaleBiasMaskSoftmaxDropout: {
      TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(3), &operands));
      TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(4), &operands));
      TF_RETURN_IF_ERROR(GetOrCreateView(custom_call, &operands));
      auto fmha =
          CreateOpWithoutAttrs<lmhlo_gpu::fusedMHAOp>(custom_call, operands);
      fmha.setDropoutRateAttr(builder_.getF64FloatAttr(config.dropout_rate()));
      fmha.setSeedAttr(builder_.getI64IntegerAttr(config.seed()));
      has_mask = true;
      has_bias = true;
      return set_common_fmha_attributes(fmha);
    }
    case xla::gpu::CudnnfMHAKind::kScaleBiasSoftmax: {
      TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(3), &operands));
      TF_RETURN_IF_ERROR(GetOrCreateView(custom_call, &operands));
      auto fmha =
          CreateOpWithoutAttrs<lmhlo_gpu::fusedMHAOp>(custom_call, operands);
      has_bias = true;
      return set_common_fmha_attributes(fmha);
    }
    case xla::gpu::CudnnfMHAKind::kScaleBiasSoftmaxDropout: {
      TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(3), &operands));
      TF_RETURN_IF_ERROR(GetOrCreateView(custom_call, &operands));
      auto fmha =
          CreateOpWithoutAttrs<lmhlo_gpu::fusedMHAOp>(custom_call, operands);
      fmha.setDropoutRateAttr(builder_.getF64FloatAttr(config.dropout_rate()));
      fmha.setSeedAttr(builder_.getI64IntegerAttr(config.seed()));
      has_bias = true;
      return set_common_fmha_attributes(fmha);
    }
    default:
      return xla::InternalError("Unknown forward fused MHA call.");
  }
}

tsl::StatusOr<Operation*> LhloDialectEmitter::EmitDnnfMHABackward(
    const HloCustomCallInstruction* custom_call) {
  TF_ASSIGN_OR_RETURN(
      auto const config,
      custom_call->backend_config<xla::gpu::CudnnfMHABackendConfig>());

  TF_ASSIGN_OR_RETURN(const xla::gpu::CudnnfMHAKind kind,
                      xla::gpu::GetCudnnfMHAKind(custom_call));

  bool is_flash_attention = config.is_flash_attention();
  bool has_dbias =
      custom_call->shape().tuple_shapes().size() == 6 && !is_flash_attention;
  bool has_mask = false;
  bool has_bias = false;

  auto set_common_fmha_backward_attributes =
      [&, this](auto op) -> tsl::StatusOr<Operation*> {
    TF_ASSIGN_OR_RETURN(lmhlo_gpu::FusedMhaBackwardDagSignature
                            fused_mha_backward_dag_signature,
                        AsLhloFusedMhaBackwardDagSignature(kind));
    op.setFusedMhaDagAttr(lmhlo_gpu::FusedMhaBackwardDagSignatureAttr::get(
        builder_.getContext(), fused_mha_backward_dag_signature));
    op.setBmm1GradGemm1DotDimensionNumbersAttr(GetDotDimensionNumbersAttr(
        builder_, config.bmm1_grad_gemm1_dot_dimension_numbers()));
    op.setBmm1GradGemm2DotDimensionNumbersAttr(GetDotDimensionNumbersAttr(
        builder_, config.bmm1_grad_gemm2_dot_dimension_numbers()));
    op.setBmm2GradGemm1DotDimensionNumbersAttr(GetDotDimensionNumbersAttr(
        builder_, config.bmm2_grad_gemm1_dot_dimension_numbers()));
    op.setBmm2GradGemm2DotDimensionNumbersAttr(GetDotDimensionNumbersAttr(
        builder_, config.bmm2_grad_gemm2_dot_dimension_numbers()));

    auto intermediate_tensor_shape = Shape(config.intermediate_tensor_shape());
    auto arrayref = [](absl::Span<const int64_t> array) {
      return llvm::ArrayRef<int64_t>{array.data(), array.size()};
    };
    auto intermediate_tensor_dims = builder_.getI64ArrayAttr(
        arrayref(intermediate_tensor_shape.dimensions()));
    op.setIntermediateTensorDimensionsAttr(intermediate_tensor_dims);

    auto intermediate_tensor_layout = builder_.getI64ArrayAttr(
        arrayref(intermediate_tensor_shape.layout().minor_to_major()));
    op.setIntermediateTensorLayoutAttr(intermediate_tensor_layout);

    op.setFmhaScaleAttr(builder_.getF64FloatAttr(config.fmha_scale()));

    int32_t operand_sizes[] = {1,
                               1,
                               1,
                               1,
                               1,
                               has_mask ? 1 : 0,
                               has_bias ? 1 : 0,
                               is_flash_attention ? 1 : 0,  // fwd_output
                               1,
                               1,
                               1,
                               is_flash_attention ? 0 : 1,  // d_S
                               is_flash_attention ? 1 : 0,  // softmax_sum
                               is_flash_attention ? 1 : 0,  // d_Q_accum
                               1,
                               has_dbias ? 1 : 0};
    op->setAttr(op.getOperandSegmentSizeAttr(),
                builder_.getDenseI32ArrayAttr(operand_sizes));

    // set is flash attention here
    op.setIsFlashAttentionAttr(
        builder_.getBoolAttr(config.is_flash_attention()));
    // set is causal mask here
    op.setIsCausalMaskAttr(builder_.getBoolAttr(config.is_causal_mask()));
    const auto& algorithm = config.algorithm();
    std::vector<int64_t> knob_ids;
    std::vector<int64_t> knob_values;
    for (const auto& entry : algorithm.tuning_knobs()) {
      knob_ids.push_back(entry.first);
      knob_values.push_back(entry.second);
    }
    auto fmha_algo_config = mlir::lmhlo_gpu::FusedMHAAlgorithmConfigAttr::get(
        builder_.getContext(), algorithm.algo_id(), knob_ids, knob_values,
        algorithm.has_workspace_size() ? algorithm.workspace_size().value()
                                       : -1);
    op.setAlgorithmConfigAttr(fmha_algo_config);
    return op.getOperation();
  };

  llvm::SmallVector<Value, 15> operands;
  TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(0), &operands));
  TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(1), &operands));
  TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(2), &operands));
  TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(3), &operands));
  TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(4), &operands));

  switch (kind) {
    case xla::gpu::CudnnfMHAKind::kBackwardBmmBmm:
    case xla::gpu::CudnnfMHAKind::kBackwardSoftmax: {
      // push fwd output for bwd here if it is flash attention
      if (config.is_flash_attention()) {
        TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(5), &operands));
      }
      TF_RETURN_IF_ERROR(GetOrCreateView(custom_call, &operands));
      auto fmha_backward = CreateOpWithoutAttrs<lmhlo_gpu::fusedMHABackwardOp>(
          custom_call, operands);
      return set_common_fmha_backward_attributes(fmha_backward);
    }
    case xla::gpu::CudnnfMHAKind::kBackwardScaleBiasSoftmax: {
      // push fwd output for bwd here if it is flash attention
      if (config.is_flash_attention()) {
        has_bias = true;
        TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(5), &operands));
        TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(6), &operands));
      }
      TF_RETURN_IF_ERROR(GetOrCreateView(custom_call, &operands));
      auto fmha_backward = CreateOpWithoutAttrs<lmhlo_gpu::fusedMHABackwardOp>(
          custom_call, operands);
      return set_common_fmha_backward_attributes(fmha_backward);
    }
    case xla::gpu::CudnnfMHAKind::kBackwardScaleBiasSoftmaxDropout: {
      // push fwd output for bwd here if it is flash attention
      if (config.is_flash_attention()) {
        has_bias = true;
        TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(5), &operands));
        TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(6), &operands));
      }
      TF_RETURN_IF_ERROR(GetOrCreateView(custom_call, &operands));
      auto fmha_backward = CreateOpWithoutAttrs<lmhlo_gpu::fusedMHABackwardOp>(
          custom_call, operands);
      fmha_backward.setDropoutRateAttr(
          builder_.getF64FloatAttr(config.dropout_rate()));
      fmha_backward.setSeedAttr(builder_.getI64IntegerAttr(config.seed()));
      return set_common_fmha_backward_attributes(fmha_backward);
    }

    case xla::gpu::CudnnfMHAKind::kBackwardScaleMaskSoftmax: {
      TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(5), &operands));
      // push fwd output for bwd here if it is flash attention
      if (config.is_flash_attention()) {
        TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(6), &operands));
      }
      TF_RETURN_IF_ERROR(GetOrCreateView(custom_call, &operands));
      has_mask = true;
      auto fmha_backward = CreateOpWithoutAttrs<lmhlo_gpu::fusedMHABackwardOp>(
          custom_call, operands);
      return set_common_fmha_backward_attributes(fmha_backward);
    }
    case xla::gpu::CudnnfMHAKind::kBackwardScaleBiasMaskSoftmax: {
      TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(5), &operands));
      // push fwd output for bwd here if it is flash attention
      if (config.is_flash_attention()) {
        has_bias = true;
        TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(6), &operands));
        TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(7), &operands));
      }
      TF_RETURN_IF_ERROR(GetOrCreateView(custom_call, &operands));
      has_mask = true;
      auto fmha_backward = CreateOpWithoutAttrs<lmhlo_gpu::fusedMHABackwardOp>(
          custom_call, operands);
      return set_common_fmha_backward_attributes(fmha_backward);
    }

    case xla::gpu::CudnnfMHAKind::kBackwardScaleMaskSoftmaxDropout: {
      TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(5), &operands));
      // push fwd output for bwd here if it is flash attention
      if (config.is_flash_attention()) {
        TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(6), &operands));
      }
      TF_RETURN_IF_ERROR(GetOrCreateView(custom_call, &operands));
      has_mask = true;
      auto fmha_backward = CreateOpWithoutAttrs<lmhlo_gpu::fusedMHABackwardOp>(
          custom_call, operands);
      fmha_backward.setDropoutRateAttr(
          builder_.getF64FloatAttr(config.dropout_rate()));
      fmha_backward.setSeedAttr(builder_.getI64IntegerAttr(config.seed()));
      return set_common_fmha_backward_attributes(fmha_backward);
    }
    case xla::gpu::CudnnfMHAKind::kBackwardScaleBiasMaskSoftmaxDropout: {
      TF_RETURN_IF_ERROR(GetOrCreateView(custom_call->operand(5), &operands));
      // push fwd output for bwd here if it is flash attention
      if (config.is_flash_attention()) {
        has_bias = true;
        TF_RETURN_IF_ERROR(
            GetOrCreateView(custom_call->operand(6), &operands));  // bias
        TF_RETURN_IF_ERROR(
            GetOrCreateView(custom_call->operand(7), &operands));  // fwd_output
      }
      TF_RETURN_IF_ERROR(GetOrCreateView(custom_call, &operands));
      has_mask = true;
      auto fmha_backward = CreateOpWithoutAttrs<lmhlo_gpu::fusedMHABackwardOp>(
          custom_call, operands);
      fmha_backward.setDropoutRateAttr(
          builder_.getF64FloatAttr(config.dropout_rate()));
      fmha_backward.setSeedAttr(builder_.getI64IntegerAttr(config.seed()));
      return set_common_fmha_backward_attributes(fmha_backward);
    }

    default:
      return xla::InternalError("Unknown backward fused MHA call.");
  }
}

xla::StatusOr<Operation*> LhloDialectEmitter::EmitCubDeviceRadixSort(
    const xla::HloCustomCallInstruction* custom_call) {
  TF_ASSIGN_OR_RETURN(
      auto radix_sort_op,
      CreateOpWithoutAttrs<lmhlo_gpu::RadixSortOp>(custom_call));
  TF_ASSIGN_OR_RETURN(xla::SortOptions options,
                      custom_call->backend_config<xla::SortOptions>());
  radix_sort_op.setDescendingAttr(builder_.getBoolAttr(options.descending()));
  return radix_sort_op.getOperation();
}

// Convert an XLA HLO constant to a global_memref + get_global_memref pair.
tsl::StatusOr<mlir::memref::GetGlobalOp> LhloDialectEmitter::EmitConstant(
    const HloInstruction* instr) {
  auto& instr_slice = instr_slices_[std::make_pair(instr, xla::ShapeIndex())];
  if (instr_slice) {
    return dyn_cast<mlir::memref::GetGlobalOp>(instr_slice.getDefiningOp());
  }

  // Insert a global_memref in the module.
  Location loc = getLocation(instr);

  auto const_instr = xla::Cast<xla::HloConstantInstruction>(instr);
  TF_RET_CHECK(const_instr->shape().IsArray() &&
               const_instr->shape().is_static());
  TF_ASSIGN_OR_RETURN(Type type, xla::ConvertShapeToType<MemRefType>(
                                     const_instr->shape(), builder_));
  auto memref_type = type.dyn_cast<MemRefType>();
  TF_RET_CHECK(memref_type != nullptr);

  TF_ASSIGN_OR_RETURN(
      DenseElementsAttr initial_value,
      CreateDenseElementsAttrFromLiteral(const_instr->literal(), builder_));

  std::string constant_name = xla::llvm_ir::ConstantNameToGlobalName(
      xla::llvm_ir::SanitizeConstantName(instr->name()));

  // Insert the global memref at the top level.
  {
    OpBuilder::InsertionGuard guard(builder_);
    builder_.clearInsertionPoint();
    auto global_var = builder_.create<memref::GlobalOp>(
        loc, constant_name, builder_.getStringAttr("private"), memref_type,
        initial_value, true, /*alignment=*/IntegerAttr());
    symbol_table_.insert(global_var);
    global_var.getOperation()->moveBefore(&module_.front());

    // For operations that do not fold this constant value in their codegen, we
    // still need to materialize it into a buffer. Since buffer allocation is
    // already done, annotate the global_memref with the information to get to
    // the allocated buffer slice for this constant if need be.
    TF_ASSIGN_OR_RETURN(BufferAllocation::Slice slice,
                        assignment_.GetUniqueTopLevelSlice(instr));
    global_var->setAttr(
        "lmhlo.alloc",
        builder_.getIndexAttr(allocations_.find(slice.allocation())
                                  ->second.cast<BlockArgument>()
                                  .getArgNumber()));
    TF_RET_CHECK(slice.offset() == 0)
        << "Each constant should have its own allocation from BufferAssignment";
    TF_RET_CHECK(slice.allocation()->size() == slice.size())
        << "Each constant should have its own allocation from BufferAssignment";
  }

  auto get_global_memref =
      builder_.create<memref::GetGlobalOp>(loc, memref_type, constant_name);

  // Update the cache to remember this value.
  instr_slice = get_global_memref;
  return get_global_memref;
}

namespace {
template <typename OpT>
void SetupChannelIdAttribute(OpT op, const xla::HloChannelInstruction* instr,
                             mlir::Builder builder) {
  if (instr->channel_id().has_value()) {
    op.setChannelIdAttr(mlir::mhlo::ChannelHandleAttr::get(
        builder.getContext(), *instr->channel_id(), 0));
  }
}

template <typename OpT>
tsl::Status SetupCommonCollectiveOpAttributes(OpT op,
                                              const HloInstruction* instr,
                                              mlir::OpBuilder& builder) {
  auto* collective = xla::Cast<xla::HloCollectiveInstruction>(instr);
  auto replica_groups_attr = xla::HloFunctionImporter::ConvertReplicaGroups(
      collective->replica_groups(), &builder);
  op->setAttr(replica_groups_attr.getName(), replica_groups_attr.getValue());
  op.setConstrainLayoutAttr(
      builder.getBoolAttr(collective->constrain_layout()));
  SetupChannelIdAttribute(op, collective, builder);
  return ::tsl::OkStatus();
}
}  // namespace

template <typename OpT>
tsl::StatusOr<OpT> LhloDialectEmitter::EmitDoneOp(
    const xla::HloInstruction* instr) {
  auto token = ret_tokens_.extract(instr->operand(0));
  TF_RET_CHECK(token) << "didn't find " << OpT::getOperationName().str()
                      << " token";
  return builder_.create<OpT>(getLocation(instr), /*resultTypes=*/std::nullopt,
                              token.mapped());
}

tsl::StatusOr<lmhlo_gpu::AllToAllStartOp>
LhloDialectEmitter::EmitAllToAllStartOp(const xla::HloInstruction* instr) {
  // All the input of async-done (which wraps the all-to-all) are also
  // listed as outputs, so we just create operands for the outputs.
  llvm::SmallVector<Value, 4> operands;
  TF_RETURN_IF_ERROR(GetOrCreateView(instr, &operands, /*result_subset=*/{}));

  mlir::Location loc = getLocation(instr);
  mlir::Type token_type = mlir::mhlo::TokenType::get(builder_.getContext());
  std::array<mlir::Type, 1> result_types = {token_type};
  auto all_to_all_start_op =
      builder_.create<lmhlo_gpu::AllToAllStartOp>(loc, result_types, operands);

  auto* all_to_all = xla::Cast<xla::HloAllToAllInstruction>(
      instr->async_wrapped_instruction());
  TF_RETURN_IF_ERROR(SetupCommonCollectiveOpAttributes(all_to_all_start_op,
                                                       all_to_all, builder_));
  if (all_to_all->split_dimension().has_value()) {
    all_to_all_start_op.setSplitDimensionAttr(
        builder_.getI64IntegerAttr(*all_to_all->split_dimension()));
  }
  all_to_all_start_op.setIsSync(IsSyncCollective(instr));
  all_to_all_start_op.setNoParallelCustomCall(
      NoParallelCustomCallCollective(instr));

  auto [_, was_inserted] =
      ret_tokens_.insert({instr, all_to_all_start_op.getToken()});
  TF_RET_CHECK(was_inserted) << "all-to-all-start already lowered";
  return all_to_all_start_op;
}

tsl::StatusOr<lmhlo_gpu::AllToAllDoneOp> LhloDialectEmitter::EmitAllToAllDoneOp(
    const HloInstruction* instr) {
  return EmitDoneOp<lmhlo_gpu::AllToAllDoneOp>(instr);
}

tsl::StatusOr<lmhlo_gpu::AllGatherStartOp>
LhloDialectEmitter::EmitAllGatherStartOp(const HloInstruction* instr) {
  llvm::SmallVector<Value, 4> operands;
  // In all-gather-start HLO, all inputs are also outputs of the HLO. In LMHLO
  // though, we list the inputs and outputs just once. In the HLO result,
  // the inputs are listed first, followed by outputs, which matches the order
  // of operands we need for LMHLO AllGatherOp.
  TF_RETURN_IF_ERROR(GetOrCreateView(instr, &operands, /*result_subset=*/{}));

  mlir::Location loc = getLocation(instr);
  mlir::Type token_type = mlir::mhlo::TokenType::get(builder_.getContext());
  std::array<mlir::Type, 1> result_types = {token_type};
  auto all_gather_start_op =
      builder_.create<lmhlo_gpu::AllGatherStartOp>(loc, result_types, operands);

  auto* all_gather = xla::Cast<xla::HloAllGatherInstruction>(instr);
  TF_RETURN_IF_ERROR(
      SetupCommonCollectiveOpAttributes(all_gather_start_op, instr, builder_));
  all_gather_start_op.setUseGlobalDeviceIdsAttr(
      builder_.getBoolAttr(all_gather->use_global_device_ids()));
  all_gather_start_op.setAllGatherDimensionAttr(
      builder_.getI64IntegerAttr(all_gather->all_gather_dimension()));
  all_gather_start_op.setIsSync(IsSyncCollective(instr));
  all_gather_start_op.setNoParallelCustomCall(
      NoParallelCustomCallCollective(instr));
  auto [_, was_inserted] =
      ret_tokens_.insert({instr, all_gather_start_op.getToken()});
  TF_RET_CHECK(was_inserted) << "all-gather-start already lowered";
  return all_gather_start_op;
}

tsl::StatusOr<lmhlo_gpu::AllGatherDoneOp>
LhloDialectEmitter::EmitAllGatherDoneOp(const HloInstruction* instr) {
  return EmitDoneOp<lmhlo_gpu::AllGatherDoneOp>(instr);
}

tsl::StatusOr<lmhlo_gpu::AllReduceStartOp>
LhloDialectEmitter::EmitAllReduceStartOp(const HloInstruction* instr) {
  llvm::SmallVector<Value, 4> operands;
  for (const HloInstruction* operand : instr->operands()) {
    TF_RETURN_IF_ERROR(GetOrCreateView(operand, &operands));
  }
  TF_RETURN_IF_ERROR(GetOrCreateView(instr, &operands, /*result_subset=*/{}));

  mlir::Location loc = getLocation(instr);
  mlir::Type token_type = mlir::mhlo::TokenType::get(builder_.getContext());
  std::array<mlir::Type, 1> result_types = {token_type};
  auto all_reduce_start_op =
      builder_.create<lmhlo_gpu::AllReduceStartOp>(loc, result_types, operands);

  auto* all_reduce = xla::Cast<xla::HloAllReduceInstruction>(instr);
  TF_RETURN_IF_ERROR(
      SetupCommonCollectiveOpAttributes(all_reduce_start_op, instr, builder_));
  all_reduce_start_op.setUseGlobalDeviceIdsAttr(
      builder_.getBoolAttr(all_reduce->use_global_device_ids()));
  all_reduce_start_op.setIsSync(IsSyncCollective(instr));
  all_reduce_start_op.setNoParallelCustomCall(
      NoParallelCustomCallCollective(instr));

  TF_RETURN_IF_ERROR(xla::HloFunctionImporter::ImportAsRegion(
      *instr->called_computations()[0], symbol_table_,
      &all_reduce_start_op.getComputation(), &builder_));

  auto [_, was_inserted] =
      ret_tokens_.insert({instr, all_reduce_start_op.getToken()});
  TF_RET_CHECK(was_inserted) << "all-reduce-start already lowered";
  return all_reduce_start_op;
}

tsl::StatusOr<lmhlo_gpu::AllReduceDoneOp>
LhloDialectEmitter::EmitAllReduceDoneOp(const HloInstruction* instr) {
  return EmitDoneOp<lmhlo_gpu::AllReduceDoneOp>(instr);
}

tsl::StatusOr<mlir::Operation*> LhloDialectEmitter::EmitAsyncStartOp(
    const xla::HloInstruction* instr) {
  const xla::HloAsyncInstruction* async =
      xla::Cast<xla::HloAsyncInstruction>(instr);

  switch (async->async_wrapped_opcode()) {
    case xla::HloOpcode::kReduceScatter:
      return EmitReduceScatterStartOp(instr);
    case xla::HloOpcode::kAllToAll:
      return EmitAllToAllStartOp(instr);
    default:
      return tsl::errors::InvalidArgument(
          "Unexpected instruction %s wrapped in %s",
          xla::HloOpcodeString(async->async_wrapped_opcode()),
          HloOpcodeString(instr->opcode()));
  }
}

tsl::StatusOr<mlir::Operation*> LhloDialectEmitter::EmitAsyncDoneOp(
    const xla::HloInstruction* instr) {
  const xla::HloAsyncInstruction* async =
      xla::Cast<xla::HloAsyncInstruction>(instr);
  switch (async->async_wrapped_opcode()) {
    case xla::HloOpcode::kReduceScatter:
      return EmitReduceScatterDoneOp(instr);
    case xla::HloOpcode::kAllToAll:
      return EmitAllToAllDoneOp(instr);
    default:
      return tsl::errors::InvalidArgument(
          "Unexpected instruction %s wrapped in %s",
          xla::HloOpcodeString(async->async_wrapped_opcode()),
          HloOpcodeString(instr->opcode()));
  }
}

tsl::StatusOr<lmhlo_gpu::ReduceScatterStartOp>
LhloDialectEmitter::EmitReduceScatterStartOp(const xla::HloInstruction* instr) {
  // All the input of async-done (which wraps the reduce-scatter) are also
  // listed as outputs, so we just create operands for the outputs.
  llvm::SmallVector<Value, 4> operands;
  TF_RETURN_IF_ERROR(GetOrCreateView(instr, &operands, /*result_subset=*/{}));

  mlir::Location loc = getLocation(instr);
  mlir::Type token_type = mlir::mhlo::TokenType::get(builder_.getContext());
  std::array<mlir::Type, 1> result_types = {token_type};
  auto reduce_scatter_start_op =
      builder_.create<lmhlo_gpu::ReduceScatterStartOp>(loc, result_types,
                                                       operands);

  auto* reduce_scatter = xla::Cast<xla::HloReduceScatterInstruction>(
      instr->async_wrapped_instruction());
  TF_RETURN_IF_ERROR(SetupCommonCollectiveOpAttributes(
      reduce_scatter_start_op, reduce_scatter, builder_));
  reduce_scatter_start_op.setUseGlobalDeviceIdsAttr(
      builder_.getBoolAttr(reduce_scatter->use_global_device_ids()));
  reduce_scatter_start_op.setScatterDimensionAttr(
      builder_.getI64IntegerAttr(reduce_scatter->scatter_dimension()));
  reduce_scatter_start_op.setIsSync(IsSyncCollective(instr));
  reduce_scatter_start_op.setNoParallelCustomCall(
      NoParallelCustomCallCollective(instr));
  TF_RETURN_IF_ERROR(xla::HloFunctionImporter::ImportAsRegion(
      *reduce_scatter->to_apply(), symbol_table_,
      &reduce_scatter_start_op.getComputation(), &builder_));

  auto [_, was_inserted] =
      ret_tokens_.insert({instr, reduce_scatter_start_op.getToken()});
  TF_RET_CHECK(was_inserted) << "reduce-scatter-start already lowered";
  return reduce_scatter_start_op;
}

tsl::StatusOr<lmhlo_gpu::ReduceScatterDoneOp>
LhloDialectEmitter::EmitReduceScatterDoneOp(const xla::HloInstruction* instr) {
  return EmitDoneOp<lmhlo_gpu::ReduceScatterDoneOp>(instr);
}

tsl::StatusOr<lmhlo_gpu::CollectivePermuteStartOp>
LhloDialectEmitter::EmitCollectivePermuteStartOp(const HloInstruction* instr) {
  llvm::SmallVector<Value, 2> operands;
  for (const HloInstruction* operand : instr->operands()) {
    TF_RETURN_IF_ERROR(GetOrCreateView(operand, &operands));
  }
  // Ignore the aliased first output and TPU-specific outputs.
  TF_RETURN_IF_ERROR(GetOrCreateView(instr, &operands, /*result_subset=*/{1}));

  mlir::Location loc = getLocation(instr);
  mlir::Type token_type = mlir::mhlo::TokenType::get(builder_.getContext());
  std::array<mlir::Type, 1> result_types = {token_type};
  auto permute_start_op = builder_.create<lmhlo_gpu::CollectivePermuteStartOp>(
      loc, result_types, operands);

  auto* permute = xla::Cast<xla::HloCollectivePermuteInstruction>(instr);
  SetupChannelIdAttribute(permute_start_op, permute, builder_);
  mlir::NamedAttribute source_target_pairs_attr =
      xla::HloFunctionImporter::ConvertSourceTargetPairs(
          permute->source_target_pairs(), &builder_);
  permute_start_op->setAttr(source_target_pairs_attr.getName(),
                            source_target_pairs_attr.getValue());
  permute_start_op.setIsSync(IsSyncCollective(instr));
  permute_start_op.setNoParallelCustomCall(
      NoParallelCustomCallCollective(instr));

  auto [_, was_inserted] =
      ret_tokens_.insert({instr, permute_start_op.getToken()});
  TF_RET_CHECK(was_inserted) << "collective-permute-start already lowered";
  return permute_start_op;
}

tsl::StatusOr<lmhlo_gpu::CollectivePermuteDoneOp>
LhloDialectEmitter::EmitCollectivePermuteDoneOp(const HloInstruction* instr) {
  return EmitDoneOp<lmhlo_gpu::CollectivePermuteDoneOp>(instr);
}

tsl::StatusOr<lmhlo::InfeedOp> LhloDialectEmitter::EmitInfeedOp(
    const HloInstruction* instr) {
  const HloInfeedInstruction* infeed = xla::Cast<HloInfeedInstruction>(instr);
  // HLO Infeed instruction has a single operand of token type and a tuple
  // with buffers and a token as its output. LMHLO Infeed operation does not
  // need the token operand or result, so drop it.
  SmallVector<Value, 2> operands;
  TF_RETURN_IF_ERROR(GetOrCreateView(instr, &operands, /*result_subset=*/{0}));
  auto infeed_op = CreateOpWithoutAttrs<lmhlo::InfeedOp>(instr, operands);
  infeed_op.setConfigAttr(builder_.getStringAttr(infeed->infeed_config()));
  return infeed_op;
}

tsl::StatusOr<lmhlo::OutfeedOp> LhloDialectEmitter::EmitOutfeedOp(
    const HloInstruction* instr) {
  const HloOutfeedInstruction* outfeed =
      xla::Cast<HloOutfeedInstruction>(instr);
  // HLO outfeed instruction has 2 operands, the source and a token, and a
  // single token output. LMHLO Outfeed does not need the token operand and
  // result, do drop it.
  SmallVector<Value, 2> operands;
  TF_RETURN_IF_ERROR(GetOrCreateView(instr->operand(0), &operands));
  auto outfeed_op = CreateOpWithoutAttrs<lmhlo::OutfeedOp>(instr, operands);
  outfeed_op.setConfigAttr(builder_.getStringAttr(outfeed->outfeed_config()));
  return outfeed_op;
}

tsl::StatusOr<lmhlo::RngGetAndUpdateStateOp>
LhloDialectEmitter::EmitRngGetAndUpdateStateOp(
    const xla::HloInstruction* instr) {
  TF_ASSIGN_OR_RETURN(
      auto rng, CreateOpWithoutAttrs<lmhlo::RngGetAndUpdateStateOp>(instr));
  auto hlo_rng = xla::Cast<xla::HloRngGetAndUpdateStateInstruction>(instr);
  rng.setDeltaAttr(builder_.getI64IntegerAttr(hlo_rng->delta()));
  return rng;
}

tsl::StatusOr<lmhlo::FftOp> LhloDialectEmitter::EmitFftOp(
    const HloInstruction* instr) {
  auto hlo_fft = xla::Cast<xla::HloFftInstruction>(instr);
  TF_ASSIGN_OR_RETURN(auto fft, CreateOpWithoutAttrs<lmhlo::FftOp>(instr));
  TF_ASSIGN_OR_RETURN(mlir::mhlo::FftType fft_type,
                      xla::ConvertFftType(hlo_fft->fft_type()));
  fft.setFftTypeAttr(
      mlir::mhlo::FftTypeAttr::get(builder_.getContext(), fft_type));
  fft.setFftLengthAttr(GetI64DenseElementsAttr(instr->fft_length()));
  return fft;
}

tsl::StatusOr<lmhlo::TriangularSolveOp>
LhloDialectEmitter::EmitTriangularSolveOp(const xla::HloInstruction* instr) {
  auto hlo_triangular_solve =
      xla::Cast<xla::HloTriangularSolveInstruction>(instr);
  TF_ASSIGN_OR_RETURN(auto triangular_solve,
                      CreateOpWithoutAttrs<lmhlo::TriangularSolveOp>(instr));
  const xla::TriangularSolveOptions& options =
      hlo_triangular_solve->triangular_solve_options();
  triangular_solve.setLeftSideAttr(builder_.getBoolAttr(options.left_side()));
  triangular_solve.setLowerAttr(builder_.getBoolAttr(options.lower()));
  triangular_solve.setUnitDiagonalAttr(
      builder_.getBoolAttr(options.unit_diagonal()));
  TF_ASSIGN_OR_RETURN(mlir::mhlo::Transpose transpose,
                      xla::ConvertTranspose(options.transpose_a()));
  triangular_solve.setTransposeAAttr(
      mlir::mhlo::TransposeAttr::get(builder_.getContext(), transpose));
  triangular_solve.setLayoutAAttr(
      GetLayoutAttribute(instr->operand(0)->shape().layout(), &builder_));
  triangular_solve.setLayoutBAttr(
      GetLayoutAttribute(instr->operand(1)->shape().layout(), &builder_));
  triangular_solve.setLayoutOutputAttr(
      GetLayoutAttribute(instr->shape().layout(), &builder_));
  return triangular_solve;
}

tsl::StatusOr<Operation*> LhloDialectEmitter::EmitBitcast(
    const xla::HloInstruction* instr) {
  // XLA buffer assignment should assign the same slice to a bitcast input and
  // output.
  const xla::ShapeIndex top_index;
  TF_ASSIGN_OR_RETURN(BufferAllocation::Slice result_slice,
                      assignment_.GetUniqueSlice(instr, top_index));
  TF_ASSIGN_OR_RETURN(BufferAllocation::Slice input_slice,
                      assignment_.GetUniqueSlice(instr->operand(0), top_index));

  if (input_slice != result_slice) {
    return tsl::errors::InvalidArgument(
        "Bitcast input and result slice should be same");
  }
  return nullptr;
}

mlir::DenseIntElementsAttr LhloDialectEmitter::GetLayoutAttribute(
    const xla::Layout& layout, Builder* builder) {
  llvm::SmallVector<int64_t, 4> minor_to_major(layout.minor_to_major().begin(),
                                               layout.minor_to_major().end());
  return builder->getIndexTensorAttr(minor_to_major);
}

tsl::Status LhloDialectEmitter::ImportAsLmhloRegion(
    xla::HloComputation* computation, mlir::Region* region) {
  auto after = builder_.saveInsertionPoint();
  auto reverter = absl::MakeCleanup(
      [this, after] { builder_.restoreInsertionPoint(after); });

  builder_ = OpBuilder(region);
  xla::HloModule* hlo_module = computation->parent();
  if (!hlo_module->has_schedule()) {
    return tsl::errors::Unimplemented(
        "Missing sequential order for the computation");
  }
  const xla::HloInstructionSequence* schedule =
      &hlo_module->schedule().sequence(computation);
  TF_RETURN_IF_ERROR(
      computation->AcceptOrdered(this, schedule->instructions()));
  builder_.create<lmhlo::TerminatorOp>(builder_.getUnknownLoc());
  return ::tsl::OkStatus();
}

tsl::StatusOr<lmhlo::CaseOp> LhloDialectEmitter::EmitCaseOp(
    const HloInstruction* instr) {
  Location loc = getLocation(instr);
  llvm::SmallVector<Value, 4> operands;
  size_t num_arguments, num_results;
  TF_RETURN_IF_ERROR(CreateOperands(instr, 1, TokenLoweringMode::kUseNull,
                                    operands, num_arguments, num_results));

  auto case_op =
      builder_.create<lmhlo::CaseOp>(loc, operands[0], instr->branch_count());

  for (int i = 0; i < instr->branch_count(); i++) {
    case_op.getBranches()[i].push_back(new mlir::Block());
    TF_RETURN_IF_ERROR(ImportAsLmhloRegion(instr->called_computations()[i],
                                           &case_op.getBranches()[i]));
  }

  return case_op;
}

tsl::StatusOr<lmhlo::WhileOp> LhloDialectEmitter::EmitWhileOp(
    const xla::HloInstruction* instr) {
  Location loc = getLocation(instr);
  SmallVector<Value, 1> operands;
  TF_RETURN_IF_ERROR(GetOrCreateView(
      instr->called_computations()[1]->root_instruction(), &operands));
  TF_RET_CHECK(operands.size() == 1);

  TF_ASSIGN_OR_RETURN(auto config,
                      instr->backend_config<xla::WhileLoopBackendConfig>());
  mlir::IntegerAttr trip_count;
  if (config.has_known_trip_count()) {
    trip_count = builder_.getI64IntegerAttr(config.known_trip_count().n());
  }
  lmhlo::WhileOp while_op =
      builder_.create<lmhlo::WhileOp>(loc, operands[0], trip_count);

  while_op.getCond().push_back(new mlir::Block());
  while_op.getBody().push_back(new mlir::Block());
  TF_RETURN_IF_ERROR(ImportAsLmhloRegion(instr->called_computations()[1],
                                         &while_op.getCond()));

  TF_RETURN_IF_ERROR(ImportAsLmhloRegion(instr->called_computations()[0],
                                         &while_op.getBody()));

  return while_op;
}

// TODO(b/264291989): Use enum to define the host transfer type (channel type).
template <typename Instr, typename OpTy>
static void CopyChannelAttrs(OpBuilder& b, Instr* instr, OpTy op,
                             int host_transfer_type) {
  op.setIsHostTransferAttr(b.getBoolAttr(instr->is_host_transfer()));
  op.setChannelHandleAttr(mlir::mhlo::ChannelHandleAttr::get(
      b.getContext(), *instr->channel_id(),
      instr->is_host_transfer() ? host_transfer_type : /*DEVICE_TO_DEVICE*/ 1));
}

template <typename Instr, typename OpTy>
static void CopyFrontendAttrs(OpBuilder& b, Instr* instr, OpTy op) {
  llvm::SmallVector<NamedAttribute> frontend_attrs;
  for (auto& [name, value] : instr->frontend_attributes().map()) {
    frontend_attrs.push_back(b.getNamedAttr(name, b.getStringAttr(value)));
  }
  op->setAttr(b.getStringAttr("frontend_attributes"),
              b.getDictionaryAttr(frontend_attrs));
}

tsl::StatusOr<lmhlo::SendOp> LhloDialectEmitter::EmitSendOp(
    const xla::HloInstruction* instr) {
  llvm::SmallVector<Value, 2> operands;
  TF_RETURN_IF_ERROR(GetOrCreateView(instr->operand(0), &operands));

  auto token = mhlo::TokenType::get(builder_.getContext());
  auto send_op = builder_.create<lmhlo::SendOp>(getLocation(instr),
                                                TypeRange(token), operands);

  // Set point-to-point op communication attributes.
  auto* send = xla::Cast<xla::HloSendInstruction>(instr);
  CopyChannelAttrs(builder_, send, send_op, /*host_transfer_type=*/2);
  CopyFrontendAttrs(builder_, send, send_op);

  auto [_, emplaced] = ret_tokens_.try_emplace(instr, send_op.getToken());
  TF_RET_CHECK(emplaced) << "send already lowered";
  return send_op;
}

tsl::StatusOr<lmhlo::SendDoneOp> LhloDialectEmitter::EmitSendDoneOp(
    const xla::HloInstruction* instr) {
  TF_ASSIGN_OR_RETURN(auto send_done_op, EmitDoneOp<lmhlo::SendDoneOp>(instr));
  // Copy send-done attributes.
  auto* send_done = xla::Cast<xla::HloSendDoneInstruction>(instr);
  CopyChannelAttrs(builder_, send_done, send_done_op,
                   /*host_transfer_type=*/2);

  return send_done_op;
}

tsl::StatusOr<lmhlo::RecvOp> LhloDialectEmitter::EmitRecvOp(
    const xla::HloInstruction* instr) {
  llvm::SmallVector<Value, 2> operands;
  TF_RETURN_IF_ERROR(GetOrCreateView(instr, &operands, {0}));

  auto token = mhlo::TokenType::get(builder_.getContext());
  auto recv_op = builder_.create<lmhlo::RecvOp>(getLocation(instr),
                                                TypeRange(token), operands);

  // Set point-to-point op communication attributes.
  auto* recv = xla::Cast<xla::HloRecvInstruction>(instr);
  CopyChannelAttrs(builder_, recv, recv_op, /*host_transfer_type=*/3);
  CopyFrontendAttrs(builder_, recv, recv_op);

  auto [_, emplaced] = ret_tokens_.try_emplace(instr, recv_op.getToken());
  TF_RET_CHECK(emplaced) << "recv already lowered";
  return recv_op;
}

tsl::StatusOr<lmhlo::RecvDoneOp> LhloDialectEmitter::EmitRecvDoneOp(
    const xla::HloInstruction* instr) {
  TF_ASSIGN_OR_RETURN(auto recv_done_op, EmitDoneOp<lmhlo::RecvDoneOp>(instr));
  // Copy recv-done attributes.
  auto* recv_done = xla::Cast<xla::HloRecvDoneInstruction>(instr);
  CopyChannelAttrs(builder_, recv_done, recv_done_op,
                   /*host_transfer_type=*/3);

  return recv_done_op;
}

tsl::StatusOr<lmhlo::CommandBufferOp> LhloDialectEmitter::EmitCommandBufferOp(
    const xla::HloInstruction* instr) {
  const std::vector<HloComputation*> called_computations =
      instr->called_computations();
  if (called_computations.size() != 1) {
    return absl::InternalError(
        "Command buffer calls must have one called computation");
  }

  if (!absl::StartsWith(called_computations[0]->name(), "command_buffer")) {
    return absl::InternalError("Called computation must be a command buffer");
  }
  return builder_.create<lmhlo::CommandBufferOp>(getLocation(instr));
}

// Sets builder insertion point for a new `memref.view` operation in the parent
// function. We create just one `memref.view` operation for every unique
// subspan of allocation, and because first use of the slice can be inside a
// block nested in a control flow operation, we have to find an insertion point
// in the parent function. Returns insertion guard for the original insertion
// point.
static tsl::StatusOr<OpBuilder::InsertionGuard> SetArrayViewInsertionPoint(
    OpBuilder& builder) {
  OpBuilder::InsertionGuard guard(builder);

  Operation* parent = builder.getInsertionBlock()->getParentOp();
  while (!isa<func::FuncOp>(parent)) {
    builder.setInsertionPoint(parent);
    if ((parent = parent->getParentOp()) == nullptr)
      return absl::InternalError(
          "Can't find an insertion point for memref.view operation");
  }

  return guard;
}

tsl::StatusOr<Value> LhloDialectEmitter::GetOrCreateArrayView(
    const xla::HloInstruction* instr, const xla::Shape& current_shape,
    const xla::ShapeIndex& shape_index) {
  // For constants, the cache is managed inside EmitConstant since it can
  // be called either from here or when we see a top-level HloConstant instr.
  if (instr->IsConstant() && shape_index.empty()) {
    TF_ASSIGN_OR_RETURN(Value constant_memref, EmitConstant(instr));
    return constant_memref;
  }

  // Cache generated ViewOp and StaticMemRefCastOp by (instruction,
  // shape_index).
  auto& instr_slice = instr_slices_[std::make_pair(instr, shape_index)];
  if (instr_slice) {
    return instr_slice;
  }

  TF_ASSIGN_OR_RETURN(BufferAllocation::Slice slice,
                      assignment_.GetUniqueSlice(instr, shape_index));

  // If the shape happens to have dynamic dimensions, create the memref using
  // the underlying static shape.
  // TODO(jurahul): Revisit this when we can model memrefs with dynamic shape
  // but static bounds in MLIR.
  xla::Shape static_shape = xla::ShapeUtil::MakeStaticShape(current_shape);

  // Try to find allocation slice with the same physical shape so that we always
  // have only one memref.view operation covering the same buffer subspan. All
  // reinterpret casts into different layouts will use the same source memref.
  xla::Shape physical_shape =
      xla::ShapeUtil::MakeShapeWithDescendingLayoutAndSamePhysicalLayout(
          static_shape);

  // Initialize values in `allocation_slices_` before taking references,
  // otherwise we can invalidate them and trigger asan errors below.
  auto static_shape_key = std::make_pair(slice, static_shape);
  auto physical_shape_key = std::make_pair(slice, physical_shape);
  allocation_slices_[static_shape_key];
  allocation_slices_[physical_shape_key];

  // Check if we already have a memref.view for a given slice and shape.
  auto& allocation_slice = allocation_slices_[static_shape_key];
  if (allocation_slice) {
    return instr_slice = allocation_slice;
  }

  TF_ASSIGN_OR_RETURN(Type out_type, xla::ConvertShapeToType<MemRefType>(
                                         static_shape, builder_));
  TF_ASSIGN_OR_RETURN(
      Type physical_out_type,
      xla::ConvertShapeToType<MemRefType>(physical_shape, builder_));

  // Try to find an insertion point for a new memref.view operation.
  TF_ASSIGN_OR_RETURN(auto guard, SetArrayViewInsertionPoint(builder_));

  // TODO(timshen): revisit location handling.
  Location loc = builder_.getUnknownLoc();

  // Creates new memref.view operation with a `physical_shape`.
  auto create_physical_slice = [&]() -> Value {
    Value alloc = allocations_[slice.allocation()];
    Value byte_shift =
        builder_.create<arith::ConstantIndexOp>(alloc.getLoc(), slice.offset());

    // ViewOp only takes memrefs without affine maps (layouts). Let ViewOp
    // produce the physical shape (where dimensions are ordered in major to
    // minor) first, then follow up with a MemRefReinterpretCast to cast the
    // resulting memref to the original layout.
    return builder_.create<memref::ViewOp>(loc, physical_out_type, alloc,
                                           byte_shift,
                                           /*sizes=*/ValueRange());
  };

  // Reuse existing physical slice if it exists, otherwise build a new
  // memref.view operation and cache it.
  auto& physical_slice = allocation_slices_[physical_shape_key];
  if (!physical_slice) {
    physical_slice = create_physical_slice();
  }

  // Start from a physical slice as a result, and maybe reinterpret cast it into
  // logical shape.
  Value result = physical_slice;

  if (result.getType() != out_type) {
    int64_t out_offset;
    SmallVector<int64_t, 4> out_strides;
    auto out_memref_type = out_type.dyn_cast<MemRefType>();
    if (!out_memref_type)
      return tsl::errors::Internal(
          "Expected memref type when creating a view for leaf type of a "
          "tuple.");
    if (failed(getStridesAndOffset(out_memref_type, out_strides, out_offset)))
      return tsl::errors::Internal(
          "Failed to get strides and offset from the output type.");
    result = builder_.create<memref::ReinterpretCastOp>(
        loc, out_memref_type, result, out_offset, out_memref_type.getShape(),
        out_strides);
  }

  return instr_slice = allocation_slice = result;
}

tsl::Status LhloDialectEmitter::GetOrCreateViewImpl(
    const HloInstruction* instr, const Shape& current_shape,
    xla::ShapeIndex* current_shape_index, SmallVectorImpl<Value>* values,
    TokenLoweringMode token_mode) {
  if (current_shape.IsTuple()) {
    for (int i = 0; i < current_shape.tuple_shapes().size(); ++i) {
      current_shape_index->push_back(i);
      TF_RETURN_IF_ERROR(
          GetOrCreateViewImpl(instr, current_shape.tuple_shapes(i),
                              current_shape_index, values, token_mode));
      current_shape_index->pop_back();
    }
    return ::tsl::OkStatus();
  }
  if (current_shape.IsArray()) {
    TF_ASSIGN_OR_RETURN(auto v, GetOrCreateArrayView(instr, current_shape,
                                                     *current_shape_index));
    values->push_back(v);
    return ::tsl::OkStatus();
  }
  if (current_shape.IsToken()) {
    switch (token_mode) {
      case TokenLoweringMode::kFailToLower:
        return tsl::errors::Internal(
            "Unexpected token kind for %s and shape index %s",
            instr->ToString(), current_shape_index->ToString());

      case TokenLoweringMode::kUseNull:
        values->push_back(Value{});
        return ::tsl::OkStatus();
    }
  }
  return tsl::errors::Internal(
      "Unexpected shape kind for %s and shape index %s", instr->ToString(),
      current_shape_index->ToString());
}

// Returns a view for the result of an instruction.
// We first get a view for the slice in the allocation, and then may need to
// create another view to adjust the slice for the shape of the instruction.
tsl::Status LhloDialectEmitter::GetOrCreateView(
    const HloInstruction* instr, SmallVectorImpl<Value>* values,
    const xla::ShapeIndex& result_subset, TokenLoweringMode token_mode) {
  xla::ShapeIndex shape_index = result_subset;
  const Shape& sub_shape =
      xla::ShapeUtil::GetSubshape(instr->shape(), shape_index);
  return GetOrCreateViewImpl(instr, sub_shape, &shape_index, values,
                             token_mode);
}

tsl::Status LhloDialectEmitter::Initialize(
    std::vector<const BufferAllocation*>* ordered_allocations) {
  TF_RET_CHECK(computation_.IsEntryComputation());

  mlir::IntegerAttr unique_id =
      builder_.getI32IntegerAttr(computation_.parent()->unique_id());
  module_->setAttr("hlo.unique_id", unique_id);
  llvm::StringRef function_name =
      computation_.name().empty() ? "__compute"
                                  : llvm::StringRef(computation_.name().data(),
                                                    computation_.name().size());

  // Create the function as () -> (), we'll compute the arguments from the
  // buffer allocation and update the type then.
  auto func_op = func::FuncOp::create(builder_.getUnknownLoc(), function_name,
                                      builder_.getFunctionType({}, {}));

  {
    // This is an optional attribute used by the XLA backend. If the resulting
    // LMHLO doesn't go through XLA, this is not needed.
    const Shape& shape = computation_.root_instruction()->shape();
    func_op->setAttr(
        "result_xla_shape",
        builder_.getStringAttr(shape.ToString(/*print_layout=*/true)));
  }
  Block* block = func_op.addEntryBlock();

  for (const BufferAllocation& alloc : assignment_.Allocations()) {
    if (!alloc.is_thread_local()) {
      ordered_allocations->push_back(&alloc);
    }
  }

  if (computation_.IsEntryComputation()) {
    // Sort the rather arbitrarily ordered allocations to match the input/output
    // parameters. Specifically we want to sort buffer allocations in the
    // following order:
    // * Parameters always order before non-parameters.
    // * Different parameters order by parameter number.
    // * Different allocations for the same parameter order by the shape index.
    //
    // TODO(timshen): there should be only one non-parameter buffer, the temp
    // buffer. Check on that.
    const auto allocation_comparator = [](const BufferAllocation* lhs,
                                          const BufferAllocation* rhs) {
      if (lhs->is_entry_computation_parameter() !=
          rhs->is_entry_computation_parameter()) {
        return lhs->is_entry_computation_parameter() >
               rhs->is_entry_computation_parameter();
      }
      if (lhs->is_entry_computation_parameter()) {
        return std::tuple<int, const xla::ShapeIndex&>(
                   lhs->parameter_number(), lhs->param_shape_index()) <
               std::tuple<int, const xla::ShapeIndex&>(
                   rhs->parameter_number(), rhs->param_shape_index());
      }
      return false;
    };

    std::stable_sort(ordered_allocations->begin(), ordered_allocations->end(),
                     allocation_comparator);
  }

  absl::flat_hash_map<const BufferAllocation*,
                      std::pair<const Shape*, xla::ShapeIndex>>
      allocation_to_output_info;
  TF_RETURN_IF_ERROR(xla::ShapeUtil::ForEachSubshapeWithStatus(
      computation_.root_instruction()->shape(),
      [&](const Shape& sub_shape, xla::ShapeIndex index) -> tsl::Status {
        TF_ASSIGN_OR_RETURN(
            auto slice,
            assignment_.GetUniqueSlice(computation_.root_instruction(), index));
        const BufferAllocation* alloc = slice.allocation();
        TF_RET_CHECK(slice.offset() == 0);
        TF_RET_CHECK(slice.size() == alloc->size());
        allocation_to_output_info[alloc] = std::make_pair(&sub_shape, index);
        return ::tsl::OkStatus();
      }));

  // The function signature will be composed of:
  // - one memref for each of the parameters.
  // - one memref for each other buffer allocation.
  llvm::SmallVector<DictionaryAttr, 8> args_attrs;
  auto it = ordered_allocations->begin();
  while (it != ordered_allocations->end()) {
    const BufferAllocation* alloc = *it;
    // There are optional attributes to help the program run through XLA. XLA
    // defines ExecutionInput and ExecutionOutput structures to carry
    // input-output type and buffer information, therefore any information they
    // need (mainly the type structure, potentially containing tuples) to be
    // preserved. They are not needed if the generated LMHLO is not sent to XLA.
    NamedAttrList arg_attr_list;
    mlir::Type arg_type = MemRefType::get({alloc->size()}, i8_type_);

    // Propagate source location information for every HLOInstruction that
    // uses this allocation.
    std::vector<mlir::Location> buf_locs;
    buf_locs.reserve(alloc->assigned_buffers().size());
    for (const auto& entry : alloc->assigned_buffers()) {
      const xla::HloValue* hlo_value = entry.first;
      buf_locs.push_back(getLocation(hlo_value->instruction()));
    }
    mlir::Location loc = builder_.getFusedLoc(buf_locs);

    if (alloc->is_entry_computation_parameter()) {
      arg_attr_list.set("lmhlo.params",
                        builder_.getIndexAttr(alloc->parameter_number()));
      if (!alloc->param_shape_index().empty()) {
        arg_attr_list.set("lmhlo.param_shape_index",
                          builder_.getI64TensorAttr(llvm::ArrayRef(
                              alloc->param_shape_index().begin(),
                              alloc->param_shape_index().end())));
      }
    }
    // Optional: an attribute for optimization. If a kernel uses this
    // allocation, but the allocation has lmhlo.constant_name, then the kernel
    // will instead use the global value indicated by the name for potentially
    // more optimizations (e.g. constant propagation).
    if (alloc->is_constant()) {
      arg_attr_list.set(
          "lmhlo.constant_name",
          builder_.getStringAttr(
              xla::llvm_ir::ConstantBufferAllocationToGlobalName(*alloc)));
    }
    auto iter = allocation_to_output_info.find(alloc);
    if (iter != allocation_to_output_info.end()) {
      const Shape* sub_shape = iter->second.first;
      const xla::ShapeIndex& shape_index = iter->second.second;
      if (!sub_shape->IsArray()) {
        it = ordered_allocations->erase(it);
        continue;
      }
      arg_attr_list.set("lmhlo.output_index",
                        builder_.getI64TensorAttr(llvm::ArrayRef(
                            shape_index.begin(), shape_index.end())));
      if (auto alias = computation_.parent()
                           ->input_output_alias_config()
                           .GetAliasedParameter(shape_index)) {
        if (alias->must_alias()) {
          arg_attr_list.set("lmhlo.must_alias", builder_.getUnitAttr());
        }
      }
    }
    block->addArgument(arg_type, loc);
    allocations_[alloc] = block->getArguments().back();
    args_attrs.push_back(arg_attr_list.getDictionary(builder_.getContext()));
    it++;
  }

  FunctionType function_type =
      builder_.getFunctionType(block->getArgumentTypes(), {});
  func_op.setType(function_type);
  func_op.setAllArgAttrs(args_attrs);

  symbol_table_.insert(func_op);
  builder_.setInsertionPointToEnd(block);

  auto return_op =
      builder_.create<lmhlo::TerminatorOp>(builder_.getUnknownLoc());
  builder_ = OpBuilder(return_op);

  return ::tsl::OkStatus();
}

tsl::Status HloToLhloModule(
    const BufferAssignment& assignment, const HloModule& hlo_module,
    ModuleOp module, std::vector<const BufferAllocation*>* ordered_allocations,
    absl::flat_hash_map<const mlir::Operation*, const xla::HloInstruction*>*
        lhlo_to_hlo_map) {
  module.getContext()
      ->loadDialect<arith::ArithDialect, bufferization::BufferizationDialect,
                    func::FuncDialect, memref::MemRefDialect, mhlo::MhloDialect,
                    lmhlo::LmhloDialect, lmhlo_gpu::LmhloGpuDialect>();

  module->setLoc(mlir::NameLoc::get(
      mlir::StringAttr::get(module.getContext(), hlo_module.name())));

  // Store the HloModule's unique_id in the MLIR module.
  Builder builder(module.getContext());
  module->setAttr("mhlo.unique_id",
                  builder.getI64IntegerAttr(hlo_module.unique_id()));

  const HloComputation* computation = hlo_module.entry_computation();

  LhloDialectEmitter emitter(assignment, *computation, module);
  TF_RETURN_IF_ERROR(emitter.Initialize(ordered_allocations));

  const xla::HloInstructionSequence* schedule =
      &hlo_module.schedule().sequence(computation);

  if (!schedule) {
    return tsl::errors::Unimplemented(
        "Missing sequential order for the computation");
  }
  BaseScopedDiagnosticHandler status_handler(module.getContext());

  const std::vector<HloInstruction*>& ordering = schedule->instructions();
  TF_RETURN_IF_ERROR(computation->AcceptOrdered(&emitter, ordering));
  TF_RETURN_IF_ERROR(status_handler.ConsumeStatus());

  (void)mlir::verify(module);

  if (lhlo_to_hlo_map) {
    auto map = emitter.ConsumeLhloToHloMap();
    std::swap(*lhlo_to_hlo_map, map);
  }
  return status_handler.ConsumeStatus();
}

OwningOpRef<mlir::ModuleOp> HloTextToLhloTranslateFunction(
    llvm::StringRef input, MLIRContext* context) {
  tsl::StatusOr<std::unique_ptr<HloModule>> maybe_module =
      xla::ParseAndReturnUnverifiedModule(
          absl::string_view(input.data(), input.size()));
  TF_CHECK_OK(maybe_module.status());

  OwningOpRef<mlir::ModuleOp> module =
      xla::llvm_ir::CreateMlirModuleOp(UnknownLoc::get(context));

  TF_CHECK_OK(
      ConvertHloToLmhlo(std::move(maybe_module).value(), module.get(), "Host"));

  return module;
}

}  // namespace mlir
