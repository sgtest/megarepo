/* Copyright 2017 The TensorFlow Authors. All Rights Reserved.

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

#include "xla/service/gpu/gpu_compiler.h"

#include <algorithm>
#include <any>
#include <cstdint>
#include <functional>
#include <memory>
#include <optional>
#include <string>
#include <tuple>
#include <utility>
#include <variant>
#include <vector>

#include "absl/log/check.h"
#include "absl/log/log.h"
#include "absl/strings/str_cat.h"
#include "absl/types/variant.h"
#include "llvm/AsmParser/Parser.h"
#include "llvm/IR/DiagnosticInfo.h"
#include "llvm/IR/DiagnosticPrinter.h"
#include "llvm/IR/LLVMContext.h"
#include "llvm/IR/Module.h"
#include "llvm/IR/Verifier.h"
#include "llvm/Support/raw_ostream.h"
#include "llvm/Transforms/Utils/SplitModule.h"
#include "mlir/IR/Diagnostics.h"  // from @llvm-project
#include "xla/hlo/ir/hlo_casting_utils.h"
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/hlo/ir/hlo_instructions.h"
#include "xla/hlo/ir/hlo_module.h"
#include "xla/hlo/ir/hlo_opcode.h"
#include "xla/hlo/ir/hlo_schedule.h"
#include "xla/hlo/transforms/hlo_constant_splitter.h"
#include "xla/mlir/backends/gpu/transforms/passes.h"
#include "xla/mlir/runtime/transforms/compilation_pipeline_gpu.h"
#include "xla/runtime/jit_executable.h"
#include "xla/service/algebraic_simplifier.h"
#include "xla/service/all_gather_broadcast_reorder.h"
#include "xla/service/all_gather_combiner.h"
#include "xla/service/all_reduce_combiner.h"
#include "xla/service/all_reduce_contiguous.h"
#include "xla/service/all_reduce_folder.h"
#include "xla/service/all_reduce_promotion.h"
#include "xla/service/all_reduce_reassociate.h"
#include "xla/service/async_collective_creator.h"
#include "xla/service/batchnorm_expander.h"
#include "xla/service/bitcast_dtypes_expander.h"
#include "xla/service/broadcast_canonicalizer.h"
#include "xla/service/buffer_assignment.h"
#include "xla/service/call_inliner.h"
#include "xla/service/collective_permute_decomposer.h"
#include "xla/service/collective_pipeliner.h"
#include "xla/service/collectives_schedule_linearizer.h"
#include "xla/service/comparison_expander.h"
#include "xla/service/conditional_canonicalizer.h"
#include "xla/service/conditional_simplifier.h"
#include "xla/service/convert_mover.h"
#include "xla/service/convolution_4d_expander.h"
#include "xla/service/convolution_pred_expander.h"
#include "xla/service/copy_insertion.h"
#include "xla/service/dot_decomposer.h"
#include "xla/service/dot_merger.h"
#include "xla/service/dump.h"
#include "xla/service/dynamic_dimension_simplifier.h"
#include "xla/service/dynamic_index_splitter.h"
#include "xla/service/dynamic_padder.h"
#include "xla/service/eigh_expander.h"
#include "xla/service/executable.h"
#include "xla/service/flatten_call_graph.h"
#include "xla/service/float_normalization.h"
#include "xla/service/float_support.h"
#include "xla/service/gather_expander.h"
#include "xla/service/gather_simplifier.h"
#include "xla/service/gpu/alias_passthrough_params.h"
#include "xla/service/gpu/all_reduce_blueconnect.h"
#include "xla/service/gpu/autotuner_util.h"
#include "xla/service/gpu/compile_module_to_llvm_ir.h"
#include "xla/service/gpu/conv_layout_normalization.h"
#include "xla/service/gpu/copy_fusion.h"
#include "xla/service/gpu/dot_dimension_sorter.h"
#include "xla/service/gpu/fusion_pipeline.h"
#include "xla/service/gpu/fusion_wrapper.h"
#include "xla/service/gpu/gemm_broadcast_folding_rewriter.h"
#include "xla/service/gpu/gemm_rewriter.h"
#include "xla/service/gpu/gemm_rewriter_triton.h"
#include "xla/service/gpu/gpu_async_collective_annotator.h"
#include "xla/service/gpu/gpu_constants.h"
#include "xla/service/gpu/gpu_conv_rewriter.h"
#include "xla/service/gpu/gpu_convert_async_collectives_to_sync.h"
#include "xla/service/gpu/gpu_cost_model_stats_collection.h"
#include "xla/service/gpu/gpu_device_info.h"
#include "xla/service/gpu/gpu_executable.h"
#include "xla/service/gpu/gpu_float_support.h"
#include "xla/service/gpu/gpu_hlo_cost_analysis.h"
#include "xla/service/gpu/gpu_hlo_schedule.h"
#include "xla/service/gpu/gpu_layout_assignment.h"
#include "xla/service/gpu/gpu_reduce_scatter_creator.h"
#include "xla/service/gpu/gpu_sanitize_constant_names.h"
#include "xla/service/gpu/gpu_scatter_expander.h"
#include "xla/service/gpu/gpu_shape_verifier.h"
#include "xla/service/gpu/gpu_types.h"
#include "xla/service/gpu/hlo_fusion_stats.h"
#include "xla/service/gpu/horizontal_loop_fusion.h"
#include "xla/service/gpu/ir_emission_utils.h"
#include "xla/service/gpu/loop_double_buffer_transformer.h"
#include "xla/service/gpu/matmul_utils.h"
#include "xla/service/gpu/metrics.h"
#include "xla/service/gpu/move_copy_to_users.h"
#include "xla/service/gpu/prepare_hlo_for_ir_emitting_pipeline.h"
#include "xla/service/gpu/reduction_degenerate_dim_remover.h"
#include "xla/service/gpu/reduction_dimension_grouper.h"
#include "xla/service/gpu/reduction_layout_normalizer.h"
#include "xla/service/gpu/reduction_splitter.h"
#include "xla/service/gpu/reduction_utils.h"
#include "xla/service/gpu/runtime_intrinsics.h"
#include "xla/service/gpu/scatter_slice_simplifier.h"
#include "xla/service/gpu/softmax_rewriter_triton.h"
#include "xla/service/gpu/topk_specializer.h"
#include "xla/service/gpu/topk_splitter.h"
#include "xla/service/gpu/tree_reduction_rewriter.h"
#include "xla/service/hlo.pb.h"
#include "xla/service/hlo_computation_deduplicator.h"
#include "xla/service/hlo_constant_folding.h"
#include "xla/service/hlo_cse.h"
#include "xla/service/hlo_dataflow_analysis.h"
#include "xla/service/hlo_dce.h"
#include "xla/service/hlo_module_config.h"
#include "xla/service/hlo_pass_fix.h"
#include "xla/service/hlo_pass_pipeline.h"
#include "xla/service/hlo_rematerialization.h"
#include "xla/service/hlo_verifier.h"
#include "xla/service/layout_normalization.h"
#include "xla/service/llvm_ir/llvm_util.h"
#include "xla/service/logistic_expander.h"
#include "xla/service/loop_schedule_linearizer.h"
#include "xla/service/operand_upcaster.h"
#include "xla/service/optimization_barrier_expander.h"
#include "xla/service/qr_expander.h"
#include "xla/service/real_imag_expander.h"
#include "xla/service/reduce_decomposer.h"
#include "xla/service/reduce_scatter_combiner.h"
#include "xla/service/reduce_scatter_reassociate.h"
#include "xla/service/reshape_decomposer.h"
#include "xla/service/reshape_mover.h"
#include "xla/service/result_caster.h"
#include "xla/service/rng_bit_generator_expander.h"
#include "xla/service/rng_expander.h"
#include "xla/service/scatter_simplifier.h"
#include "xla/service/sharding_propagation.h"
#include "xla/service/sharding_remover.h"
#include "xla/service/simplify_fp_conversions.h"
#include "xla/service/slice_sinker.h"
#include "xla/service/slow_operation_alarm.h"
#include "xla/service/sort_simplifier.h"
#include "xla/service/spmd/collective_permute_motion.h"
#include "xla/service/spmd/stateful_rng_spmd_partitioner.h"
#include "xla/service/stable_sort_expander.h"
#include "xla/service/stochastic_convert_decomposer.h"
#include "xla/service/topk_rewriter.h"
#include "xla/service/transpose_folding.h"
#include "xla/service/tuple_simplifier.h"
#include "xla/service/while_loop_all_reduce_code_motion.h"
#include "xla/service/while_loop_constant_sinking.h"
#include "xla/service/while_loop_simplifier.h"
#include "xla/service/while_loop_trip_count_annotator.h"
#include "xla/service/zero_sized_hlo_elimination.h"
#include "xla/status_macros.h"
#include "xla/stream_executor/cuda/cuda_platform_id.h"
#include "xla/stream_executor/device_description.h"
#include "xla/stream_executor/device_description.pb.h"
#include "xla/stream_executor/dnn.h"
#include "xla/stream_executor/stream_executor.h"
#include "xla/util.h"
#include "xla/xla.pb.h"
#include "xla/xla_data.pb.h"
#include "tsl/platform/blocking_counter.h"
#include "tsl/platform/casts.h"
#include "tsl/platform/cpu_info.h"
#include "tsl/platform/env.h"
#include "tsl/platform/errors.h"
#include "tsl/platform/statusor.h"
#include "tsl/platform/threadpool.h"
#include "tsl/profiler/lib/traceme.h"

#ifdef PLATFORM_GOOGLE
#include "xla/hlo/experimental/auto_sharding/auto_sharding.h"
#endif  // PLATFORM_GOOGLE

namespace xla {
namespace gpu {
namespace {
bool ConvIsLowerable(HloInstruction* conv) {
  return GpuConvRewriter::ConvIsLowerable(conv);
}

StatusOr<AutotuneConfig> GetAutotuneConfig(
    se::StreamExecutor* stream_exec, const DebugOptions& debug_options,
    const GpuCompiler::CompileOptions& options,
    const GpuTargetConfig& gpu_target_config,
    const AutotuneResults* autotune_results) {
  if (stream_exec) {
    return AutotuneConfig{DeviceConfig{stream_exec, options.device_allocator},
                          debug_options};
  }
  AutotuneConfig deviceless_config =
      AutotuneConfig{DevicelessConfig{gpu_target_config.device_description_str},
                     debug_options};
  // Deviceless config means we can't run autotuning, and need to rely on saved
  // results.
  TF_RETURN_IF_ERROR(AutotunerUtil::LoadAutotuneResults(*autotune_results));
  return deviceless_config;
}
}  // end anonymous namespace

StatusOr<std::unique_ptr<Executable>>
GpuXlaRuntimeAotCompilationResult::LoadExecutable(
    Compiler* compiler, se::StreamExecutor* executor) const {
  XlaRuntimeExecutableProto xla_runtime_executable =
      xla_runtime_gpu_executable_.xla_runtime_executable();
  TF_ASSIGN_OR_RETURN(HloModuleConfig hlo_module_config,
                      HloModule::CreateModuleConfigFromProto(
                          xla_runtime_executable.hlo_module_proto(),
                          GetDebugOptionsFromFlags()));
  TF_ASSIGN_OR_RETURN(
      std::unique_ptr<HloModule> hlo_module,
      HloModule::CreateFromProto(xla_runtime_executable.hlo_module_proto(),
                                 hlo_module_config));
  auto gpu_compiler = tensorflow::down_cast<GpuCompiler*>(compiler);

  std::vector<GpuExecutable::ConstantInfo> constants;
  for (auto& cst : xla_runtime_gpu_executable_.constants()) {
    GpuExecutable::ConstantInfo constant = {
        cst.symbol_name(),
        {cst.content().begin(), cst.content().end()},
        cst.allocation_index()};
    constants.push_back(std::move(constant));
  }

  return GpuExecutable::LoadFromObjFile(
      std::move(hlo_module), xla_runtime_executable.obj_file(),
      xla_runtime_executable.mlir_module(),
      xla_runtime_gpu_executable_.entry_func_attrs(),
      GetDebugOptionsFromFlags(), xla_runtime_gpu_executable_.gpu_asm_text(),
      xla_runtime_gpu_executable_.gpu_binary(), std::move(constants),
      gpu_compiler->GetGpuVersion(executor), executor);
}

GpuCompiler::GpuCompiler(se::Platform::Id platform_id,
                         const char* target_triple, const char* data_layout)
    : platform_id_(platform_id),
      target_triple_(target_triple),
      data_layout_(data_layout),
      pointer_size_(llvm::DataLayout(data_layout)
                        .getPointerSize(0 /* default address space */)) {}

namespace {
// Adds the HloVerifier for GPU to the given pipeline.
void AddHloVerifier(HloPassPipeline* pipeline, HloVerifierOpts&& opts = {},
                    bool debug_only = false) {
  std::unique_ptr<TargetVerifierMetadata> verifier_metadata =
      std::make_unique<GpuVerifierMetadata>(std::move(opts));
  if (debug_only) {
    pipeline->AddInvariantCheckerDebug<HloVerifier>(
        std::move(verifier_metadata), "hlo verifier (debug)");
  } else {
    pipeline->AddInvariantChecker<HloVerifier>(std::move(verifier_metadata),
                                               "hlo verifier");
  }
}

void SetInstructionMetadata(HloModule* module) {
  for (HloComputation* computation : module->computations()) {
    for (HloInstruction* instruction : computation->instructions()) {
      instruction->set_creation_pass_id(-1);
      instruction->set_logical_creation_pass_id(-1);
    }
  }
}
}  // namespace

// Runs optimization passes on the given HLO module.
Status GpuCompiler::OptimizeHloModule(HloModule* hlo_module,
                                      se::StreamExecutor* stream_exec,
                                      const CompileOptions& options,
                                      const GpuTargetConfig& gpu_target_config,
                                      const AutotuneResults* autotune_results) {
  const DebugOptions& debug_options = hlo_module->config().debug_options();

  // By default use an externally provided thread pool.
  tsl::thread::ThreadPool* thread_pool = options.thread_pool;
  std::optional<tsl::thread::ThreadPool> overriding_thread_pool;
  int num_threads = hlo_module->config()
                        .debug_options()
                        .xla_gpu_force_compilation_parallelism();
  // If an external thread pool is provided or single-threaded operation is
  // requested do not create a thread pool.
  if (thread_pool == nullptr && num_threads != 1) {
    // Zero means "default", treat it as "max parallelism" here.
    if (num_threads == 0) {
      num_threads = tsl::port::MaxParallelism();
    }
    overriding_thread_pool.emplace(tsl::Env::Default(), "", num_threads);
    thread_pool = &*overriding_thread_pool;
  }

  AlgebraicSimplifierOptions layout_insensitive_algsimp_opts({},
                                                             ConvIsLowerable);

  // GPU only supports canonical convolutions.
  layout_insensitive_algsimp_opts.set_supports_non_canonical_dots(false);

  // "slow" minmax means we propagate nan.
  layout_insensitive_algsimp_opts.set_minmax_propagate_nan(
      !debug_options.xla_gpu_enable_fast_min_max());

  // Always simplify reduce(transpose(x)) and reduce(reshape(x)), even when
  // the transpose/reshape has multiple users.  This helps int8 models, which
  // tend to have lots of transpose+reshape's (converting between NCHW and
  // NCHW_VECT_C).  Without this, those reshape+transposes can get materialized
  // out, which is really bad for perf.
  layout_insensitive_algsimp_opts
      .set_unconditionally_simplify_reduce_of_transpose_or_reshape(true);

  if (gpu_target_config.platform_name == "ROCM") {
    layout_insensitive_algsimp_opts.set_enable_conv_operand_swap(false);
  }
  layout_insensitive_algsimp_opts
      .set_enable_unconditional_reduce_of_concat_replacement(false);

  SetInstructionMetadata(hlo_module);

  HloPassPipeline pre_spmd_pipeline("pre-spmd-partitioner");
  // Run some IR cleanup passes before running the SPMD partitioning
  // passes.
  pre_spmd_pipeline.AddPass<CallInliner>();
  pre_spmd_pipeline.AddPass<ZeroSizedHloElimination>();
  pre_spmd_pipeline.AddPass<ConditionalCanonicalizer>();

  pre_spmd_pipeline.AddPass<TopkDecomposer>([&](const HloInstruction* instr) {
    return instr->opcode() == HloOpcode::kTopK;
  });

  // The SPMD partitioner would mess up the sort+slice structure, so we need to
  // rewrite Topk before that happens.
  pre_spmd_pipeline.AddPass<TopkRewriter>(
      [](const HloSortInstruction*, int64_t) { return true; });

  TF_RETURN_IF_ERROR(pre_spmd_pipeline.Run(hlo_module).status());

  const int64_t num_partitions = hlo_module->config().num_partitions();
  bool auto_sharding = hlo_module->config().use_auto_spmd_partitioning();

#ifndef PLATFORM_GOOGLE
  if (auto_sharding) {
    LOG(ERROR) << "GPU autosharding is not yet available in open source.";
  }
#endif

  if (num_partitions > 1) {
    if (!hlo_module->config().use_spmd_partitioning()) {
      return InvalidArgument(
          "num_partitions=%d but SPMD partitioning not enabled.",
          num_partitions);
    }
    HloPassPipeline spmd_pipeline("spmd-partitioner");
    HloPassPipeline& spmd_simplify =
        spmd_pipeline.AddPass<HloPassFix<HloPassPipeline>>("spmd-simplify");

    spmd_simplify.AddPass<AlgebraicSimplifier>(layout_insensitive_algsimp_opts);

    spmd_simplify.AddPass<SortSimplifier>();
    spmd_simplify.AddPass<TupleSimplifier>();
    spmd_simplify.AddPass<ScatterSimplifier>();
    spmd_simplify.AddPass<ScatterExpander>(
        ScatterExpander::kEliminateSimpleScatters);
    spmd_simplify.AddPass<GatherSimplifier>();
    spmd_simplify.AddPass<GatherExpander>(
        GatherExpander::kEliminateSimpleGathers);
    spmd_simplify.AddPass<WhileLoopConstantSinking>();
    spmd_simplify.AddPass<WhileLoopSimplifier>();

    ReshapeMoverOptions reshape_mover_options;
    reshape_mover_options.reshape_of_1d_broadcast_is_cheap = true;
    spmd_simplify.AddPass<ReshapeMover>(reshape_mover_options);
    spmd_simplify.AddPass<HloConstantFolding>();
    spmd_simplify.AddPass<ConditionalSimplifier>();
    spmd_simplify.AddPass<HloDCE>();

    spmd_pipeline.AddPass<HloConstantSplitter>();

#ifdef PLATFORM_GOOGLE
    if (auto_sharding) {
      AutoShardingOption option;
      option.enable = true;
      if (!hlo_module->config().auto_spmd_partitioning_mesh_shape().empty()) {
        option.device_mesh_shape =
            hlo_module->config().auto_spmd_partitioning_mesh_shape();
      } else {
        // Use a simple mesh shape if not specified.
        option.device_mesh_shape = {
            gpu_target_config.gpu_device_info.core_count, 1};
      }
      if (!hlo_module->config().auto_spmd_partitioning_mesh_ids().empty()) {
        option.device_mesh_ids =
            hlo_module->config().auto_spmd_partitioning_mesh_ids();
      }
      option.memory_budget_per_device =
          hlo_module->config()
              .debug_options()
              .xla_gpu_auto_spmd_partitioning_memory_budget_gb() *
          1024 * 1024 * 1024;
      option.memory_budget_ratio =
          hlo_module->config()
              .debug_options()
              .xla_gpu_auto_spmd_partitioning_memory_budget_ratio();
      spmd_pipeline.AddPass<AutoSharding>(option);
    }
#endif  // PLATFORM_GOOGLE

    spmd_pipeline.AddPass<ShardingPropagation>(
        /*is_spmd=*/true, /*propagate_metadata=*/false,
        hlo_module->config().allow_spmd_sharding_propagation_to_output());
    spmd_pipeline.AddPass<spmd::StatefulRngSpmdPartitioner>(
        num_partitions, hlo_module->config().replica_count());
    spmd_pipeline.AddPass<CollectivePermuteMotion>();
    TF_RETURN_IF_ERROR(spmd_pipeline.Run(hlo_module).status());
  } else {
    HloPassPipeline sharding_removal_pipeline("sharding-removal");
    // Remove redundant sharding ops when partition_count == 1.
    sharding_removal_pipeline.AddPass<ShardingRemover>();
    sharding_removal_pipeline.AddPass<HloDCE>();
    TF_RETURN_IF_ERROR(sharding_removal_pipeline.Run(hlo_module).status());
  }

  {
    HloPassPipeline pipeline("optimization");
    AddHloVerifier(&pipeline);
    pipeline.AddPass<TopKSplitter>();
    pipeline.AddPass<TopkSpecializer>();
    pipeline.AddPass<TopkDecomposer>();

    HloPredicate upcaster_filter = [&](const HloInstruction* instr) {
      const auto* cuda_cc = std::get_if<se::CudaComputeCapability>(
          &gpu_target_config.gpu_device_info.compute_capability);
      if (cuda_cc != nullptr &&
          !cuda_cc->IsAtLeast(se::CudaComputeCapability::VOLTA)) {
        return true;
      }
      return !gpu::IsMatrixMultiplication(*instr);
    };

    pipeline.AddPass<OperandUpcaster>(upcaster_filter);
    pipeline.AddPass<ResultCaster>(upcaster_filter);

    // Expand random number generation.
    pipeline.AddPass<RngExpander>();
    pipeline.AddPass<RngBitGeneratorExpander>(RandomAlgorithm::RNG_PHILOX);

    // Comparison total order expander
    pipeline.AddPass<ComparisonExpander>();

    // Remove zero-sized HLO from the input so that other passes don't have to
    // handle it.
    pipeline.AddPass<ZeroSizedHloElimination>();

    if (debug_options.xla_gpu_deterministic_ops()) {
      // Scatter can be indeterministic if indices are not unique or a non
      // associative combiner function is used. Eliminate these Scatter ops.
      pipeline.AddPass<ScatterExpander>(
          ScatterExpander::kEliminateIndeterminisitcScatters);
    }
    // Scatters unsupported on XLA:GPU are eliminated.
    pipeline.AddPass<GpuScatterExpander>();

    // TODO(phawkins): replace QR and Eigh decompositions with calls to
    // cuSOLVER.
    pipeline.AddPass<QrExpander>();
    pipeline.AddPass<EighExpander>();

    pipeline.AddPass<DynamicIndexSplitter>();

    // TODO(b/64094172): make Call work on GPU instead of inlining.
    pipeline.AddPass<CallInliner>();

    pipeline.AddPass<DotDimensionSorter>();
    pipeline.AddPass<DotDecomposer>();

    pipeline.AddPass<StochasticConvertDecomposer>();

    pipeline.AddPass<Convolution4DExpander>();

    // Replace PRED convolutions with F16.
    pipeline.AddPass<ConvolutionPredExpander>();

    // Expand the sort op to support stable sorting if required.
    pipeline.AddPass<StableSortExpander>();

    pipeline.AddPass<BatchNormExpander>(
        /*rewrite_training_op=*/true,
        /*rewrite_inference_op=*/true,
        /*rewrite_grad_op=*/true);

    pipeline.AddPass<LogisticExpander>();
    pipeline.AddPass<ConditionalCanonicalizer>();
    pipeline.AddPass<DynamicDimensionSimplifier>();

    DynamicPadderOptions dynamic_padder_options;

    switch (hlo_module->config().debug_options().xla_gpu_shape_checks()) {
      case DebugOptions::IGNORE:
        dynamic_padder_options.shape_check_mode =
            DynamicDimensionInference::ShapeCheckMode::kIgnore;
        break;
      case DebugOptions::RUNTIME: {
        dynamic_padder_options.shape_check_mode =
            DynamicDimensionInference::ShapeCheckMode::kRuntime;
        dynamic_padder_options.assertion_generator = [&](HloInstruction* inst) {
          auto created = Cast<HloCustomCallInstruction>(
              inst->parent()->AddInstruction(HloInstruction::CreateCustomCall(
                  ShapeUtil::MakeTokenShape(), {inst},
                  kXlaGpuAssertCustomCallTag,
                  "Buffers have different size at runtime",
                  API_VERSION_STATUS_RETURNING)));
          created->set_custom_call_has_side_effect(true);
        };
        break;
      }
      case DebugOptions::COMPILE_TIME:
        dynamic_padder_options.shape_check_mode =
            DynamicDimensionInference::ShapeCheckMode::kCompileTime;
        break;
      default:
        LOG(FATAL) << "Unreachable";
    }

    pipeline.AddPass<DynamicPadder>(dynamic_padder_options);

    // Build simplification pipeline.  The passes in here are run to a fixed
    // point.
    [&, &pipeline =
            pipeline.AddPass<HloPassFix<HloPassPipeline>>("simplification")] {
      AddHloVerifier(&pipeline, HloVerifierOpts{}, /*debug_only=*/true);

      // BatchNormExpander can create zero-sized ops, so zero-sized HLO
      // elimination has to come after that pass.
      pipeline.AddPass<ZeroSizedHloElimination>();

      pipeline.AddPass<GatherSimplifier>();
      pipeline.AddPass<GatherExpander>(GatherExpander::kEliminateSimpleGathers);
      pipeline.AddPass<ScatterSimplifier>();
      pipeline.AddPass<ScatterExpander>(
          ScatterExpander::kEliminateSimpleScatters);
      pipeline.AddPass<ScatterSliceSimplifier>();
      pipeline.AddPass<AlgebraicSimplifier>(layout_insensitive_algsimp_opts);
      pipeline.AddPass<BitcastDtypesExpander>();
      // AlgebraicSimplifier may add contracting dimensions to a dot.
      pipeline.AddPass<DotDimensionSorter>();
      pipeline.AddPass<DotDecomposer>();
      // Only merge "smallish" dots.  This threshold was not set carefully, but
      // so far we know that 1mb is too small.
      pipeline.AddPass<DotMerger>(/*max_size_to_merge=*/int64_t{16} << 20);
      pipeline.AddPass<SortSimplifier>();
      pipeline.AddPass<TupleSimplifier>();
      pipeline.AddPass<WhileLoopConstantSinking>();
      pipeline.AddPass<WhileLoopSimplifier>();
      pipeline.AddPass<SliceSinker>();

      ReshapeMoverOptions reshape_mover_options;
      reshape_mover_options.reshape_of_1d_broadcast_is_cheap = true;
      pipeline.AddPass<ReshapeMover>(reshape_mover_options);
      pipeline.AddPass<HloConstantFolding>();
      pipeline.AddPass<ConditionalSimplifier>();
      pipeline.AddPass<RealImagExpander>();
      pipeline.AddPass<TransposeFolding>(CanFoldTransposeOperandIntoDot);
      pipeline.AddPass<HloCSE>(/*is_layout_sensitive=*/false);
      pipeline.AddPass<HloDCE>();
    }();

    // ConvertMover and ReshapeMover fight with each other: ConvertMover wants
    // to move some converts down the graph, but ReshapeMover wants to move them
    // up the graph.  As a compromise, let ReshapeMover run to a fixed point,
    // and then run ConvertMover + algsimp to a fixed point.
    [&, &pipeline =
            pipeline.AddPass<HloPassFix<HloPassPipeline>>("simplification-2")] {
      pipeline.AddPass<ConvertMover>();
      pipeline.AddPass<AlgebraicSimplifier>(layout_insensitive_algsimp_opts);
    }();

    pipeline.AddPass<HloComputationDeduplicator>(
        /*mark_fusion_duplications=*/false);
    TF_RETURN_IF_ERROR(pipeline.Run(hlo_module).status());
  }

  const bool enable_all_pipelined =
      debug_options.xla_gpu_enable_pipelined_collectives();

  // Optimize collectives generated by SPMD partitioning. Enable these passes
  // otherwise as well so that all collectives can get these optimizations.
  {
    HloPassPipeline collectives_pipeline("collective-optimizations");
    collectives_pipeline.AddPass<AllReduceFolder>();
    collectives_pipeline.AddPass<ReduceScatterCreator>();
    collectives_pipeline.AddPass<AllReduceReassociate>(
        debug_options.xla_gpu_enable_reassociation_for_converted_ar());
    collectives_pipeline.AddPass<ReduceScatterReassociate>();
    const DebugOptions& debug_options = hlo_module->config().debug_options();
    collectives_pipeline.AddPass<WhileLoopAllReduceCodeMotion>(
        /*enable_reduce_scatter=*/debug_options
            .xla_gpu_enable_while_loop_reduce_scatter_code_motion());

    if (enable_all_pipelined ||
        debug_options.xla_gpu_enable_pipelined_all_reduce()) {
      CollectivePipeliner::Config config{
          /*level_to_operate_on=*/0,
          /*max_pipelining_per_loop=*/INT64_MAX,
          /*last_run=*/true,
          /*pipeline_use_tree=*/false,
          /*process_different_sized_ops=*/true,
          /*pipelining_direction=*/
          CollectivePipeliner::PipeliningDirection::kForward,
          /*should_process=*/HloPredicateIsOp<HloOpcode::kAllReduce>};
      collectives_pipeline.AddPass<CollectivePipeliner>(config);
    }
    if (enable_all_pipelined ||
        debug_options.xla_gpu_enable_pipelined_all_gather()) {
      CollectivePipeliner::Config config{
          /*level_to_operate_on=*/0,
          /*max_pipelining_per_loop=*/INT64_MAX,
          /*last_run=*/true,
          /*pipeline_use_tree=*/false,
          /*process_different_sized_ops=*/true,
          /*pipelining_direction=*/
          CollectivePipeliner::PipeliningDirection::kBackward,
          /*should_process=*/HloPredicateIsOp<HloOpcode::kAllGather>};
      collectives_pipeline.AddPass<CollectivePipeliner>(config);
    }
    if (enable_all_pipelined ||
        debug_options.xla_gpu_enable_pipelined_reduce_scatter()) {
      CollectivePipeliner::Config config{
          /*level_to_operate_on=*/0,
          /*max_pipelining_per_loop=*/INT64_MAX,
          /*last_run=*/true,
          /*pipeline_use_tree=*/false,
          /*process_different_sized_ops=*/true,
          /*pipelining_direction=*/
          CollectivePipeliner::PipeliningDirection::kForward,
          /*should_process=*/HloPredicateIsOp<HloOpcode::kReduceScatter>};
      collectives_pipeline.AddPass<CollectivePipeliner>(config);
    }

    // Run algebraic simplifier to reshape(broadcast) into a broadcast when
    // the reshape is just adding a unit dimension. This will help with the
    // AllGatherBroadcastReorder pass.
    collectives_pipeline.AddPass<AlgebraicSimplifier>(
        layout_insensitive_algsimp_opts);

    collectives_pipeline.AddPass<AllGatherBroadcastReorder>();

    // promote 16 bit integer all-reduce and reduce-scatter to 32-bit.
    const std::pair<PrimitiveType, PrimitiveType> ar_promoted_types[] = {
        {U16, U32}, {S16, S32}};
    collectives_pipeline.AddPass<AllReducePromotion>(ar_promoted_types);
    // Remove dead computations left over after ar/rs promotion.
    collectives_pipeline.AddPass<HloDCE>();

    // Run WhileLoopTripCountAnnotator after collective pipelining and before
    // layout assignment and fusion.This pass does some pattern-matching on
    // while bodies/conditions, and this is where the HLO is "nicest".
    //
    // It's important that we don't make semantic changes (e.g. unrolling) to
    // any `while` loops after this point, because otherwise the trip-count
    // annotations added by this pass may not be correct after the
    // modifications.
    collectives_pipeline.AddPass<WhileLoopTripCountAnnotator>();

    TF_RETURN_IF_ERROR(collectives_pipeline.Run(hlo_module).status());
  }

  // Run target-specific HLO optimization passes for convolution
  // canonicalization.
  GpuVersion gpu_version = gpu_target_config.gpu_device_info.compute_capability;
  se::dnn::VersionInfo dnn_version = gpu_target_config.dnn_version_info;
  if (stream_exec != nullptr) {
    gpu_version = GetGpuVersion(stream_exec);
    se::dnn::DnnSupport* dnn = stream_exec->AsDnn();
    if (dnn == nullptr) {
      return tsl::errors::FailedPrecondition(
          "DNN library initialization failed."
          " Look at the errors above for more details.");
    }
    TF_ASSIGN_OR_RETURN(dnn_version, dnn->GetVersion());
  }

  TF_RETURN_IF_ERROR(OptimizeHloConvolutionCanonicalization(
      hlo_module, gpu_version, dnn_version, options.device_allocator));

  {
    // Run layout assignment in a separate pipeline from
    // "post-layout-assignment" because we want everything after layout
    // assignment to have a layout-sensitive invariant-checker, but
    // HloPassPipeline also runs its invariant checker before any passes are
    // run, meaning, the pipeline that contains layout assignment cannot contain
    // a layout-sensitive verifier!
    HloPassPipeline pipeline("layout assignment");
    // Layout assignment uses alias analysis, which requires the call graph to
    // be flattened.
    pipeline.AddPass<FlattenCallGraph>();
    ChannelLayoutConstraints layout_constraints;
    pipeline.AddPass<GpuLayoutAssignment>(
        hlo_module->mutable_entry_computation_layout(), stream_exec,
        &layout_constraints);
    TF_RETURN_IF_ERROR(pipeline.Run(hlo_module).status());
  }

  // Run target-specific HLO optimization passes after layout assignment.
  TF_RETURN_IF_ERROR(OptimizeHloPostLayoutAssignment(
      hlo_module, stream_exec, options, gpu_target_config, autotune_results,
      thread_pool));

  const GpuDeviceInfo& gpu_device_info = gpu_target_config.gpu_device_info;

  TF_RETURN_IF_ERROR(
      FusionPipeline(debug_options, ShapeSizeBytesFunction(), gpu_device_info)
          .Run(hlo_module)
          .status());

  if (debug_options.xla_gpu_collect_cost_model_stats()) {
    GpuHloCostAnalysis::Options cost_analysis_options{
        ShapeSizeBytesFunction(),
        /*per_second_rates=*/{},
        /*count_multiple_input_accesses=*/true};

    HloPassPipeline post_fusion_analysis("post_fusion_analysis");
    post_fusion_analysis.AddPass<GpuCostModelStatsCollection>(
        gpu_device_info, cost_analysis_options);
    TF_RETURN_IF_ERROR(post_fusion_analysis.Run(hlo_module).status());
  }

  TF_RETURN_IF_ERROR(
      HorizontalFusionPipeline(gpu_device_info).Run(hlo_module).status());

  if (VLOG_IS_ON(2)) {
    HloFusionStatsVisitor stats;
    TF_RETURN_IF_ERROR(hlo_module->entry_computation()->Accept(&stats));
    VLOG(2) << stats.ToString();
  }

  {
    HloPassPipeline pipeline("post-fusion optimization");
    pipeline.AddPass<AllGatherCombiner>(
        debug_options.xla_gpu_all_gather_combine_threshold_bytes(),
        /*combine_threshold_count=*/256);
    pipeline.AddPass<AllReduceCombiner>(
        debug_options.xla_gpu_all_reduce_combine_threshold_bytes(),
        /*combine_threshold_count=*/256);
    pipeline.AddPass<ReduceScatterCombiner>(
        debug_options.xla_gpu_reduce_scatter_combine_threshold_bytes(),
        /*combine_threshold_count=*/256);

    if (debug_options.xla_gpu_all_reduce_contiguous()) {
      pipeline.AddPass<AllReduceContiguous>();
    }

    TF_RETURN_IF_ERROR(AddHloEmitterAutotuningPasses(
        &pipeline, stream_exec, debug_options, options, gpu_target_config,
        autotune_results, thread_pool));

    int32_t blueconnect_num_devices_per_host =
        debug_options.xla_gpu_all_reduce_blueconnect_num_devices_per_host();
    if (blueconnect_num_devices_per_host > 0) {
      pipeline.AddPass<AllReduceBlueConnect>(blueconnect_num_devices_per_host);
    }

    if (debug_options.xla_gpu_enable_while_loop_double_buffering()) {
      pipeline.AddPass<LoopDoubleBufferTransformer>();
      pipeline.AddPass<TupleSimplifier>();
      pipeline.AddPass<HloDCE>();
    }

    {
      // Convert all collectives to their async form, and then annotate the ones
      // that actually need to run asynchronously with a GPU specific backend
      // config.
      AsyncCollectiveCreator::CollectiveCreatorConfig config;
      config.convert_all_reduce = HloPredicateTrue;
      config.convert_collective_permute = HloPredicateTrue;
      config.convert_all_gather = HloPredicateTrue;
      config.convert_reduce_scatter = HloPredicateTrue;
      config.convert_all_to_all = HloPredicateTrue;
      pipeline.AddPass<AsyncCollectiveCreator>(std::move(config));

      auto convert_to_async = [&debug_options](const HloInstruction* inst) {
        const bool enable_all_async =
            debug_options.xla_gpu_enable_async_collectives();
        switch (inst->opcode()) {
          case HloOpcode::kAllReduceStart:
            return enable_all_async ||
                   debug_options.xla_gpu_enable_async_all_reduce();
          case HloOpcode::kAllGatherStart:
            return enable_all_async ||
                   debug_options.xla_gpu_enable_async_all_gather();
          case HloOpcode::kCollectivePermuteStart:
            return enable_all_async ||
                   debug_options.xla_gpu_enable_async_collective_permute();
          case HloOpcode::kAsyncStart: {
            auto async_inst = Cast<HloAsyncInstruction>(inst);
            switch (async_inst->async_wrapped_opcode()) {
              case HloOpcode::kReduceScatter:
                return enable_all_async ||
                       debug_options.xla_gpu_enable_async_reduce_scatter();
              case HloOpcode::kAllToAll:
                return enable_all_async ||
                       debug_options.xla_gpu_enable_async_all_to_all();
              default:
                return false;
            }
          }
          default:
            return false;
        }
      };
      pipeline.AddPass<GpuAsyncCollectiveAnnotator>(convert_to_async);
    }
    pipeline.AddPass<CollectivePermuteDecomposer>(
        debug_options.xla_gpu_collective_permute_decomposer_threshold());

    if (enable_all_pipelined || debug_options.xla_gpu_enable_pipelined_p2p()) {
      auto may_pipeline_p2p = [](const HloInstruction* instruction) {
        const HloRecvDoneInstruction* recv_done =
            DynCast<const HloRecvDoneInstruction>(instruction);
        if (!recv_done || recv_done->is_host_transfer()) return false;
        // Check that the recv-done is used for non-trivial computation, which
        // can also help avoid repeatedly pipelining a loop.
        return recv_done->user_count() == 1 && recv_done->parent() != nullptr &&
               recv_done->users()[0] != recv_done->parent()->root_instruction();
      };
      // We curretly use one asynchronous stream to execute P2P operations,
      // as such, can only support pipelining at most one P2P chain in each
      // loop.
      CollectivePipeliner::Config config{
          /*level_to_operate_on=*/0,
          /*max_pipelining_per_loop=*/1,
          /*last_run=*/true,
          /*pipeline_use_tree=*/false,
          /*process_different_sized_ops=*/true,
          /*pipelining_direction=*/
          CollectivePipeliner::PipeliningDirection::kBackward,
          /*should_process=*/may_pipeline_p2p};
      pipeline.AddPass<CollectivePipeliner>(config);
    }

    AlgebraicSimplifierOptions options = layout_insensitive_algsimp_opts;
    options.set_is_layout_sensitive(true);
    pipeline.AddPass<AlgebraicSimplifier>(options);

    // This invocation is used to populate deduplicated_name for fusions that
    // are considered duplicates according to the comparator in this pass.
    // Currently, the pass doesn't actually deduplicate the fusions.
    pipeline.AddPass<HloComputationDeduplicator>(
        /*mark_fusion_duplications=*/true);

    TF_RETURN_IF_ERROR(pipeline.Run(hlo_module).status());
  }

  return OkStatus();
}

// Modifies the given HLO module so that it will be accepted by IrEmitter.
// Unlike optimization passes, the passes are necessary for correctness.
Status GpuCompiler::PrepareHloModuleForIrEmitting(HloModule* hlo_module) {
  return PrepareHloModuleForIrEmittingPipeline(*hlo_module, GetCanShareBuffer())
      .Run(hlo_module)
      .status();
}

Status GpuCompiler::OptimizeHloPostLayoutAssignment(
    HloModule* hlo_module, se::StreamExecutor* stream_exec,
    const CompileOptions& options, const GpuTargetConfig& gpu_target_config,
    const AutotuneResults* autotune_results,
    tsl::thread::ThreadPool* thread_pool) {
  // Constants:
  const DebugOptions& debug_options = hlo_module->config().debug_options();
  const GpuVersion gpu_version =
      gpu_target_config.gpu_device_info.compute_capability;
  const se::CudaComputeCapability* const cuda_cc =
      std::get_if<se::CudaComputeCapability>(&gpu_version);
  const AlgebraicSimplifierOptions simplifier_options = [&] {
    AlgebraicSimplifierOptions opts;
    opts.set_supports_non_canonical_dots(false);
    opts.set_is_layout_sensitive(true);
    opts.set_enable_conv_operand_swap(false);
    // "slow" minmax means we propagate nan.
    opts.set_minmax_propagate_nan(!debug_options.xla_gpu_enable_fast_min_max());
    opts.set_enable_unconditional_reduce_of_concat_replacement(false);
    return opts;
  }();
  TF_ASSIGN_OR_RETURN(AutotuneConfig autotune_config,
                      GetAutotuneConfig(stream_exec, debug_options, options,
                                        gpu_target_config, autotune_results));
  // Lambdas and related constants:
  const GpuFloatSupport bf16_support(BF16);
  const GpuFloatSupport f8e5m2_support(F8E5M2);
  const GpuFloatSupport f8e4m3fn_support(F8E4M3FN);
  const FloatSupport f8e4m3b11fnuz_support(F8E4M3B11FNUZ);
  const FloatSupport f8e5m2fnuz_support(F8E5M2FNUZ);
  const FloatSupport f8e4m3fnuz_support(F8E4M3FNUZ);
  auto add_float_normalization = [&](HloPassPipeline& pipeline) {
    auto& sub_pipeline =
        pipeline.AddPass<HloPassPipeline>("float_normalization");
    sub_pipeline.AddPass<FloatNormalization>(&bf16_support);
    sub_pipeline.AddPass<FloatNormalization>(&f8e5m2_support);
    sub_pipeline.AddPass<FloatNormalization>(&f8e4m3fn_support);
    sub_pipeline.AddPass<FloatNormalization>(&f8e4m3b11fnuz_support);
    sub_pipeline.AddPass<FloatNormalization>(&f8e5m2fnuz_support);
    sub_pipeline.AddPass<FloatNormalization>(&f8e4m3fnuz_support);
    // Remove `f32 -> bf16 -> f32` casts inserted by bf16 normalization.
    if (debug_options.xla_gpu_simplify_all_fp_conversions()) {
      sub_pipeline.AddPass<SimplifyFPConversions>(
          SimplifyFPConversions::Scope::kSimplifyAllConversions);
    }
  };

  {
    HloPassPipeline pipeline("hlo normalization");

    // The LayoutAssignment pass may leave behind kCopy instructions which are
    // duplicate or NOPs, so remove them with algebraic simplification and CSE.
    pipeline.AddPass<HloPassFix<AlgebraicSimplifier>>(simplifier_options);

    // GemmRewriter assumes that all transposes are folded into gemms, but,
    // since commit 7d529df, this is not always true at this point.
    // Therefore, rerun transpose folding.
    pipeline.AddPass<TransposeFolding>(CanFoldTransposeOperandIntoDot,
                                       TransposeFolding::NeverFoldTranspose);

    pipeline.AddPass<ReshapeDecomposer>();
    pipeline.AddPass<ReduceDecomposer>([&](const HloInstruction* r) {
      return IsReductionFromOrToContiguousDimensions(*r);
    });
    pipeline.AddPass<HloPassFix<MoveCopyToUsers>>();

    // Rewrite GEMMs into custom calls.
    if (debug_options.xla_gpu_enable_triton_gemm() && cuda_cc != nullptr &&
        cuda_cc->IsAtLeast(se::CudaComputeCapability::VOLTA)) {
      pipeline.AddPass<GemmRewriterTriton>(gpu_version);
    }
    pipeline.AddPass<GemmRewriter>(gpu_version);

    // Rewrite GEMMs with broadcasted inputs as strided GEMMs.
    pipeline.AddPass<GemmBroadcastFoldingRewriter>();

    if (debug_options.xla_gpu_normalize_layouts()) {
      pipeline.AddPass<LayoutNormalization>(&NormalizeLayoutForGpuCustomCalls);
      pipeline.AddPass<HloPassFix<AlgebraicSimplifier>>(simplifier_options);
    }
    pipeline.AddPass<BroadcastCanonicalizer>();

    pipeline.AddPass<ReductionDegenerateDimRemover>();
    pipeline.AddPass<ReductionLayoutNormalizer>();
    // Run Softmax fusion after layout normalization. We expect a default layout
    // in the softmax codegen pipeline. However we should run before
    // ReductionDimensionGrouper, as that makes matching the softmax pattern
    // harder.
    if (debug_options.xla_gpu_enable_triton_softmax_fusion() &&
        cuda_cc != nullptr &&
        cuda_cc->IsAtLeast(se::CudaComputeCapability::VOLTA)) {
      pipeline.AddPass<HloPassFix<AlgebraicSimplifier>>(simplifier_options);
      pipeline.AddPass<SoftmaxRewriterTriton>(gpu_version);
    }

    pipeline.AddPass<ReductionDimensionGrouper>();
    pipeline.AddPass<HloPassFix<ReductionSplitter>>();
    pipeline.AddPass<HloPassFix<GpuTreeReductionRewriter>>(gpu_version);
    TF_RETURN_IF_ERROR(pipeline.Run(hlo_module).status());
  }

  HloPassPipeline pipeline("post-layout_assignment");
  AddHloVerifier(&pipeline,
                 HloVerifierOpts{}
                     .MakeLayoutSensitive()
                     .WithInstructionCanChangeLayout(
                         LayoutAssignment::InstructionCanChangeLayout)
                     .VerifyBroadcastDimensionsOrder()
                     .VerifyReshapeIsBitcast(),
                 /*debug_only=*/true);

  // Linearize collective schedule if online autotuning of convolutions is
  // enabled.
  pipeline.AddPass<CollectivesScheduleLinearizer>(
      [this, stream_exec](const HloModule* module) {
        return RequiresCollectiveScheduleLinearizer(module, stream_exec);
      });

  // Triton compilation needs normalized operations on bf16 (i.e. converted to
  // f32).
  add_float_normalization(pipeline);

  TF_RETURN_IF_ERROR(AddTritonGemmAutotuningPasses(
      &pipeline, hlo_module, autotune_config, thread_pool));
  // Inline back the calls which have better performance with cuBLAS.
  pipeline.AddPass<CallInliner>();
  // TODO(tdanyluk): Apply CublasPadForGemms to the cuBLAS GEMMs generated
  // here for possibly better cuBLAS performance.
  pipeline.AddPass<GemmRewriter>(gpu_version);
  // Rewrite GEMMs with broadcasted inputs as strided GEMMs.
  pipeline.AddPass<GemmBroadcastFoldingRewriter>();

  TF_RETURN_IF_ERROR(AddConvAndGemmAutotuningPasses(
      &pipeline, hlo_module, autotune_config, thread_pool));

  // The Triton autotuner can insert new bf16 reductions that need to be
  // normalized again.
  add_float_normalization(pipeline);

  // Clean up new_tuple described above.
  pipeline.AddPass<TupleSimplifier>();

  // The LayoutAssignment pass may leave behind kCopy instructions which are
  // duplicate or NOPs, so remove them with algebraic simplification and CSE.
  pipeline.AddPass<HloPassFix<AlgebraicSimplifier>>(simplifier_options);

  // Since this CSE runs after collective schedule linearizer which inserts
  // control dependencies, ignore these control deps when replacing instructions
  // with equivalent ones here.
  pipeline.AddPass<HloCSE>(/*is_layout_sensitive=*/true,
                           /*only_fusion_computations*/ false,
                           /*ignore_control_dependencies=*/true);
  TF_RETURN_IF_ERROR(pipeline.Run(hlo_module).status());

  return OkStatus();
}

StatusOr<std::unique_ptr<HloModule>> GpuCompiler::RunHloPasses(
    std::unique_ptr<HloModule> module, se::StreamExecutor* stream_exec,
    const CompileOptions& options) {
  const DebugOptions& debug_options = module->config().debug_options();
  TF_RETURN_IF_ERROR(LoadAutotuneResultsFromFile(debug_options));

  // We dump the post-optimization HLO in RunBackend so no need to dump it here.
  XLA_SCOPED_LOGGING_TIMER_IF(
      absl::StrCat("GpuCompiler::RunHloPasses for ", module->name()),
      !options.is_autotuning_compilation);
  uint64_t start_usecs = tsl::Env::Default()->NowMicros();
  tsl::profiler::TraceMe activity(
      [&] { return absl::StrCat("HLO Transforms:", module->name()); },
      tsl::profiler::TraceMeLevel::kInfo);

  GpuTargetConfig gpu_target_config = GetGpuTargetConfig(stream_exec);
  TF_RETURN_IF_ERROR(OptimizeHloModule(module.get(), stream_exec, options,
                                       gpu_target_config,
                                       /*autotune_results=*/nullptr));

  TF_RETURN_IF_ERROR(PrepareHloModuleForIrEmitting(module.get()));

  uint64_t end_usecs = tsl::Env::Default()->NowMicros();

  // This won't record values for calls that error out (because if they error
  // out we have no way of telling how far through the process we got).
  RecordHloPassesDuration(end_usecs - start_usecs);

  TF_RETURN_IF_ERROR(SerializeAutotuneResultsToFile(debug_options));

  return std::move(module);
}

StatusOr<std::unique_ptr<HloModule>> GpuCompiler::RunHloPassesWithoutDevice(
    std::unique_ptr<HloModule> module, const CompileOptions& options,
    const GpuTargetConfig& gpu_target_config,
    const AutotuneResults& autotune_results) {
  // We dump the post-optimization HLO in RunBackend so no need to dump it here.
  XLA_SCOPED_LOGGING_TIMER_IF(
      absl::StrCat("GpuCompiler::RunHloPasses for ", module->name()),
      !options.is_autotuning_compilation);
  uint64_t start_usecs = tsl::Env::Default()->NowMicros();
  tsl::profiler::TraceMe activity(
      [&] { return absl::StrCat("HLO Transforms:", module->name()); },
      tsl::profiler::TraceMeLevel::kInfo);
  TF_RETURN_IF_ERROR(OptimizeHloModule(module.get(), nullptr, options,
                                       gpu_target_config, &autotune_results));

  TF_RETURN_IF_ERROR(PrepareHloModuleForIrEmitting(module.get()));

  uint64_t end_usecs = tsl::Env::Default()->NowMicros();

  // This won't record values for calls that error out (because if they error
  // out we have no way of telling how far through the process we got).
  RecordHloPassesDuration(end_usecs - start_usecs);

  return std::move(module);
}

namespace {
Status RunPostSchedulingCopyInsertion(
    HloModule* module,
    const HloDataflowAnalysis::CanShareBuffer& can_share_buffer) {
  // We run a separate pass of copy elision here because the sequential ordering
  // from the HLO schedule potentially allows for more copies to be eliminated.
  constexpr int64_t kRegionBasedLiveRangeAnalysisLimit = -1;
  const int64_t kUseRegionBasedLiveRangeAnalysis =
      module->config()
              .debug_options()
              .xla_gpu_copy_insertion_use_region_analysis()
          ? kRegionBasedLiveRangeAnalysisLimit
          : 0;
  CopyInsertion copy_insertion(can_share_buffer,
                               kUseRegionBasedLiveRangeAnalysis);
  TF_RETURN_IF_ERROR(copy_insertion.RemoveUnnecessaryCopies(module));

  // Stash away the schedule during copy insertion, to avoid validation failures
  // while the module is in flux.
  HloSchedule saved_schedule = module->schedule();
  module->clear_schedule();

  // RemoveUnnecessaryCopies only considers interference when determining
  // whether it is legal to remove a copy. However, copies in the graph may be
  // necessary for other reason such as preventing a constant from being live
  // out of the graph. So run AddSpecialCaseCopies to re-insert these copies.
  TF_RETURN_IF_ERROR(
      copy_insertion.CopyInsertion::AddSpecialCaseCopies(module));

  TF_RETURN_IF_ERROR(HloDCE().Run(module).status());

  // The passes above can add and remove copies, update the schedule to
  // account for these transformations. Newly added instructions will be
  // placed ASAP in the schedule.

  // Update and restore the schedule. The saved schedule has a reference to the
  // updated HLO module. The saved schedule needs to be updated before restoring
  // it to the module to avoid validation failures.
  TF_RETURN_IF_ERROR(saved_schedule.Update());
  TF_RETURN_IF_ERROR(module->set_schedule(std::move(saved_schedule)));

  return OkStatus();
}
}  // namespace

StatusOr<std::unique_ptr<BufferAssignment>> GpuCompiler::AssignBuffers(
    HloModule* hlo_module, se::StreamExecutor* stream_exec) {
  const GpuDeviceInfo gpu_device_info = GetGpuDeviceInfo(stream_exec);
  const int64_t scheduler_mem_limit =
      GetSchedulerMemoryLimit(hlo_module, gpu_device_info, pointer_size_);
  TF_RETURN_IF_ERROR(
      ScheduleGpuModule(hlo_module, pointer_size_, scheduler_mem_limit));
  TF_RETURN_IF_ERROR(
      RunPostSchedulingCopyInsertion(hlo_module, GetCanShareBuffer()));

  auto buffer_size_bytes_function =
      [this](const BufferValue& buffer_value) -> int64_t {
    return GetSizeOfShape(buffer_value.shape(), pointer_size_);
  };

  TF_ASSIGN_OR_RETURN(
      std::unique_ptr<BufferAssignment> assignment,
      BufferAssigner::Run(
          hlo_module,
          std::make_unique<SequentialHloOrdering>(hlo_module->schedule()),
          buffer_size_bytes_function,
          /*color_alignment=*/
          [](LogicalBuffer::Color) { return kXlaAllocatedBufferAlignBytes; },
          /*allocate_buffers_for_constants=*/true,
          /*colorer=*/BufferAssigner::DefaultColorer(),
          /*must_not_live_out=*/{}, GetCanShareBuffer()));

  return std::move(assignment);
}

using OutputInfoMap =
    absl::flat_hash_map<ShapeIndex, GpuExecutable::OutputInfo>;

static void NullDiagnosticHandler(const llvm::DiagnosticInfo& diag_info,
                                  void* context) {
  std::string error_string;
  llvm::raw_string_ostream string_printer(error_string);
  llvm::DiagnosticPrinterRawOStream diagnostic_printer(string_printer);
  diag_info.print(diagnostic_printer);

  VLOG(5) << error_string;
}

StatusOr<std::pair<std::string, std::vector<uint8_t>>>
GpuCompiler::CompileToTargetBinary(const HloModuleConfig& module_config,
                                   std::unique_ptr<llvm::Module> llvm_module,
                                   GpuVersion gpu_version,
                                   se::StreamExecutor* stream_exec,
                                   const CompileOptions& options,
                                   const HloModule* debug_module) {
  using BackendCompileResult = std::pair<std::string, std::vector<uint8_t>>;

  const auto compile_single_module =
      [this, gpu_version, &module_config, &options, debug_module](
          llvm::Module* llvm_module, bool relocatable,
          std::optional<int> shard_number) -> StatusOr<BackendCompileResult> {
    {
      // This may print multiple lines per HLO compilation because of the
      // parallelized compilation of LLVM modules.
      XLA_SCOPED_LOGGING_TIMER_IF(
          absl::StrCat(
              "GpuCompiler::RunBackend - Running LLVM verifier for ",
              (debug_module != nullptr ? debug_module->name() : "(unknown)")),
          !options.is_autotuning_compilation);

      llvm_module->getContext().setDiagnosticHandlerCallBack(
          NullDiagnosticHandler, nullptr);

      std::string err;
      llvm::raw_string_ostream err_stream(err);

      // verifyModule() returns true if the module is broken.
      TF_RET_CHECK(!llvm::verifyModule(*llvm_module, &err_stream))
          << "Invalid LLVM IR before optimizations:\n"
          << err_stream.str()
          << "\nThis probably indicates a bug in the HLO -> LLVM IR "
             "lowering. Rerun with --xla_dump_to to get the IR"
          << (debug_module
                  ? absl::StrCat(" and looks for files with name containing: *",
                                 FilenameFor(*debug_module, "", ""), "*")
                  : ".");
    }
    StatusOr<std::pair<std::string, std::vector<uint8_t>>> result =
        CompileTargetBinary(module_config, llvm_module, gpu_version,
                            relocatable, debug_module, options);

    if (!result.ok()) {
      return result;
    }

    const bool should_dump =
        DumpingEnabledForHloModule(debug_module ? debug_module->name() : "",
                                   module_config.debug_options());

    if (should_dump) {
      if (debug_module) {
        if (shard_number.has_value()) {
          llvm_ir::DumpIrIfEnabled(*debug_module, *llvm_module,
                                   /*optimized=*/true,
                                   std::to_string(*shard_number));
        } else {
          llvm_ir::DumpIrIfEnabled(*debug_module, *llvm_module,
                                   /*optimized=*/true);
        }
      } else {
        LOG(ERROR)
            << "Dumping is not implemented since the file name cannot be "
               "inferred. Please implement (potentially MLIR) module -> "
               "filename heuristic.";
      }
    }

    if (user_post_optimization_hook_) {
      user_post_optimization_hook_(*llvm_module);
    }

    // Write PTX to IR dump directory, if IR dumping was requested.
    if (should_dump) {
      absl::string_view ptx = result->first;
      if (debug_module) {
        if (shard_number.has_value()) {
          DumpToFileInDirOrStdout(*debug_module, "",
                                  std::to_string(*shard_number) + ".ptx", ptx);
        } else {
          DumpToFileInDirOrStdout(*debug_module, "", "ptx", ptx);
        }
      } else {
        LOG(ERROR)
            << "Dumping is not implemented since the file name cannot be "
               "inferred. Please implement (potentially MLIR) module -> "
               "filename heuristic.";
      }
    }

    return result;
  };

  // Disable multi-threading during deviceless AOT compilation.
  // TODO(anlunx): Enable multi-threading once deviceless AOT compilation is
  // enabled.
  if (!stream_exec) {
    return compile_single_module(llvm_module.get(), /*relocatable=*/false,
                                 /*shard_number=*/std::nullopt);
  }

  tsl::thread::ThreadPool* thread_pool;
  std::optional<tsl::thread::ThreadPool> overriding_thread_pool;
  switch (
      module_config.debug_options().xla_gpu_force_compilation_parallelism()) {
    case 0:
      thread_pool = options.thread_pool;
      break;
    case 1:
      thread_pool = nullptr;
      break;
    default:
      overriding_thread_pool.emplace(
          tsl::Env::Default(), "",
          module_config.debug_options()
              .xla_gpu_force_compilation_parallelism());
      thread_pool = &*overriding_thread_pool;
      break;
  }

  if (!thread_pool) {
    return compile_single_module(llvm_module.get(), /*relocatable=*/false,
                                 /*shard_number=*/std::nullopt);
  }

  // Test whether LinkModules is supported.
  TF_ASSIGN_OR_RETURN(bool can_use_link_modules,
                      CanUseLinkModules(module_config));
  if (!can_use_link_modules) {
    return compile_single_module(llvm_module.get(), /*relocatable=*/false,
                                 /*shard_number=*/std::nullopt);
  }
  std::vector<std::unique_ptr<llvm::Module>> llvm_modules;
  int num_functions = 0;
  for (llvm::Function& func : llvm_module->functions()) {
    if (!func.isDeclaration() &&
        func.getLinkage() == llvm::GlobalValue::LinkageTypes::ExternalLinkage) {
      num_functions++;
    }
  }

  // Record the name of some constant global variables and their initializers.
  // We'll change the linkage type of these variables from external to internal
  // to ensure constant-folding works properly after calling llvm::SplitModule.
  llvm::DenseMap<llvm::StringRef, llvm::Constant*> const_initializer_map;
  for (llvm::GlobalVariable& gv : llvm_module->globals()) {
    if (gv.hasName() && gv.isConstant() && gv.hasInitializer() &&
        gv.hasExternalLinkage()) {
      llvm::Constant* initializer = gv.getInitializer();
      unsigned int num_elements = 0;
      if (auto* caz =
              llvm::dyn_cast<llvm::ConstantAggregateZero>(initializer)) {
        num_elements = caz->getElementCount().getFixedValue();
      } else if (auto* cds = llvm::dyn_cast<llvm::ConstantDataSequential>(
                     initializer)) {
        num_elements = cds->getNumElements();
      }
      if (num_elements > 0) {
        const_initializer_map[gv.getName()] = initializer;
      }
    }
  }

  llvm::SplitModule(
      *llvm_module,
      std::max<unsigned>(
          1, std::min<unsigned>(thread_pool->NumThreads(), num_functions)),
      [&](std::unique_ptr<llvm::Module> module) {
        // Change the linkage type of some global constant variables to internal
        for (llvm::GlobalVariable& gv : module->globals()) {
          if (gv.hasName() && gv.isConstant() && !gv.hasInitializer() &&
              const_initializer_map.count(gv.getName()) != 0) {
            gv.setInitializer(const_initializer_map[gv.getName()]);
            gv.setLinkage(llvm::GlobalValue::InternalLinkage);
          }
        }
        llvm_modules.push_back(std::move(module));
      },
      /*PreserveLocals=*/true);

  std::vector<StatusOr<BackendCompileResult>> compile_results(
      llvm_modules.size());
  tsl::BlockingCounter counter(llvm_modules.size());
  for (int i = 0; i < llvm_modules.size(); i++) {
    thread_pool->Schedule(
        [&compile_results, compile_single_module, i, &llvm_modules, &counter] {
          llvm::Module* original_module = llvm_modules[i].get();
          llvm::LLVMContext context;

          std::unique_ptr<llvm::Module> new_llvm_module;
          // Switch to a new context by dumping and re-parsing LLVM IR. Each
          // thread has its own context to avoid race conditions.
          {
            std::string ir = llvm_ir::DumpToString(original_module);
            llvm::SMDiagnostic err;
            new_llvm_module = llvm::parseAssemblyString(ir, err, context);
            if (!new_llvm_module) {
              std::string err_string;
              llvm::raw_string_ostream os(err_string);
              err.print(/*ProgName=*/nullptr, os, /*ShowColors=*/false);
              LOG(FATAL) << "Failed to parse IR: " << err_string;
            }
          }

          compile_results[i] = compile_single_module(
              new_llvm_module.get(), /*relocatable=*/true, /*shard_number=*/i);
          counter.DecrementCount();
        });
  }
  counter.Wait();

  std::string ptx_snippets;
  std::vector<std::vector<uint8_t>> submodule_compile_results;
  for (auto& maybe_result : compile_results) {
    TF_ASSIGN_OR_RETURN(auto result, maybe_result);
    if (result.second.empty()) {
      continue;
    }
    ptx_snippets += result.first;
    ptx_snippets += "\n";
    submodule_compile_results.push_back(result.second);
  }

  auto maybe_backend_result =
      this->LinkModules(stream_exec, std::move(submodule_compile_results),
                        module_config.debug_options());
  if (!maybe_backend_result.ok()) {
    LOG(ERROR) << "The CUDA linking API did not work. Please use "
                  "XLA_FLAGS=--xla_gpu_force_compilation_parallelism=1 to "
                  "bypass it, but expect to get longer compilation time due to "
                  "the lack of multi-threading. Original error: "
               << maybe_backend_result.status();
    return maybe_backend_result.status();
  }

  return std::make_pair(ptx_snippets, std::move(*maybe_backend_result));
}

StatusOr<std::unique_ptr<Executable>> GpuCompiler::RunBackend(
    std::unique_ptr<HloModule> module, se::StreamExecutor* stream_exec,
    const CompileOptions& options) {
  if (!options.is_autotuning_compilation) {
    VLOG(1) << "Starting to compile HLO module " << module->name();
  }

  XLA_SCOPED_LOGGING_TIMER_IF(
      absl::StrCat("GpuCompiler::RunBackend for ", module->name()),
      !options.is_autotuning_compilation);
  std::string slow_compilation_msg =
      absl::StrCat("Compiling module ", module->name());
  auto slow_compile_alarm = SlowCompilationAlarm(slow_compilation_msg);

  if (options.is_autotuning_compilation) {
    if (module->config()
            .debug_options()
            .xla_gpu_enable_persistent_temp_buffers()) {
      LOG(WARNING) << "Doing autotuning compilations with "
                      "xla_gpu_enable_persistent_temp_buffers wastes memory!";
    }
    if (module->config().debug_options().xla_embed_ir_in_executable()) {
      LOG(WARNING) << "Doing autotuning compilations with "
                      "xla_embed_ir_in_executable wastes memory!";
    }
  }

  TF_RET_CHECK(stream_exec != nullptr);

  llvm::LLVMContext llvm_context;

  const GpuDeviceInfo gpu_device_info = GetGpuDeviceInfo(stream_exec);

  if (module->config().hlo_profiling_enabled() || VLOG_IS_ON(1)) {
    HloCostAnalysis::Options cost_analysis_options{ShapeSizeBytesFunction()};
    cost_analysis_options.set_bytes_per_second(
        stream_exec->GetDeviceDescription().memory_bandwidth());
    GpuHloCostAnalysis cost_analysis(cost_analysis_options, &gpu_device_info);
    TF_RETURN_IF_ERROR(module->entry_computation()->Accept(&cost_analysis));
    if (!options.is_autotuning_compilation) {
      VLOG(1) << "HLO memory read+written: "
              << tsl::strings::HumanReadableNumBytes(
                     cost_analysis.bytes_accessed());
    }
    if (module->config().hlo_profiling_enabled()) {
      LOG(ERROR) << "--xla_hlo_profile for GPU is unsupported.";
    }
  }

  const int64_t scheduler_mem_limit =
      GetSchedulerMemoryLimit(module.get(), gpu_device_info, pointer_size_);
  TF_RETURN_IF_ERROR(
      ScheduleGpuModule(module.get(), pointer_size_, scheduler_mem_limit));
  TF_RETURN_IF_ERROR(
      RunPostSchedulingPipelines(module.get(), scheduler_mem_limit));

  CompileModuleResults compile_module_results;
  TF_RETURN_IF_ERROR(CompileModuleToLlvmIrImpl(
      module.get(), &llvm_context, target_triple_, data_layout_,
      stream_exec->platform()->Name(), stream_exec->platform()->id(),
      gpu_device_info, GetCanShareBuffer(), BufferSizeBytesFunction(),
      &compile_module_results, stream_exec));

  if (user_pre_optimization_hook_) {
    user_pre_optimization_hook_(*compile_module_results.llvm_module);
  }
  std::string ir_module_string_before_opt;
  const bool embed_ir_in_executable =
      module->config().debug_options().xla_embed_ir_in_executable();
  if (embed_ir_in_executable) {
    ir_module_string_before_opt =
        llvm_ir::DumpToString(compile_module_results.llvm_module.get());
  }

  llvm_ir::DumpIrIfEnabled(*module, *compile_module_results.llvm_module,
                           /*optimized=*/false);

  std::string asm_text;
  std::vector<uint8_t> binary;
  TF_ASSIGN_OR_RETURN(
      std::tie(asm_text, binary),
      CompileToTargetBinary(
          module->config(), std::move(compile_module_results.llvm_module),
          GetGpuVersion(stream_exec), stream_exec, options, module.get()));

  if (DumpingEnabledForHloModule(*module) &&
      std::holds_alternative<GpuExecutable::OwnedThunkSequence>(
          compile_module_results.executable)) {
    const ThunkSequence& thunk_sequence =
        *std::get<GpuExecutable::OwnedThunkSequence>(
            compile_module_results.executable);
    DumpToFileInDirOrStdout(*module, "", "thunk_sequence.txt",
                            thunk_sequence.ToString());
  }

  std::shared_ptr<const BufferAssignment> buffer_assignment;
  std::unique_ptr<BufferAssignmentProto> buffer_assignment_proto;
  std::function<std::string()> buffer_assignment_dumper = [] {
    return std::string();
  };
  if (!options.is_autotuning_compilation) {
    // Make it shared to be captured in the later lambda.
    buffer_assignment = std::move(compile_module_results.buffer_assignment);
    buffer_assignment_proto =
        std::make_unique<BufferAssignmentProto>(buffer_assignment->ToProto());
    buffer_assignment_dumper = [buffer_assignment] {
      return buffer_assignment->ToVerboseString();
    };
  }

  TF_ASSIGN_OR_RETURN(
      auto gpu_executable,
      GpuExecutable::Create(GpuExecutable::Params{
          /*asm_text=*/(options.is_autotuning_compilation && !binary.empty())
              ? std::string()
              : std::move(asm_text),
          /*binary=*/std::move(binary),
          /*gpu_version=*/GetGpuVersion(stream_exec),
          /*executable=*/std::move(compile_module_results.executable),
          /*entry_func_attrs=*/
          std::move(compile_module_results.entry_func_attrs),
          /*constants=*/std::move(compile_module_results.constants),
          /*output_info=*/std::move(compile_module_results.output_info),
          /*module_name=*/std::move(compile_module_results.module_name),
          /*output_shape=*/std::move(compile_module_results.output_shape),
          /*allocations=*/std::move(compile_module_results.allocations),
          /*enable_persistent_temp_buffers=*/
          module->config()
              .debug_options()
              .xla_gpu_enable_persistent_temp_buffers(),
          /*debug_buffer_assignment=*/std::move(buffer_assignment_proto),
          /*verbose_buffer_assignment_string_dumper=*/
          std::move(buffer_assignment_dumper),
          /*debug_module=*/options.is_autotuning_compilation
              ? std::unique_ptr<HloModule>()
              : std::move(module),
          /*enable_debug_info_manager=*/!options.is_autotuning_compilation}));
  if (embed_ir_in_executable) {
    DCHECK_NE("", ir_module_string_before_opt);
    gpu_executable->set_ir_module_string(ir_module_string_before_opt);
  }

  IncrementCompiledProgramsCount();

  if (!options.is_autotuning_compilation && gpu_executable->has_module()) {
    // Dump computation proto state and buffer assignment for
    // CompiledMemoryAnalysis.
    auto hlo_proto = std::make_unique<HloProto>();
    *hlo_proto->mutable_hlo_module() = gpu_executable->module().ToProto();
    *hlo_proto->mutable_buffer_assignment() = buffer_assignment->ToProto();
    gpu_executable->set_hlo_proto(std::move(hlo_proto));
    gpu_executable->set_debug_info(buffer_assignment->GetStats().ToString());
  }

  return static_cast<std::unique_ptr<Executable>>(std::move(gpu_executable));
}

StatusOr<std::vector<std::unique_ptr<AotCompilationResult>>>
GpuCompiler::CompileAheadOfTime(std::unique_ptr<HloModuleGroup> module_group,
                                const AotCompilationOptions& options) {
  CHECK(options.PlatformId() == se::cuda::kCudaPlatformId);

  std::vector<std::unique_ptr<HloModule>> modules =
      module_group->ConsumeModules();
  std::vector<std::unique_ptr<AotCompilationResult>> results;

  std::any target_config = options.target_config();
  auto* gpu_target_config = std::any_cast<GpuTargetConfig>(&target_config);
  CHECK(gpu_target_config != nullptr || options.executor() != nullptr);

  for (const auto& module : modules) {
    llvm::LLVMContext llvm_context;

    const int64_t scheduler_mem_limit = GetSchedulerMemoryLimit(
        module.get(),
        gpu_target_config != nullptr ? gpu_target_config->gpu_device_info
                                     : GetGpuDeviceInfo(options.executor()),
        pointer_size_);
    TF_RETURN_IF_ERROR(
        ScheduleGpuModule(module.get(), pointer_size_, scheduler_mem_limit));
    TF_RETURN_IF_ERROR(
        RunPostSchedulingPipelines(module.get(), scheduler_mem_limit));

    // Compile the module
    CompileModuleResults compile_module_results;

    if (gpu_target_config) {
      TF_RETURN_IF_ERROR(CompileModuleToLlvmIrImpl(
          module.get(), &llvm_context, target_triple_, data_layout_,
          gpu_target_config->platform_name, options.PlatformId(),
          gpu_target_config->gpu_device_info, GetCanShareBuffer(),
          BufferSizeBytesFunction(), &compile_module_results));
    } else {
      CHECK(options.executor() != nullptr);
      auto stream_exec = options.executor();
      TF_RETURN_IF_ERROR(CompileModuleToLlvmIrImpl(
          module.get(), &llvm_context, target_triple_, data_layout_,
          stream_exec->platform()->Name(), options.PlatformId(),
          GetGpuDeviceInfo(stream_exec), GetCanShareBuffer(),
          BufferSizeBytesFunction(), &compile_module_results));
    }
    if (user_pre_optimization_hook_) {
      user_pre_optimization_hook_(*compile_module_results.llvm_module);
    }

    using BackendCompileResult = std::pair<std::string, std::vector<uint8_t>>;
    BackendCompileResult backend_result;
    if (gpu_target_config) {
      TF_ASSIGN_OR_RETURN(
          backend_result,
          CompileToTargetBinary(
              module->config(), std::move(compile_module_results.llvm_module),
              gpu_target_config->gpu_device_info.compute_capability,
              options.executor(), {options.device_allocator()}, module.get()));
    } else {
      TF_ASSIGN_OR_RETURN(
          backend_result,
          CompileToTargetBinary(
              module->config(), std::move(compile_module_results.llvm_module),
              GetGpuVersion(options.executor()), options.executor(),
              {options.device_allocator()}, module.get()));
    }

    auto& compiled_executable = compile_module_results.executable;

    if (!std::holds_alternative<GpuExecutable::OwnedGpuRuntimeProgram>(
            compiled_executable)) {
      return InternalError("Gpu runtime program was not provided");
    }

    // TODO(ezhulenev): Unify AOT compilation with GpuRuntimeExecutable::Create
    // (see `gpu/runtime/executable.h`).

    const auto& program =
        std::get<GpuExecutable::OwnedGpuRuntimeProgram>(compiled_executable);

    // Options for the default XLA runtime compilation pipeline.
    runtime::CompilationPipelineOptions copts;

    // Populate mapping from XLA (SE) enums/structs type id to symbol names.
    copts.populate_type_id_names = RegisterXlaGpuTypeIdNames;

    // For passing LMHLO attributes as XLA (SE) enums/structs to custom calls.
    copts.populate_attr_encodings = RegisterXlaGpuAttrEncoding;

    // Options for constructing XLA runtime JitExecutable.
    runtime::JitExecutable::Options opts;
    opts.specialization = runtime::JitExecutable::Specialization::kDisabled;
    opts.compiler.register_dialects =
        runtime::RegisterDefaultXlaGpuRuntimeDialects;

    // Register XLA Gpu runtime custom calls with the linker.
    opts.compiler.symbols_binding = runtime::ToSymbolsBinding(
        RegisterXlaGpuRuntimeCustomCalls, RegisterXlaGpuTypeIdNames);

    opts.compiler.create_compilation_pipeline =
        [copts](xla::runtime::PassManager& passes) {
          runtime::CreateDefaultXlaGpuRuntimeCompilationPipeline(passes, copts);
        };

    // Instantiate new JitExecutable from the MLIR source.
    auto jit_executable = runtime::JitExecutable::Instantiate(
        program->module, program->entry_point, opts);
    if (!jit_executable.ok())
      return InternalError("Failed to compile XLA program: %s",
                           jit_executable.status().message());

    // For static shapes we can always serialize only the default executable.
    runtime::Executable& executable = jit_executable->DefaultExecutable().get();

    // Check if XLA runtime executable saved the compilation result.
    std::unique_ptr<llvm::MemoryBuffer> obj_file = executable.obj_file();
    if (!obj_file)
      return InternalError("XLA runtime executable didn't save the obj file");

    std::string data(obj_file->getBuffer().data(),
                     obj_file->getBuffer().size());

    results.emplace_back(std::make_unique<GpuXlaRuntimeAotCompilationResult>(
        module->ToProto(), data, program->module,
        compile_module_results.entry_func_attrs, backend_result.first,
        backend_result.second, compile_module_results.constants));
  }
  return std::move(results);
}

HloCostAnalysis::ShapeSizeFunction GpuCompiler::ShapeSizeBytesFunction() const {
  // Capture just the pointer size, not the entire GpuCompiler object.
  return [pointer_size = pointer_size_](const Shape& shape) {
    return GetSizeOfShape(shape, pointer_size);
  };
}

StatusOr<std::unique_ptr<AotCompilationResult>> GpuCompiler::Export(
    Executable* executable) const {
  auto* gpu_executable = tensorflow::down_cast<GpuExecutable*>(executable);
  if (!gpu_executable) return Internal("GpuExecutable is null");
  HloModuleProto module_proto = gpu_executable->module().ToProto();
  TF_ASSIGN_OR_RETURN(auto obj_file, gpu_executable->GetObjFile());
  TF_ASSIGN_OR_RETURN(auto mlir_module, gpu_executable->GetMlirModule());
  xla::EntryFunctionAttributes entry_func_attrs =
      gpu_executable->entry_func_attrs();
  auto text = gpu_executable->text();
  auto binary = gpu_executable->binary();

  std::unique_ptr<AotCompilationResult> result =
      std::make_unique<xla::gpu::GpuXlaRuntimeAotCompilationResult>(
          module_proto, obj_file, mlir_module, entry_func_attrs, text, binary,
          gpu_executable->constants());
  return result;
}

Status GpuCompiler::RunPostSchedulingPipelines(
    HloModule* module, int64_t scheduler_mem_limit) const {
  {
    HloPassPipeline pipeline("post-scheduling-passes");

    HloPredicate is_nop =
        HloPredicateIsOp<HloOpcode::kParameter, HloOpcode::kConstant,
                         HloOpcode::kBitcast, HloOpcode::kGetTupleElement>;
    pipeline.AddPass<GpuConvertAsyncCollectivesToSync>(is_nop);
    pipeline.AddPass<OptimizationBarrierExpander>();

    TF_RETURN_IF_ERROR(pipeline.Run(module).status());
  }

  {
    HloPassPipeline pipeline("remat-pipeline");

    HloCostAnalysis hlo_cost_analysis(ShapeSizeBytesFunction());
    HloRematerialization::RematerializationModeConfig
        rematerialization_mode_config(/*recompute=*/true, /*compress=*/true,
                                      /*host_offload=*/false);
    HloRematerialization::Options options(
        hlo_cost_analysis, rematerialization_mode_config,
        // Assume 75% of the total device memory is available for XLA.
        /*memory_limit_bytes=*/scheduler_mem_limit,
        /*block_size_limit=*/1, /*block_rematerialization_factor=*/1,
        /*min_remat_size=*/0, /*compact_shape_function=*/nullptr,
        /*host_memory_offload_config=*/std::nullopt);
    HloRematerialization::RematerializationSizes sizes;
    pipeline.AddPass<HloRematerialization>(options, sizes);

    TF_ASSIGN_OR_RETURN(bool changed, pipeline.Run(module));
    if (changed) {
      VLOG(1) << "HloRematerialization saved "
              << sizes.before_bytes - sizes.after_bytes << " bytes";
    }
  }

  {
    HloPassPipeline pipeline("fusion-wrapper");
    pipeline.AddPass<FusionWrapper>();
    // Wrap remaining unfused ops that have no LHLO equivalent in single-op
    // fusions. This needs to happen after rematerialization, because that will
    // insert additional copies.
    TF_RETURN_IF_ERROR(pipeline.Run(module).status());
  }
  return OkStatus();
}

}  // namespace gpu
}  // namespace xla
