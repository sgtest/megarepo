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
#include "xla/service/gpu/fusions/reduction.h"

#include <array>
#include <cstdint>
#include <functional>
#include <memory>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include "absl/container/flat_hash_map.h"
#include "absl/container/inlined_vector.h"
#include "absl/log/check.h"
#include "absl/strings/str_cat.h"
#include "absl/strings/string_view.h"
#include "absl/types/span.h"
#include "llvm/ADT/STLExtras.h"
#include "llvm/ADT/Twine.h"
#include "llvm/IR/Constants.h"
#include "llvm/IR/DerivedTypes.h"
#include "llvm/IR/GlobalVariable.h"
#include "llvm/IR/IRBuilder.h"
#include "llvm/IR/Instructions.h"
#include "llvm/IR/Type.h"
#include "llvm/IR/Value.h"
#include "llvm/Support/AtomicOrdering.h"
#include "llvm/Support/Casting.h"
#include "mlir/IR/Value.h"  // from @llvm-project
#include "mlir/Support/LLVM.h"  // from @llvm-project
#include "xla/hlo/ir/hlo_casting_utils.h"
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/hlo/ir/hlo_instructions.h"
#include "xla/hlo/ir/hlo_opcode.h"
#include "xla/mlir_hlo/lhlo/IR/lhlo_ops.h"
#include "xla/service/buffer_assignment.h"
#include "xla/service/elemental_ir_emitter.h"
#include "xla/service/gpu/fusions/fusion_emitter.h"
#include "xla/service/gpu/fusions/thunk_util.h"
#include "xla/service/gpu/fusions/tiling_util.h"
#include "xla/service/gpu/gpu_fusible.h"
#include "xla/service/gpu/hlo_fusion_analysis.h"
#include "xla/service/gpu/ir_emission_utils.h"
#include "xla/service/gpu/ir_emitter_context.h"
#include "xla/service/gpu/ir_emitter_nested.h"
#include "xla/service/gpu/kernel_arguments.h"
#include "xla/service/gpu/kernel_mapping_scheme.h"
#include "xla/service/gpu/kernel_reuse_cache.h"
#include "xla/service/gpu/kernel_thunk.h"
#include "xla/service/gpu/launch_dimensions.h"
#include "xla/service/gpu/parallel_loop_emitter.h"
#include "xla/service/gpu/reduction_utils.h"
#include "xla/service/gpu/target_util.h"
#include "xla/service/gpu/thunk.h"
#include "xla/service/llvm_ir/fused_ir_emitter.h"
#include "xla/service/llvm_ir/ir_array.h"
#include "xla/service/llvm_ir/kernel_support_library.h"
#include "xla/service/llvm_ir/llvm_loop.h"
#include "xla/service/llvm_ir/llvm_util.h"
#include "xla/service/llvm_ir/loop_emitter.h"
#include "xla/shape.h"
#include "xla/shape_util.h"
#include "xla/status.h"
#include "xla/status_macros.h"
#include "xla/statusor.h"
#include "tsl/platform/errors.h"
#include "tsl/platform/logging.h"
#include "tsl/platform/status.h"
#include "tsl/platform/statusor.h"

namespace xla {
namespace gpu {
namespace {

using TypedPointer = std::pair<llvm::Value* const, llvm::Type* const>;

// Fusion root -> array of indexes, one per reduction output.
using ReductionOutputMap =
    ConstHloInstructionMap<absl::Span<llvm_ir::IrArray const>>;

using ExtraOutputGensMap = ConstHloInstructionMap<llvm_ir::ElementGenerator>;

int GetNumOutputs(const Shape& shape) {
  if (shape.IsTuple()) {
    return shape.tuple_shapes_size();
  }
  return 1;
}

llvm::Type* GetIndexType(const HloFusionInstruction& fusion,
                         const TilingScheme& tiling_scheme,
                         llvm::IRBuilder<>* builder) {
  return GetIndexTypeForKernel(&fusion,
                               tiling_scheme.GetNumThreadsPerBlockPhysical() *
                                   tiling_scheme.GetNumberOfBlocksPhysical(),
                               builder);
}

class ReductionEmitter {
 public:
  ReductionEmitter(HloFusionAnalysis& analysis,
                   IrEmitterContext& ir_emitter_context,
                   ElementalIrEmitter& elemental_emitter,
                   mlir::lmhlo::FusionOp fusion_op,
                   const HloFusionInstruction& fusion,
                   KernelReuseCache& kernel_cache, llvm::IRBuilder<>* builder)
      : analysis_(analysis),
        ir_emitter_context_(ir_emitter_context),
        elemental_emitter_(elemental_emitter),
        fusion_op_(fusion_op),
        fusion_(fusion),
        kernel_cache_(kernel_cache),
        builder_(builder),
        index_ty_(GetIndexType(
            fusion, analysis.GetReductionCodegenInfo()->GetTilingScheme(),
            builder)) {}

  StatusOr<FusionEmissionResult> Emit();

 private:
  friend class ReductionGroupEmitter;

  StatusOr<std::unique_ptr<Thunk>> BuildKernelThunkForFusion(
      const LaunchDimensions& launch_dimensions,
      absl::string_view discriminator,
      std::function<Status(std::vector<llvm_ir::IrArray>,
                           std::vector<llvm_ir::IrArray>)>
          kernel_builder_fn);

  StatusOr<std::unique_ptr<Thunk>> BuildFusedInitializerThunk(
      const HloInstruction* fusion_root, mlir::Value dest,
      BufferAllocation::Slice dest_slice, int output_index);

  Status EmitIRForReduction(
      absl::Span<const HloInstruction* const> instr_index_group,
      FusedIrEmitter& fused_emitter, const ReductionOutputMap& result_ir_arrays,
      const Shape& input_shape);

  void MaybeEmitFenceForAMDGPU();
  void EmitSyncThreads();

  // For a row reduction, returns the number of rows we can process in parallel
  // per warp.
  int RowReductionGetRowsPerWarp() const {
    int reduced_dimension_size = ReducedDimensionSize();
    if (WarpSize() % reduced_dimension_size != 0 ||
        reduced_dimension_size >= WarpSize()) {
      return 1;
    }
    return WarpSize() / reduced_dimension_size;
  }

  int ReducedDimensionSize() const {
    return analysis_.GetReductionCodegenInfo()
        ->GetTilingScheme()
        .GetDimsInElems()[2];
  }

  HloFusionAnalysis& analysis_;
  IrEmitterContext& ir_emitter_context_;
  ElementalIrEmitter& elemental_emitter_;
  mlir::lmhlo::FusionOp fusion_op_;
  const HloFusionInstruction& fusion_;
  KernelReuseCache& kernel_cache_;
  llvm::IRBuilder<>* builder_;
  llvm::Type* index_ty_;
};

class ReductionGroupEmitter {
 public:
  struct ReductionCalculationState {
    llvm::GlobalVariable* shared_cache;
    llvm::Value* initial_value;
    llvm::AllocaInst* partial_result_address;
    llvm::AllocaInst* input_address;
    llvm_ir::ElementGenerator input_gen;
  };

  ReductionGroupEmitter(
      ReductionEmitter& reduction_emitter,
      absl::Span<const HloReduceInstruction* const> reduce_instr_index_group,
      const ReductionOutputMap& result_ir_arrays,
      FusedIrEmitter& fused_emitter);

  const ReductionCalculationState& GetCalculationStateFor(
      const HloInstruction* instruction, int operand_idx) const {
    const ReductionOpState& op_state = state_.at(instruction);
    CHECK_LT(operand_idx, op_state.size());
    return op_state[operand_idx];
  }

  void SetCalculationStateFor(
      const ReductionCalculationState& calculation_state,
      const HloInstruction* instruction, int operand_idx) {
    ReductionOpState& op_state = state_[instruction];
    CHECK_EQ(operand_idx, op_state.size());
    op_state.push_back(calculation_state);
  }

  void EmitReductionOutputForRowReduction(
      const TilingKernelInfo& tiling_kernel_info,
      const HloReduceInstruction* reduction, const HloInstruction* root,
      int partial_result_idx) const;

  void EmitReductionOutputForColumnReduction(
      const TilingKernelInfo& tiling_kernel_info,
      const HloReduceInstruction* reduction, const HloInstruction* root,
      int partial_result_idx) const;

  void EmitFullWarpShuffleDownLoopForReduce(
      const HloComputation* reducer,
      absl::Span<TypedPointer const> partial_result_addresses,
      int threads_per_block, int num_results_per_warp) const;

  void WriteReductionOutput(const TilingKernelInfo& tiling_kernel_info,
                            const HloReduceInstruction* reduction,
                            const HloInstruction* root, int partial_result_idx,
                            absl::Span<TypedPointer const> values) const;

  llvm_ir::IrArray::Index GetOutputIndexForReduction(
      int partial_result_idx, const TilingKernelInfo& tiling_kernel_info,
      const HloReduceInstruction* reduction, const HloInstruction* root,
      int output_idx) const;

  void GenerateElementForReducer(
      const HloReduceInstruction* reduction, llvm::Value* partial_result_index,
      const llvm_ir::IrArray::Index& index_without_linear,
      const llvm_ir::IrArray::Index& input_index,
      int num_partial_results) const;

  Status EmitExtraOutputsForReduce(
      const Shape& reduction_operand_shape,
      const llvm_ir::IrArray::Index& index,
      const ExtraOutputGensMap& extra_output_gens) const;

 private:
  ReductionEmitter& reduction_emitter_;
  const ReductionOutputMap& result_ir_arrays_;

  // One state per reduction operand.
  using ReductionOpState = absl::InlinedVector<ReductionCalculationState, 2>;

  // HloInstruction -> operand_idx -> cache
  absl::flat_hash_map<const HloInstruction*, ReductionOpState> state_;
};

// Allocates a shared tile of given dimensions, applying scaling specified in
// tilng_scheme as a major-most dimension to avoid collisions.
llvm::GlobalVariable* AllocateShared(
    llvm::IRBuilder<>* builder, const TilingScheme& tiling_scheme,
    llvm::Type* element_type,
    absl::Span<int64_t const> dimensions_major_to_minor,
    absl::string_view buffer_name) {
  CHECK(!dimensions_major_to_minor.empty());
  llvm::Type* ty = element_type;
  for (auto dim : llvm::reverse(dimensions_major_to_minor)) {
    ty = llvm::ArrayType::get(ty, dim);
  }
  ty = llvm::ArrayType::get(ty, tiling_scheme.GetThreadIdScalingFactor());
  return llvm_ir::AllocateSharedMemoryTile(
      builder->GetInsertBlock()->getModule(), ty, buffer_name);
}

// Creates accumulator alloca's, populates them with initial values, generates
// __shared__ caches and returns the populated object.
ReductionGroupEmitter::ReductionGroupEmitter(
    ReductionEmitter& reduction_emitter,
    absl::Span<const HloReduceInstruction* const> reduce_instr_index_group,
    const ReductionOutputMap& result_ir_arrays, FusedIrEmitter& fused_emitter)
    : reduction_emitter_(reduction_emitter),
      result_ir_arrays_(result_ir_arrays) {
  const ReductionCodegenInfo& reduction_info =
      *reduction_emitter_.analysis_.GetReductionCodegenInfo();
  VLOG(10) << "Emit prologue for reduction: "
           << reduction_emitter_.fusion_.ToString();

  auto* builder = reduction_emitter_.builder_;
  for (const HloReduceInstruction* reduce_hlo : reduce_instr_index_group) {
    int num_partial_results = reduction_info.GetNumPartialResults();
    for (int op_result_idx = 0;
         op_result_idx < GetNumOutputs(reduce_hlo->shape()); op_result_idx++) {
      Shape result_shape = reduce_hlo->shape().IsTuple()
                               ? reduce_hlo->shape().tuple_shapes(op_result_idx)
                               : reduce_hlo->shape();

      llvm::Type* element_type = llvm_ir::PrimitiveTypeToIrType(
          result_shape.element_type(), builder->GetInsertBlock()->getModule());
      llvm::AllocaInst* reduction_input_address =
          llvm_ir::EmitAllocaAtFunctionEntry(
              element_type, "reduction_input_address", builder);

      llvm::AllocaInst* partial_result_address =
          llvm_ir::EmitAllocaAtFunctionEntryWithCount(
              element_type,
              /*element_count=*/builder->getInt32(num_partial_results),
              "partial_reduction_result", builder);

      const HloInstruction* init_value =
          reduce_hlo->init_values()[op_result_idx];

      // Initialize the partial result with the initial value of the reduction.
      llvm::Value* init_ir_value = (*fused_emitter.GetGenerator(
          *init_value))(llvm_ir::IrArray::Index(builder->getInt32Ty()))
                                       .value();

      for (int i = 0; i < num_partial_results; ++i) {
        builder->CreateStore(
            init_ir_value, builder->CreateInBoundsGEP(
                               partial_result_address->getAllocatedType(),
                               partial_result_address, {builder->getInt32(i)}));
      }

      const TilingScheme& tiling_scheme = reduction_info.GetTilingScheme();
      llvm::GlobalVariable* shared_cache = [&]() -> llvm::GlobalVariable* {
        if (reduction_info.IsRowReduction()) {
          // Multi-row reductions do not use shared memory.
          if (reduction_emitter_.RowReductionGetRowsPerWarp() > 1) {
            return nullptr;
          }
          // Allocate __shared__
          // cache[num_partial_results][num_warps][scaling_factor].
          CHECK_EQ(tiling_scheme.GetNumThreadsPerBlock() % WarpSize(), 0);
          int num_warps = tiling_scheme.GetNumThreadsPerBlock() / WarpSize();
          return AllocateShared(builder, tiling_scheme, element_type,
                                {num_partial_results, num_warps},
                                "shared_cache");
        } else {
          int64_t num_threads_x =
              tiling_scheme.GetNumThreadsFor(TilingScheme::DimX);
          // Allocate __shared__
          // cache[num_threads][num_threads + 1], where
          // num_threads == num_threads_x == num_threads_y.  The "+1" is used to
          // avoid bank conflicts.
          //
          // (Although each thread produces num_partial_results results, we
          // don't need that much cache: Only one result is live at a time.)
          CHECK_EQ(num_threads_x,
                   tiling_scheme.GetNumThreadsFor(TilingScheme::DimY));
          return AllocateShared(builder, tiling_scheme, element_type,
                                {num_threads_x, num_threads_x + 1},
                                "shared_cache");
        }
      }();

      llvm_ir::ElementGenerator input_gen =
          *fused_emitter.GetGenerator(*reduce_hlo->inputs()[op_result_idx]);
      SetCalculationStateFor(
          {shared_cache, init_ir_value, partial_result_address,
           reduction_input_address, input_gen},
          reduce_hlo, op_result_idx);
    }
  }
}

void ReductionEmitter::MaybeEmitFenceForAMDGPU() {
  auto* module = builder_->GetInsertBlock()->getModule();
  if (IsAMDGPU(module) &&
      ir_emitter_context_.rocm_compute_capability().gcn_arch_name().substr(
          0, 6) == "gfx90a") {
    builder_->CreateFence(
        llvm::AtomicOrdering::SequentiallyConsistent,
        builder_->getContext().getOrInsertSyncScopeID("workgroup"));
  }
}

void ReductionEmitter::EmitSyncThreads() {
  MaybeEmitFenceForAMDGPU();
  EmitCallToTargetIntrinsic(TargetIntrinsicID::kBarrierId, {}, {}, builder_);
}

// Builds a thunk that calls a new or reused kernel for a fusion operation.
//
// The caller must specify the same launch dimensions for fusions which have
// the same computation.
//
// If a given fusion is implemented using multiple kernels, then for each
// kernel we should provide a discriminator, such as "init" and "impl".
//
// The builder_fn is only invoked if the kernel couldn't be reused.
//
// This is the typical usage pattern of this method:
//
// ```
// auto builder_fn = [](std::vector<llvm_ir::IrArray> inputs,
//                      std::vector<llvm_ir::IrArray> outputs) { ... };
// TF_ASSIGN_OR_RETURN(
//   auto thunk,
//   BuildKernelThunkForFusion(..., fusion_op, launch_dimensions, builder_fn,
//                             ...));
// AddThunkToThunkSequence(std::move(thunk))
// ```
StatusOr<std::unique_ptr<Thunk>> ReductionEmitter::BuildKernelThunkForFusion(
    const LaunchDimensions& launch_dimensions, absl::string_view discriminator,
    std::function<Status(std::vector<llvm_ir::IrArray>,
                         std::vector<llvm_ir::IrArray>)>
        kernel_builder_fn) {
  const HloComputation* fused_computation =
      fusion_.fused_instructions_computation();
  std::string suggested_kernel_name = std::string(fusion_.name());

  TF_ASSIGN_OR_RETURN(
      auto kernel_arguments,
      ir_emitter_context_.emit_ir_from_hlo()
          ? KernelArguments::Create(ir_emitter_context_.buffer_assignment(),
                                    &fusion_)
          : KernelArguments::Create(ir_emitter_context_.allocations(),
                                    fusion_op_));

  auto kernel_builder_status = OkStatus();
  auto [entry, cached] = kernel_cache_.Get(
      fused_computation, kernel_arguments.args(), discriminator,
      [&]() -> KernelReuseCache::Entry {
        auto [kernel, input_arrays, output_arrays] = BuildKernelPrototype(
            ir_emitter_context_, suggested_kernel_name, kernel_arguments.args(),
            fusion_.operand_count(), launch_dimensions, builder_);
        kernel_builder_status = kernel_builder_fn(input_arrays, output_arrays);
        return {kernel->getName().str(), launch_dimensions};
      });
  TF_RETURN_IF_ERROR(kernel_builder_status);
  if (cached) {
    VLOG(3) << "Reuse: " << suggested_kernel_name << " -> "
            << entry.kernel_name;
  }

  if (ir_emitter_context_.emit_ir_from_hlo()) {
    return std::make_unique<KernelThunk>(
        &fusion_, entry.kernel_name, kernel_arguments.args(), launch_dimensions,
        // Shared memory is allocated statically.
        /*shmem_bytes=*/0);
  }

  return std::make_unique<KernelThunk>(
      fusion_op_, entry.kernel_name, kernel_arguments.args(), launch_dimensions,
      // Shared memory is allocated statically.
      /*shmem_bytes=*/0);
}

Status ReductionGroupEmitter::EmitExtraOutputsForReduce(
    const Shape& reduction_operand_shape, const llvm_ir::IrArray::Index& index,
    const ExtraOutputGensMap& extra_output_gens) const {
  if (extra_output_gens.empty()) {
    return OkStatus();
  }

  auto* builder = reduction_emitter_.builder_;
  // Compute all extra output values before writing them. This avoids
  // overwriting aliased input/output buffers before all reads occurred.
  std::vector<std::pair<const HloInstruction*, llvm::Value*>>
      extra_output_ir_values;
  extra_output_ir_values.reserve(extra_output_gens.size());

  auto get_index = [&](const HloInstruction* instr) {
    const Shape& s = instr->shape();
    return ShapeUtil::EqualIgnoringElementType(reduction_operand_shape, s)
               ? index
               : index.SourceIndexOfBitcast(reduction_operand_shape, s,
                                            builder);
  };

  for (const auto& [instr, generator] : extra_output_gens) {
    TF_ASSIGN_OR_RETURN(llvm::Value* const extra_output_ir_value,
                        generator(get_index(instr)));
    extra_output_ir_values.emplace_back(instr, extra_output_ir_value);
  }

  for (const auto& [instr, generator] : extra_output_ir_values) {
    absl::Span<llvm_ir::IrArray const> result_ir = result_ir_arrays_.at(instr);
    CHECK_EQ(result_ir.size(), 1);
    result_ir[0].EmitWriteArrayElement(
        get_index(instr), generator, builder, /*use_linear_index=*/
        reduction_emitter_.analysis_.GetReductionCodegenInfo()
                ->GetNumPartialResults() == 1);
  }
  return OkStatus();
}

StatusOr<std::unique_ptr<Thunk>> ReductionEmitter::BuildFusedInitializerThunk(
    const HloInstruction* fusion_root, mlir::Value dest,
    BufferAllocation::Slice dest_slice, int output_index) {
  const HloReduceInstruction* reduce =
      DynCast<HloReduceInstruction>(fusion_root);
  TF_RET_CHECK(reduce);

  const HloInstruction* init_value = reduce->init_values()[0];
  TF_ASSIGN_OR_RETURN(
      std::optional<std::unique_ptr<Thunk>> constant_init_thunk,
      BuildConstantInitializerThunk(ir_emitter_context_, fusion_op_,
                                    fusion_root, init_value, dest, dest_slice));
  if (constant_init_thunk) {
    return *std::move(constant_init_thunk);
  }

  const Shape dest_shape = fusion_root->shape();

  TF_ASSIGN_OR_RETURN(LaunchDimensions launch_dimensions,
                      CalculateLaunchDimensions(
                          dest_shape, ir_emitter_context_.gpu_device_info()));
  const HloComputation* fused_computation =
      fusion_.fused_instructions_computation();

  auto builder_fn = [&](std::vector<llvm_ir::IrArray> inputs,
                        std::vector<llvm_ir::IrArray> outputs) -> Status {
    FusedIrEmitter fused_emitter(elemental_emitter_);
    for (int i = 0; i < fused_computation->num_parameters(); i++) {
      fused_emitter.BindGenerator(
          *fused_computation->parameter_instruction(i),
          [builder = builder_,
           input = inputs[i]](llvm_ir::IrArray::Index index) {
            return input.EmitReadArrayElement(index, builder);
          });
    }
    HloInstruction* instr = fused_computation->root_instruction();
    if (instr->opcode() == HloOpcode::kTuple) {
      instr = instr->mutable_operand(output_index);
    } else {
      CHECK_EQ(0, output_index);
    }
    TF_RET_CHECK(instr->shape().IsArray());
    TF_ASSIGN_OR_RETURN(auto generator,
                        fused_emitter.GetGenerator(*instr->operand(1)));
    TF_RETURN_IF_ERROR(ParallelLoopEmitter(generator, {outputs[output_index]},
                                           launch_dimensions, builder_)
                           .EmitLoop(fusion_.name()));
    return OkStatus();
  };

  return BuildKernelThunkForFusion(launch_dimensions,
                                   /*discriminator=*/
                                   absl::StrCat("init_", output_index),
                                   builder_fn);
}

// Gets the output offset as calculated from thread_id.x (to be applied to the
// offset calculated from block_id and thread_id.y).
static llvm::Value* GetStartOffsetX(const TilingScheme& tiling_scheme,
                                    llvm::Value* thread_id_x,
                                    llvm::Type* index_ty,
                                    llvm::IRBuilder<>* b) {
  int64_t multiplier =
      tiling_scheme.GetIndexingOrder() == TilingScheme::StridedIndexingX
          ? tiling_scheme.GetVectorSize()
          : tiling_scheme.GetTileSizeFor(TilingScheme::DimX);
  return b->CreateMul(thread_id_x,
                      llvm::ConstantInt::get(index_ty, multiplier));
}

// Emits shuffle-down reduction for the `partial_result_address` using the
// reduction computation `reducer`, writes output into
// `partial_result_address`.
//
// Multiple partial_result_address inputs happen when doing variadic
// reduction: each one should get the output value.
void ReductionGroupEmitter::EmitFullWarpShuffleDownLoopForReduce(
    const HloComputation* reducer,
    absl::Span<TypedPointer const> partial_result_addresses,
    int threads_per_block, int num_results_per_warp) const {
  // This only works when the block size is a multiple of 32 threads.
  // We check this here as a mistake in the number of threads per
  // block is very hard to detect.
  CHECK_EQ(threads_per_block % 32, 0);
  CHECK_EQ(WarpSize() % num_results_per_warp, 0);

  auto* builder = reduction_emitter_.builder_;
  for (int distance = 16 / num_results_per_warp; distance >= 1; distance /= 2) {
    absl::InlinedVector<llvm::Value*, 2> reduction_params;

    for (auto acc : partial_result_addresses) {
      reduction_params.push_back(acc.first);
    }

    for (auto [partial_result_address, element_type] :
         partial_result_addresses) {
      int bit_width = llvm_ir::GetSizeInBits(element_type);
      llvm::Value* result_from_other_lane = llvm_ir::EmitAllocaAtFunctionEntry(
          element_type, "result_from_other_lane", builder);

      reduction_params.push_back(result_from_other_lane);

      // Bitcast cannot be applied to aggregate types (even packed ones), so
      // we bitcast addresses of load/store to intN* of the same bit-width.
      llvm::Type* shuffled_value_type = element_type->isStructTy()
                                            ? builder->getIntNTy(bit_width)
                                            : element_type;
      auto convert_pointer_for_shuffle = [&](llvm::Value* ptr) { return ptr; };

      llvm::Value* partial_result = builder->CreateLoad(
          shuffled_value_type,
          convert_pointer_for_shuffle(partial_result_address),
          "partial_reduction_result");
      builder->CreateStore(
          EmitFullWarpShuffleDown(partial_result, builder->getInt32(distance),
                                  builder),
          convert_pointer_for_shuffle(result_from_other_lane));
    }

    StatusOr<std::vector<llvm::Value*>> returned_scalars =
        CallNestedComputationWithScalarAddrs(
            builder, reduction_emitter_.ir_emitter_context_, *reducer,
            reduction_params);
    TF_CHECK_OK(returned_scalars.status());

    for (int i = 0; i < returned_scalars->size(); i++) {
      builder->CreateStore(/*Val=*/returned_scalars->at(i),
                           /*Ptr=*/partial_result_addresses[i].first);
    }
  }
}

llvm_ir::IrArray::Index ReductionGroupEmitter::GetOutputIndexForReduction(
    int partial_result_idx, const TilingKernelInfo& tiling_kernel_info,
    const HloReduceInstruction* reduction, const HloInstruction* root,
    int output_idx) const {
  auto constant = [&](uint64_t c) -> llvm::Constant* {
    return llvm::ConstantInt::get(reduction_emitter_.index_ty_, c);
  };

  auto* builder = reduction_emitter_.builder_;
  const auto& reduction_info =
      *reduction_emitter_.analysis_.GetReductionCodegenInfo();
  const TilingScheme& tiling_scheme = reduction_info.GetTilingScheme();
  const TilingThreadIdInfo& thread_id_info = tiling_kernel_info.thread_id_info;

  llvm_ir::IrArray::Index start_offset = [&] {
    llvm::Value* x_loc = thread_id_info.thread_id_x;
    llvm::Value* y_loc = thread_id_info.thread_id_y;
    if (!reduction_info.IsRowReduction()) {
      std::swap(x_loc, y_loc);
    }
    llvm::Value* start_offset_x = GetStartOffsetX(
        tiling_scheme, x_loc, reduction_emitter_.index_ty_, builder);
    return tiling_kernel_info.tile_origin
        .AddOffsetToDim(y_loc, TilingScheme::DimY, builder)
        .AddOffsetToDim(start_offset_x, TilingScheme::DimX, builder);
  }();

  const Shape& operand_shape = reduction->inputs()[output_idx]->shape();
  Shape reduction_kept_element_shape =
      ShapeUtil::DeleteDimensions(reduction->dimensions(), operand_shape);

  // Given the llvm_ir::IrArray index of a reduction input, returns the linear
  // address of the reduction output as if the reduction were going to keep
  // the input shape with the dimensions being reduced moved.
  llvm::Value* untransposed_output_linear_address = [&] {
    const llvm_ir::IrArray::Index index = start_offset.AddOffsetToDim(
        constant(partial_result_idx), TilingScheme::DimX, builder);
    if (reduction_info.IsRowReduction()) {
      // For row-reduction, y-coordinate determines which row we write into.
      return index[TilingScheme::DimY];
    }
    // For column reduction, we get the transposed address.
    absl::Span<const int64_t> dims_in_elem = tiling_scheme.GetDimsInElems();
    llvm::Value* x_dim_size =
        index.GetConstantWithIndexType(dims_in_elem[TilingScheme::DimX]);
    llvm::Value* x_block_offset =
        builder->CreateMul(index[TilingScheme::DimZ], x_dim_size);
    return builder->CreateAdd(x_block_offset, index[TilingScheme::DimX]);
  }();

  // A reduction is allowed to transpose its output.  For example, suppose
  // we are reducing the second dimension of f32[10,20,30]{3,2,1}.  We are
  // allowed to produce as output either f32[10,30]{1,0} (no transpose) or
  // f32[10,30]{0,1} (transposing the two output dims).
  //
  // At this point in the function we have a "partial sum" of input elements
  // (stored in partial_result_addresses), and we need to accumulate it into
  // the correct output element.
  llvm_ir::IrArray::Index element_index(
      /*linear=*/untransposed_output_linear_address,
      reduction_kept_element_shape, builder);
  const Shape& output_shape = !reduction->shape().IsTuple()
                                  ? reduction->shape()
                                  : reduction->shape().tuple_shapes(output_idx);
  llvm_ir::IrArray::Index output_index(element_index.multidim(), output_shape,
                                       element_index.GetType());
  // We need to check for root == reduction separately, because for variadic
  // reduce the root shape would be a tuple, while 'output_shape' is the
  // subshape.
  return (root == reduction ||
          ShapeUtil::EqualIgnoringElementType(output_shape, root->shape()))
             ? output_index
             : output_index.SourceIndexOfBitcast(output_shape, root->shape(),
                                                 builder);
}

llvm::Value* CastSharedToGlobal(llvm::IRBuilder<>* builder, llvm::Value* input,
                                llvm::Type* element_type, llvm::Twine name) {
  return builder->CreateAddrSpaceCast(
      input,
      llvm::PointerType::get(element_type,
                             /*AddressSpace=*/0),
      name);
}

void ReductionGroupEmitter::WriteReductionOutput(
    const TilingKernelInfo& tiling_kernel_info,
    const HloReduceInstruction* reduction, const HloInstruction* root,
    int partial_result_idx, const absl::Span<TypedPointer const> values) const {
  auto* builder = reduction_emitter_.builder_;
  const auto& reduction_info =
      *reduction_emitter_.analysis_.GetReductionCodegenInfo();
  const HloComputation* reducer = reduction->to_apply();
  for (const auto& [oidx, typed_ptr] : llvm::enumerate(values)) {
    auto [output_ptr, type] = typed_ptr;
    llvm_ir::IrArray::Index output_index = GetOutputIndexForReduction(
        partial_result_idx, tiling_kernel_info, reduction, root, oidx);

    llvm::Value* output_address =
        result_ir_arrays_.at(root)[oidx].EmitArrayElementAddress(
            output_index, builder, "output_element_address");
    if (reduction_info.IsRaceFree()) {
      FusedIrEmitter fused_emitter(reduction_emitter_.elemental_emitter_);
      llvm::Value* loaded = builder->CreateLoad(type, output_ptr, "output");
      fused_emitter.BindGenerator(
          *reduction,
          [&](const llvm_ir::IrArray::Index& index) { return loaded; });
      llvm_ir::ElementGenerator gen = *fused_emitter.GetGenerator(*root);
      llvm::Value* generated = *gen(output_index);
      builder->CreateStore(generated, output_address);
    } else {
      CHECK_EQ(values.size(), 1);
      CHECK_EQ(reduction, root)
          << "output fusion is not allowed for racing reductions";
      TF_CHECK_OK(EmitAtomicOperationForNestedComputation(
          builder, reduction_emitter_.ir_emitter_context_, *reducer,
          output_address, output_ptr, type));
    }
  }
}

// `current_output`: the value the tile has calculated.
// `output_address`: address where the output value has to be written.
void ReductionGroupEmitter::EmitReductionOutputForRowReduction(
    const TilingKernelInfo& tiling_kernel_info,
    const HloReduceInstruction* reduction, const HloInstruction* root,
    int partial_result_idx) const {
  const HloComputation* reducer = reduction->to_apply();
  const auto& thread_id_info = tiling_kernel_info.thread_id_info;
  auto constant = [&](uint64_t c) -> llvm::Constant* {
    return llvm::ConstantInt::get(reduction_emitter_.index_ty_, c);
  };

  auto* builder = reduction_emitter_.builder_;
  auto is_zero = [&](llvm::Value* value) {
    return builder->CreateICmpEQ(value, constant(0));
  };

  int num_outputs = reducer->num_parameters() / 2;
  absl::InlinedVector<TypedPointer, 2> current_outputs;
  for (int output_idx = 0; output_idx < num_outputs; output_idx++) {
    const ReductionGroupEmitter::ReductionCalculationState& state =
        GetCalculationStateFor(reduction, output_idx);
    current_outputs.push_back(
        {builder->CreateInBoundsGEP(
             state.partial_result_address->getAllocatedType(),
             state.partial_result_address, {constant(partial_result_idx)},
             "current_output"),
         state.partial_result_address->getAllocatedType()});
  }

  const auto& reduction_info =
      *reduction_emitter_.analysis_.GetReductionCodegenInfo();
  const TilingScheme& tiling_scheme = reduction_info.GetTilingScheme();
  int num_rows_per_warp = reduction_emitter_.RowReductionGetRowsPerWarp();
  EmitFullWarpShuffleDownLoopForReduce(
      reducer, absl::MakeSpan(current_outputs),
      tiling_scheme.GetNumThreadsPerBlockPhysical(), num_rows_per_warp);

  KernelSupportLibrary ksl(builder);
  llvm::Value* warp_id =
      builder->CreateUDiv(thread_id_info.thread_id_x, constant(WarpSize()));

  auto emit_write_output = [&](llvm::Value* write_condition,
                               const absl::Span<TypedPointer const> values) {
    ksl.If("reduction_write_output", write_condition, [&] {
      WriteReductionOutput(tiling_kernel_info, reduction, root,
                           partial_result_idx, values);
    });
  };

  if (num_rows_per_warp > 1) {
    llvm::Value* is_writing_thread = is_zero(builder->CreateAnd(
        thread_id_info.thread_id_x,
        constant(reduction_emitter_.ReducedDimensionSize() - 1)));
    emit_write_output(is_writing_thread, current_outputs);
    return;
  }

  ksl.If("intra_warp_reduce_write", is_zero(thread_id_info.lane_id), [&] {
    for (int oidx = 0; oidx < num_outputs; oidx++) {
      const auto& state = GetCalculationStateFor(reduction, oidx);
      llvm::Value* shmem_output_addr = thread_id_info.GEPIntoSharedMemory(
          builder, state.shared_cache, {constant(partial_result_idx), warp_id});
      builder->CreateStore(builder->CreateLoad(current_outputs[oidx].second,
                                               current_outputs[oidx].first),
                           shmem_output_addr);
    }
  });

  // TODO(cheshire): Don't we want to sync it once for everything in the
  // output? Not once per each?
  reduction_emitter_.EmitSyncThreads();
  ksl.If("inter_warp_reduce", is_zero(warp_id), [&] {
    absl::InlinedVector<TypedPointer, 2> selected_values;
    for (int oidx = 0; oidx < num_outputs; oidx++) {
      const auto& state = GetCalculationStateFor(reduction, oidx);
      llvm::Value* block_accum_addr = thread_id_info.GEPIntoSharedMemory(
          builder, state.shared_cache,
          {constant(partial_result_idx), thread_id_info.lane_id});

      llvm::Type* element_type =
          state.partial_result_address->getAllocatedType();

      // Ensure initial value address is in generic, not scratch.
      llvm::Value* initial_value_addr =
          CastSharedToGlobal(builder,
                             llvm_ir::EmitAllocaAtFunctionEntry(
                                 element_type, "initial_value_addr", builder),
                             element_type, /*name=*/"");
      builder->CreateStore(state.initial_value, initial_value_addr);

      llvm::Value* warp_exists = builder->CreateICmpULT(
          thread_id_info.thread_id_x,
          constant(tiling_scheme.GetNumThreadsFor(TilingScheme::DimX) /
                   WarpSize()));

      llvm::Value* selected_value = builder->CreateSelect(
          warp_exists, block_accum_addr, initial_value_addr);

      selected_values.push_back({selected_value, element_type});
    }

    // If only one warp is present in the block, then we don't need inter-warp
    // reduction.
    // TODO(b/241414088) If only warp is present, then inter-warp
    // communication using shared memory and synchronization using barrier is
    // also unnecessary and should be removed.
    if (tiling_scheme.GetNumThreadsPerBlock() > WarpSize()) {
      EmitFullWarpShuffleDownLoopForReduce(
          reducer, absl::MakeSpan(selected_values),
          tiling_scheme.GetNumThreadsPerBlock(), /*num_results_per_warp=*/1);
    }

    emit_write_output(is_zero(thread_id_info.thread_id_x), selected_values);
  });
}

// Same arguments as EmitReductionOutputForRowReduction.
void ReductionGroupEmitter::EmitReductionOutputForColumnReduction(
    const TilingKernelInfo& tiling_kernel_info,
    const HloReduceInstruction* reduction, const HloInstruction* root,
    int partial_result_idx) const {
  auto* builder = reduction_emitter_.builder_;
  KernelSupportLibrary ksl(builder);
  const HloComputation* reducer = reduction->to_apply();
  const auto& thread_id_info = tiling_kernel_info.thread_id_info;

  auto constant = [&](uint64_t c) -> llvm::Constant* {
    return llvm::ConstantInt::get(reduction_emitter_.index_ty_, c);
  };
  auto is_zero = [&](llvm::Value* value) {
    return builder->CreateICmpEQ(value, constant(0));
  };
  const auto& reduction_info =
      *reduction_emitter_.analysis_.GetReductionCodegenInfo();
  const TilingScheme& tiling_scheme = reduction_info.GetTilingScheme();
  int num_outputs = reducer->num_parameters() / 2;

  // Wait for reads from shmem in the last iteration to complete.  (If this is
  // slow, we could "double-buffer" by having two shmem buffers and switching
  // between them.)
  if (partial_result_idx > 0) {
    reduction_emitter_.EmitSyncThreads();
  }

  // Store the transpose in shared memory.
  for (int output_idx = 0; output_idx < num_outputs; output_idx++) {
    const auto& state = GetCalculationStateFor(reduction, output_idx);
    llvm::GlobalVariable* shared_cache = state.shared_cache;
    llvm::AddrSpaceCastInst* shmem_output_addr =
        llvm::cast<llvm::AddrSpaceCastInst>(thread_id_info.GEPIntoSharedMemory(
            builder, shared_cache,
            {thread_id_info.thread_id_x, thread_id_info.thread_id_y},
            "shmem_output_address"));
    llvm::Value* current_output = builder->CreateInBoundsGEP(
        state.partial_result_address->getAllocatedType(),
        state.partial_result_address, {constant(partial_result_idx)},
        "current_output");

    llvm::Value* current_output_value = builder->CreateLoad(
        state.partial_result_address->getAllocatedType(), current_output);
    builder->CreateStore(current_output_value, shmem_output_addr);
  }

  reduction_emitter_.EmitSyncThreads();

  // Get transposed element from shared memory.
  absl::InlinedVector<TypedPointer, 2> shmem_transposed_addrs;
  for (int output_idx = 0; output_idx < num_outputs; output_idx++) {
    const auto& state = GetCalculationStateFor(reduction, output_idx);
    llvm::AddrSpaceCastInst* shmem_transposed_addr =
        llvm::cast<llvm::AddrSpaceCastInst>(thread_id_info.GEPIntoSharedMemory(
            builder, state.shared_cache,
            {thread_id_info.thread_id_y, thread_id_info.thread_id_x},
            "shmem_transposed_addr"));
    shmem_transposed_addrs.push_back(
        {shmem_transposed_addr, llvm::cast<llvm::GetElementPtrInst>(
                                    shmem_transposed_addr->getPointerOperand())
                                    ->getResultElementType()});
  }

  EmitFullWarpShuffleDownLoopForReduce(reducer,
                                       absl::MakeSpan(shmem_transposed_addrs),
                                       tiling_scheme.GetNumThreadsPerBlock(),
                                       /*num_results_per_warp=*/1);

  // Some warps in the block are completely outside of the bound of the
  // tensor, so they should not write any output at all.
  llvm::Value* has_output = builder->CreateAnd(
      builder->CreateICmpULT(
          GetStartOffsetX(tiling_scheme, thread_id_info.thread_id_y,
                          reduction_emitter_.index_ty_, builder),
          tiling_kernel_info.output_tile_bounds[1]),
      builder->CreateICmpULT(thread_id_info.thread_id_x,
                             tiling_kernel_info.output_tile_bounds[0]));

  ksl.If("reduction_write_output",
         builder->CreateAnd(has_output, is_zero(thread_id_info.lane_id)), [&] {
           WriteReductionOutput(tiling_kernel_info, reduction, root,
                                partial_result_idx, shmem_transposed_addrs);
         });
}

// Generate a single element of the tile (update the accumulator state) for a
// given reducer of index `i`.
void ReductionGroupEmitter::GenerateElementForReducer(
    const HloReduceInstruction* reduction, llvm::Value* partial_result_index,
    const llvm_ir::IrArray::Index& index_without_linear,
    const llvm_ir::IrArray::Index& input_index, int num_partial_results) const {
  HloComputation* reducer = reduction->to_apply();
  auto* builder = reduction_emitter_.builder_;
  CHECK_EQ(reducer->num_parameters() % 2, 0);

  absl::InlinedVector<llvm::Value*, 2> reduction_accumulators;
  absl::InlinedVector<llvm::Value*, 2> reduction_input_value;
  for (int red_idx = 0; red_idx < reducer->num_parameters() / 2; red_idx++) {
    const auto& state = GetCalculationStateFor(reduction, red_idx);

    llvm::AllocaInst* input_address = state.input_address;
    llvm::AllocaInst* partial_reduction_result_address =
        state.partial_result_address;
    llvm::Value* const input_ir_value = *state.input_gen(
        num_partial_results > 1 ? index_without_linear : input_index);
    builder->CreateStore(input_ir_value, input_address);
    llvm::Value* partial_result_address = builder->CreateInBoundsGEP(
        partial_reduction_result_address->getAllocatedType(),
        partial_reduction_result_address, {partial_result_index});
    reduction_accumulators.push_back(partial_result_address);
    reduction_input_value.push_back(input_address);
  }

  absl::InlinedVector<llvm::Value*, 4> reduction_params;
  for (llvm::Value* acc : reduction_accumulators) {
    reduction_params.push_back(acc);
  }
  for (llvm::Value* value : reduction_input_value) {
    reduction_params.push_back(value);
  }

  // Emit a call to the variadic reducer. Since it may be returning a
  // tuple, we can't return it directly as a value. Instead, before
  // the call, we create N (N = # arguments in the tuple) allocas, one
  // for each returned argument, then when we make the call we pass N
  // pointers as last parameters, the called computation writes into
  // those pointers, and we have returned values on the stack (as well
  // as pointers to them).
  StatusOr<std::vector<llvm::Value*>> returned_scalars =
      CallNestedComputationWithScalarAddrs(
          builder, reduction_emitter_.ir_emitter_context_, *reducer,
          reduction_params);
  TF_CHECK_OK(returned_scalars.status());

  for (int i = 0; i < returned_scalars->size(); i++) {
    builder->CreateStore(returned_scalars->at(i), reduction_accumulators[i]);
  }
}

// Emits code for reductions in the output_instructions.
Status ReductionEmitter::EmitIRForReduction(
    absl::Span<const HloInstruction* const> instr_index_group,
    FusedIrEmitter& fused_emitter, const ReductionOutputMap& result_ir_arrays,
    const Shape& input_shape) {
  const auto& reduction_info = *analysis_.GetReductionCodegenInfo();
  std::vector<const HloInstruction*> roots;
  std::vector<const HloReduceInstruction*> heroes;
  ExtraOutputGensMap extra_output_gens;

  for (const HloInstruction* hlo : instr_index_group) {
    auto& hero = FindNonTrivialHero(*hlo);
    if (IsRealReductionHero(*hlo, hero)) {
      auto reduction = Cast<HloReduceInstruction>(&hero);
      roots.push_back(hlo);
      heroes.push_back(reduction);
    } else {
      extra_output_gens[hlo] = *fused_emitter.GetGenerator(*hlo);
    }
  }

  CHECK(!heroes.empty()) << " expect at least one reduce instructions.";
  const TilingScheme& tiling_scheme = reduction_info.GetTilingScheme();
  CHECK_EQ(tiling_scheme.GetNumThreadsPerBlockPhysical() % WarpSize(), 0);
  ReductionGroupEmitter group_emitter(*this, heroes, result_ir_arrays,
                                      fused_emitter);

  EmitTileElementFunction emit_reduction_element =
      [&](const TilingThreadIdInfo& thread_id_info,
          const llvm_ir::IrArray::Index& index, llvm::Value* y_loc,
          llvm::Value* x_loc) {
        llvm_ir::IrArray::Index input_index = GetUnnormalizedIndex(
            index, input_shape, builder_,
            reduction_info.GetTilingScheme().GetDimsInElems());
        llvm::Value* partial_result_index =
            reduction_info.IsRowReduction()
                ? builder_->getInt32(0)
                : builder_->CreateSub(
                      x_loc,
                      GetStartOffsetX(tiling_scheme, thread_id_info.thread_id_x,
                                      index_ty_, builder_));

        // Clear the linear index field of the llvm_ir::IrArray::Index to enable
        // the use of GetElementPointer with array types. This enables the
        // vectorization of the computation for different partial results. Use
        // this index if 'num_partial_results > 1'.
        int num_partial_results = reduction_info.GetNumPartialResults();
        llvm_ir::IrArray::Index index_without_linear{
            input_index.multidim(), input_shape, input_index.GetType()};

        // Emit code to generate the input and perform the reduction computation
        // for each reduction instruction.
        for (const HloReduceInstruction* reduce : heroes) {
          group_emitter.GenerateElementForReducer(
              reduce, partial_result_index, index_without_linear, input_index,
              num_partial_results);
        }

        // Emit code to generate the output for the non-reduction instructions
        // in the fusion, if any.
        TF_CHECK_OK(group_emitter.EmitExtraOutputsForReduce(
            input_shape, input_index, extra_output_gens));
      };

  TF_ASSIGN_OR_RETURN(
      TilingKernelInfo tiling_kernel_info,
      EmitTilingKernel(builder_, tiling_scheme, index_ty_,
                       [&](const TilingThreadIdInfo& thread_id_info,
                           const llvm_ir::IrArray::Index& index,
                           std::array<llvm::Value*, 2> tile_dimensions) {
                         EmitTile(builder_, reduction_info.GetTilingScheme(),
                                  index, thread_id_info, tile_dimensions,
                                  emit_reduction_element);
                       }));

  KernelSupportLibrary ksl(builder_);
  for (auto [reduce, root] : llvm::zip(heroes, roots)) {
    for (int partial_result_idx = 0;
         partial_result_idx < reduction_info.GetNumPartialResults();
         ++partial_result_idx) {
      if (reduction_info.IsRowReduction()) {
        group_emitter.EmitReductionOutputForRowReduction(
            tiling_kernel_info, reduce, root, partial_result_idx);
      } else {
        group_emitter.EmitReductionOutputForColumnReduction(
            tiling_kernel_info, reduce, root, partial_result_idx);
      }
    }
  }

  return OkStatus();
}

StatusOr<FusionEmissionResult> ReductionEmitter::Emit() {
  auto* reduction_codegen_info = analysis_.GetReductionCodegenInfo();
  TF_ASSIGN_OR_RETURN(auto launch_dimensions, analysis_.GetLaunchDimensions());

  FusionEmissionResult result;
  VLOG(3) << "Launch dimensions of " << fusion_.name() << ": "
          << launch_dimensions.ToString();
  const HloComputation* fused_computation =
      fusion_.fused_instructions_computation();
  if (!reduction_codegen_info->IsRaceFree()) {
    // We need to get the dest slice by traversing the slice assigned to
    // fusion, because instructions inside fusion don't have buffer assignment.
    //
    // The order of fusion roots is determined by its position in the result
    // tuple. For example, in the following fused computation
    //
    // %fused_computation {
    //   %a = ...
    //   &b = ...
    //   ROOT %root = tuple(%a, %b)
    // }
    //
    // The fusion root with index = 0 is %a, and the fusion root %b has index 1.
    // Therefore we can get the ordered slices by calling ForEachSubshape on the
    // result shape.
    std::vector<BufferAllocation::Slice> slices;
    if (ir_emitter_context_.emit_ir_from_hlo()) {
      TF_RETURN_IF_ERROR(ShapeUtil::ForEachSubshapeWithStatus(
          fusion_.shape(), [&](const Shape& subshape, ShapeIndex index) {
            if (!ShapeUtil::IsLeafIndex(fusion_.shape(), index)) {
              return OkStatus();
            }

            TF_ASSIGN_OR_RETURN(
                BufferAllocation::Slice slice,
                ir_emitter_context_.buffer_assignment().GetUniqueSlice(&fusion_,
                                                                       index));
            slices.push_back(slice);
            return OkStatus();
          }));
    }

    absl::Span<const HloInstruction* const> fusion_roots =
        analysis_.fusion_roots();
    for (int i = 0; i < fusion_roots.size(); ++i) {
      const HloInstruction* fusion_root = fusion_roots[i];

      mlir::Value dest = ir_emitter_context_.emit_ir_from_hlo()
                             ? nullptr
                             : fusion_op_.getOutputBuffers()[i];

      BufferAllocation::Slice dest_slice;
      if (ir_emitter_context_.emit_ir_from_hlo()) {
        dest_slice = slices[i];
      } else {
        TF_ASSIGN_OR_RETURN(
            dest_slice,
            GetAllocationSlice(dest, ir_emitter_context_.allocations()));
      }

      if (IsReductionFromOrToContiguousDimensions(*fusion_root)) {
        TF_ASSIGN_OR_RETURN(
            result.thunks.emplace_back(),
            BuildFusedInitializerThunk(fusion_root, dest, dest_slice, i));
      }
    }
  }

  auto builder_fn = [&, this](std::vector<llvm_ir::IrArray> inputs,
                              std::vector<llvm_ir::IrArray> outputs) -> Status {
    FusedIrEmitter fused_emitter(elemental_emitter_);
    for (int i = 0; i < fused_computation->num_parameters(); i++) {
      HloInstruction* fused_operand =
          fused_computation->parameter_instruction(i);
      fused_emitter.BindGenerator(
          *fused_operand,
          [builder = builder_, input = inputs[i],
           fused_operand](const llvm_ir::IrArray::Index& index) {
            return input.EmitReadArrayElement(index, builder,
                                              fused_operand->name());
          });
    }

    // Get outputs.
    ReductionOutputMap result_ir_arrays;

    int ir_arrays_idx = 0;
    for (const HloInstruction* root : analysis_.fusion_roots()) {
      int get_num_results = GetNumOutputs(root->shape());
      result_ir_arrays[root] =
          absl::MakeSpan(outputs).subspan(ir_arrays_idx, get_num_results);
      ir_arrays_idx += get_num_results;
    }

    KernelSupportLibrary ksl(builder_, llvm_ir::UnrollMode::kDefaultUnroll);

    // Use raw block_id_y to select the i-th parallel reduction to run. Using
    // block_id_y instead of block_id_x simplifies the index calculation
    // for reduction code generation as the block_id_y is orthogonal to
    // the indices used within the reductions.
    const std::vector<std::vector<const HloInstruction*>>& instr_index_groups =
        reduction_codegen_info->GetIndexGroups();
    Shape reduce_operand_shape =
        reduction_codegen_info->GetReduceOperandShape();

    llvm::CallInst* raw_block_id_y = gpu::EmitCallToTargetIntrinsic(
        gpu::TargetIntrinsicID::kBlockIdy, {}, {}, builder_);
    llvm_ir::AddRangeMetadata(0, instr_index_groups.size(),
                              llvm::cast<llvm::Instruction>(raw_block_id_y));
    for (int i = 0; i < instr_index_groups.size(); ++i) {
      TF_RETURN_IF_ERROR(ksl.IfWithStatus(
          absl::StrCat("reduce-group-", i),
          builder_->CreateICmpEQ(raw_block_id_y, builder_->getInt32(i)), [&] {
            return EmitIRForReduction(instr_index_groups[i], fused_emitter,
                                      result_ir_arrays, reduce_operand_shape);
          }));
    }

    return OkStatus();
  };

  TF_ASSIGN_OR_RETURN(
      result.thunks.emplace_back(),
      BuildKernelThunkForFusion(launch_dimensions, "", builder_fn));
  return result;
}

}  // namespace

StatusOr<FusionEmissionResult> ReductionFusion::Emit(
    IrEmitterContext& ir_emitter_context, ElementalIrEmitter& elemental_emitter,
    mlir::lmhlo::FusionOp fusion_op, const HloFusionInstruction& fusion,
    KernelReuseCache& kernel_cache, llvm::IRBuilder<>* builder) const {
  return ReductionEmitter(analysis_, ir_emitter_context, elemental_emitter,
                          fusion_op, fusion, kernel_cache, builder)
      .Emit();
}

}  // namespace gpu
}  // namespace xla
