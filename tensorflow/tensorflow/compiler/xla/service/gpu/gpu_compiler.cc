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

#include "tensorflow/compiler/xla/service/gpu/gpu_compiler.h"

#include <algorithm>
#include <any>
#include <cstdint>
#include <functional>
#include <memory>
#include <optional>
#include <queue>
#include <string>
#include <utility>
#include <variant>
#include <vector>

#include "absl/container/flat_hash_set.h"
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
#include "tensorflow/compiler/xla/hlo/ir/hlo_casting_utils.h"
#include "tensorflow/compiler/xla/hlo/ir/hlo_instruction.h"
#include "tensorflow/compiler/xla/hlo/ir/hlo_instructions.h"
#include "tensorflow/compiler/xla/hlo/ir/hlo_module.h"
#include "tensorflow/compiler/xla/hlo/ir/hlo_opcode.h"
#include "tensorflow/compiler/xla/hlo/transforms/hlo_constant_splitter.h"
#include "tensorflow/compiler/xla/mlir/backends/gpu/transforms/passes.h"
#include "tensorflow/compiler/xla/mlir/runtime/transforms/compilation_pipeline_gpu.h"
#include "tensorflow/compiler/xla/runtime/jit_executable.h"
#include "tensorflow/compiler/xla/service/algebraic_simplifier.h"
#include "tensorflow/compiler/xla/service/all_gather_broadcast_reorder.h"
#include "tensorflow/compiler/xla/service/all_gather_combiner.h"
#include "tensorflow/compiler/xla/service/all_reduce_combiner.h"
#include "tensorflow/compiler/xla/service/all_reduce_contiguous.h"
#include "tensorflow/compiler/xla/service/all_reduce_folder.h"
#include "tensorflow/compiler/xla/service/all_reduce_promotion.h"
#include "tensorflow/compiler/xla/service/all_reduce_reassociate.h"
#include "tensorflow/compiler/xla/service/async_collective_creator.h"
#include "tensorflow/compiler/xla/service/batchnorm_expander.h"
#include "tensorflow/compiler/xla/service/bitcast_dtypes_expander.h"
#include "tensorflow/compiler/xla/service/broadcast_canonicalizer.h"
#include "tensorflow/compiler/xla/service/buffer_assignment.h"
#include "tensorflow/compiler/xla/service/call_inliner.h"
#include "tensorflow/compiler/xla/service/collective_pipeliner.h"
#include "tensorflow/compiler/xla/service/collectives_schedule_linearizer.h"
#include "tensorflow/compiler/xla/service/comparison_expander.h"
#include "tensorflow/compiler/xla/service/conditional_canonicalizer.h"
#include "tensorflow/compiler/xla/service/conditional_simplifier.h"
#include "tensorflow/compiler/xla/service/convert_mover.h"
#include "tensorflow/compiler/xla/service/convolution_4d_expander.h"
#include "tensorflow/compiler/xla/service/convolution_pred_expander.h"
#include "tensorflow/compiler/xla/service/copy_insertion.h"
#include "tensorflow/compiler/xla/service/dot_decomposer.h"
#include "tensorflow/compiler/xla/service/dot_dimension_merger.h"
#include "tensorflow/compiler/xla/service/dot_merger.h"
#include "tensorflow/compiler/xla/service/dump.h"
#include "tensorflow/compiler/xla/service/dynamic_dimension_simplifier.h"
#include "tensorflow/compiler/xla/service/dynamic_index_splitter.h"
#include "tensorflow/compiler/xla/service/dynamic_padder.h"
#include "tensorflow/compiler/xla/service/eigh_expander.h"
#include "tensorflow/compiler/xla/service/flatten_call_graph.h"
#include "tensorflow/compiler/xla/service/float_normalization.h"
#include "tensorflow/compiler/xla/service/gather_expander.h"
#include "tensorflow/compiler/xla/service/gather_simplifier.h"
#include "tensorflow/compiler/xla/service/gpu/alias_passthrough_params.h"
#include "tensorflow/compiler/xla/service/gpu/all_reduce_blueconnect.h"
#include "tensorflow/compiler/xla/service/gpu/compile_module_to_llvm_ir.h"
#include "tensorflow/compiler/xla/service/gpu/conv_layout_normalization.h"
#include "tensorflow/compiler/xla/service/gpu/copy_fusion.h"
#include "tensorflow/compiler/xla/service/gpu/dot_dimension_sorter.h"
#include "tensorflow/compiler/xla/service/gpu/fusion_merger.h"
#include "tensorflow/compiler/xla/service/gpu/gemm_broadcast_folding_rewriter.h"
#include "tensorflow/compiler/xla/service/gpu/gemm_rewriter.h"
#include "tensorflow/compiler/xla/service/gpu/gemm_rewriter_triton.h"
#include "tensorflow/compiler/xla/service/gpu/gpu_async_collective_annotator.h"
#include "tensorflow/compiler/xla/service/gpu/gpu_constants.h"
#include "tensorflow/compiler/xla/service/gpu/gpu_conv_rewriter.h"
#include "tensorflow/compiler/xla/service/gpu/gpu_device_info.h"
#include "tensorflow/compiler/xla/service/gpu/gpu_executable.h"
#include "tensorflow/compiler/xla/service/gpu/gpu_float_support.h"
#include "tensorflow/compiler/xla/service/gpu/gpu_hlo_cost_analysis.h"
#include "tensorflow/compiler/xla/service/gpu/gpu_hlo_schedule.h"
#include "tensorflow/compiler/xla/service/gpu/gpu_layout_assignment.h"
#include "tensorflow/compiler/xla/service/gpu/gpu_reduce_scatter_creator.h"
#include "tensorflow/compiler/xla/service/gpu/gpu_sanitize_constant_names.h"
#include "tensorflow/compiler/xla/service/gpu/gpu_scatter_expander.h"
#include "tensorflow/compiler/xla/service/gpu/gpu_shape_verifier.h"
#include "tensorflow/compiler/xla/service/gpu/hlo_fusion_stats.h"
#include "tensorflow/compiler/xla/service/gpu/horizontal_input_fusion.h"
#include "tensorflow/compiler/xla/service/gpu/horizontal_loop_fusion.h"
#include "tensorflow/compiler/xla/service/gpu/instruction_fusion.h"
#include "tensorflow/compiler/xla/service/gpu/ir_emission_utils.h"
#include "tensorflow/compiler/xla/service/gpu/matmul_utils.h"
#include "tensorflow/compiler/xla/service/gpu/metrics.h"
#include "tensorflow/compiler/xla/service/gpu/move_copy_to_users.h"
#include "tensorflow/compiler/xla/service/gpu/multi_output_fusion.h"
#include "tensorflow/compiler/xla/service/gpu/priority_fusion.h"
#include "tensorflow/compiler/xla/service/gpu/reduction_degenerate_dim_remover.h"
#include "tensorflow/compiler/xla/service/gpu/reduction_dimension_grouper.h"
#include "tensorflow/compiler/xla/service/gpu/reduction_layout_normalizer.h"
#include "tensorflow/compiler/xla/service/gpu/reduction_splitter.h"
#include "tensorflow/compiler/xla/service/gpu/reduction_utils.h"
#include "tensorflow/compiler/xla/service/gpu/runtime_intrinsics.h"
#include "tensorflow/compiler/xla/service/gpu/scatter_slice_simplifier.h"
#include "tensorflow/compiler/xla/service/gpu/softmax_rewriter_triton.h"
#include "tensorflow/compiler/xla/service/gpu/topk_specializer.h"
#include "tensorflow/compiler/xla/service/gpu/topk_splitter.h"
#include "tensorflow/compiler/xla/service/gpu/tree_reduction_rewriter.h"
#include "tensorflow/compiler/xla/service/gpu/variadic_op_splitter.h"
#include "tensorflow/compiler/xla/service/hlo_computation_deduplicator.h"
#include "tensorflow/compiler/xla/service/hlo_constant_folding.h"
#include "tensorflow/compiler/xla/service/hlo_cse.h"
#include "tensorflow/compiler/xla/service/hlo_dce.h"
#include "tensorflow/compiler/xla/service/hlo_module_config.h"
#include "tensorflow/compiler/xla/service/hlo_pass_fix.h"
#include "tensorflow/compiler/xla/service/hlo_pass_pipeline.h"
#include "tensorflow/compiler/xla/service/hlo_verifier.h"
#include "tensorflow/compiler/xla/service/layout_normalization.h"
#include "tensorflow/compiler/xla/service/llvm_ir/llvm_util.h"
#include "tensorflow/compiler/xla/service/logistic_expander.h"
#include "tensorflow/compiler/xla/service/loop_schedule_linearizer.h"
#include "tensorflow/compiler/xla/service/operand_upcaster.h"
#include "tensorflow/compiler/xla/service/qr_expander.h"
#include "tensorflow/compiler/xla/service/real_imag_expander.h"
#include "tensorflow/compiler/xla/service/reduce_decomposer.h"
#include "tensorflow/compiler/xla/service/reduce_scatter_combiner.h"
#include "tensorflow/compiler/xla/service/reduce_scatter_reassociate.h"
#include "tensorflow/compiler/xla/service/reshape_decomposer.h"
#include "tensorflow/compiler/xla/service/reshape_mover.h"
#include "tensorflow/compiler/xla/service/result_caster.h"
#include "tensorflow/compiler/xla/service/rng_bit_generator_expander.h"
#include "tensorflow/compiler/xla/service/rng_expander.h"
#include "tensorflow/compiler/xla/service/scatter_simplifier.h"
#include "tensorflow/compiler/xla/service/sharding_propagation.h"
#include "tensorflow/compiler/xla/service/sharding_remover.h"
#include "tensorflow/compiler/xla/service/simplify_fp_conversions.h"
#include "tensorflow/compiler/xla/service/slice_sinker.h"
#include "tensorflow/compiler/xla/service/slow_operation_alarm.h"
#include "tensorflow/compiler/xla/service/sort_simplifier.h"
#include "tensorflow/compiler/xla/service/spmd/collective_permute_motion.h"
#include "tensorflow/compiler/xla/service/spmd/stateful_rng_spmd_partitioner.h"
#include "tensorflow/compiler/xla/service/stable_sort_expander.h"
#include "tensorflow/compiler/xla/service/stochastic_convert_decomposer.h"
#include "tensorflow/compiler/xla/service/topk_rewriter.h"
#include "tensorflow/compiler/xla/service/transpose_folding.h"
#include "tensorflow/compiler/xla/service/tuple_simplifier.h"
#include "tensorflow/compiler/xla/service/while_loop_all_reduce_code_motion.h"
#include "tensorflow/compiler/xla/service/while_loop_constant_sinking.h"
#include "tensorflow/compiler/xla/service/while_loop_simplifier.h"
#include "tensorflow/compiler/xla/service/while_loop_trip_count_annotator.h"
#include "tensorflow/compiler/xla/service/zero_sized_hlo_elimination.h"
#include "tensorflow/compiler/xla/status_macros.h"
#include "tensorflow/compiler/xla/stream_executor/cuda/cuda_platform_id.h"
#include "tensorflow/compiler/xla/stream_executor/device_description.h"
#include "tensorflow/compiler/xla/stream_executor/device_description.pb.h"
#include "tensorflow/compiler/xla/stream_executor/dnn.h"
#include "tensorflow/compiler/xla/stream_executor/stream_executor.h"
#include "tensorflow/compiler/xla/stream_executor/stream_executor_pimpl.h"
#include "tensorflow/compiler/xla/util.h"
#include "tensorflow/compiler/xla/xla.pb.h"
#include "tensorflow/compiler/xla/xla_data.pb.h"
#include "tensorflow/tsl/platform/blocking_counter.h"
#include "tensorflow/tsl/platform/casts.h"
#include "tensorflow/tsl/platform/cpu_info.h"
#include "tensorflow/tsl/platform/env.h"
#include "tensorflow/tsl/platform/errors.h"
#include "tensorflow/tsl/platform/statusor.h"
#include "tensorflow/tsl/platform/threadpool.h"
#include "tensorflow/tsl/profiler/lib/traceme.h"

#ifdef PLATFORM_GOOGLE
#include "tensorflow/compiler/xla/hlo/experimental/auto_sharding/auto_sharding.h"
#endif  // PLATFORM_GOOGLE

namespace xla {
namespace gpu {
namespace {
bool ConvIsLowerable(HloInstruction* conv) {
  return GpuConvRewriter::ConvIsLowerable(conv);
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

GpuTargetConfig::GpuTargetConfig(const se::GpuTargetConfigProto& proto)
    : gpu_device_info(proto.gpu_device_info()),
      platform_name(proto.platform_name()),
      dnn_version_info(proto.dnn_version_info()) {
  if (proto.has_cuda_compute_capability()) {
    stream_executor::CudaComputeCapability cuda_compute_capability(
        proto.cuda_compute_capability());
    gpu_version = cuda_compute_capability;
  } else {
    CHECK(proto.has_rocm_compute_capability());
    stream_executor::RocmComputeCapability rocm_compute_capability(
        proto.rocm_compute_capability());
    gpu_version = rocm_compute_capability;
  }

  device_description_str = proto.device_description_str();
}

se::GpuTargetConfigProto GpuTargetConfig::ToProto() const {
  se::GpuTargetConfigProto proto;
  *proto.mutable_gpu_device_info() = gpu_device_info.ToProto();

  if (std::holds_alternative<se::CudaComputeCapability>(gpu_version)) {
    auto cuda_compute_capability =
        std::get<se::CudaComputeCapability>(gpu_version);
    *proto.mutable_cuda_compute_capability() =
        cuda_compute_capability.ToProto();
  } else {
    auto rocm_compute_capability =
        std::get<se::RocmComputeCapability>(gpu_version);
    *proto.mutable_rocm_compute_capability() =
        rocm_compute_capability.ToProto();
  }

  proto.set_platform_name(platform_name);
  *proto.mutable_dnn_version_info() = dnn_version_info.ToProto();
  proto.set_device_description_str(device_description_str);
  return proto;
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
}  // namespace

// Runs optimization passes on the given HLO module.
Status GpuCompiler::OptimizeHloModule(HloModule* hlo_module,
                                      se::StreamExecutor* stream_exec,
                                      const CompileOptions& options,
                                      const GpuTargetConfig& gpu_target_config,
                                      const AutotuneResults* autotune_results) {
  const DebugOptions& debug_options = hlo_module->config().debug_options();

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
      if (gpu_target_config.platform_name == "ROCM") {
        return !gpu::IsMatrixMultiplication(*instr);
      } else {
        return !std::get<se::CudaComputeCapability>(
                    gpu_target_config.gpu_version)
                    .IsAtLeast(se::CudaComputeCapability::VOLTA) ||
               !gpu::IsMatrixMultiplication(*instr);
      }
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
    if (debug_options.xla_gpu_enable_pipelined_all_reduce()) {
      CollectivePipeliner::Config config{
          /*op=*/HloOpcode::kAllReduce,
          /*level_to_operate_on=*/0,
          /*max_pipelining_per_loop=*/INT64_MAX,
          /*last_run=*/true,
          /*process_different_sized_ops=*/true,
          /*pipelining_direction=*/
          CollectivePipeliner::PipeliningDirection::kForward,
          /*should_process=*/HloPredicateTrue};
      collectives_pipeline.AddPass<CollectivePipeliner>(config);
    }
    if (debug_options.xla_gpu_enable_pipelined_all_gather()) {
      CollectivePipeliner::Config config{
          /*op=*/HloOpcode::kAllGather,
          /*level_to_operate_on=*/0,
          /*max_pipelining_per_loop=*/INT64_MAX,
          /*last_run=*/true,
          /*process_different_sized_ops=*/true,
          /*pipelining_direction=*/
          CollectivePipeliner::PipeliningDirection::kBackward,
          /*should_process=*/HloPredicateTrue};
      collectives_pipeline.AddPass<CollectivePipeliner>(config);
    }
    if (debug_options.xla_gpu_enable_pipelined_reduce_scatter()) {
      CollectivePipeliner::Config config{
          /*op=*/HloOpcode::kReduceScatter,
          /*level_to_operate_on=*/0,
          /*max_pipelining_per_loop=*/INT64_MAX,
          /*last_run=*/true,
          /*process_different_sized_ops=*/true,
          /*pipelining_direction=*/
          CollectivePipeliner::PipeliningDirection::kForward,
          /*should_process=*/HloPredicateTrue};
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
  GpuVersion gpu_version = gpu_target_config.gpu_version;
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
      hlo_module, stream_exec, options, gpu_target_config, autotune_results));

  const GpuDeviceInfo& gpu_device_info = gpu_target_config.gpu_device_info;
  auto get_cuda_compute_capability = [&]() {
    return stream_exec != nullptr
               ? stream_exec->GetDeviceDescription().cuda_compute_capability()
               : se::CudaComputeCapability();
  };

  {
    HloPassFix<HloPassPipeline> fusion("fusion");
    // We try to split variadic ops with many parameters into several such ops
    // to avoid exceeding the parameter space.
    fusion.AddPass<VariadicOpSplitter>();
    AddHloVerifier(
        &fusion,
        HloVerifierOpts{}.MakeLayoutSensitive().WithInstructionCanChangeLayout(
            LayoutAssignment::InstructionCanChangeLayout),
        /*debug_only=*/true);

    if (debug_options.xla_gpu_enable_priority_fusion()) {
      GpuHloCostAnalysis::Options cost_analysis_options{
          ShapeSizeBytesFunction(),
          /*per_second_rates=*/{},
          /*count_multiple_input_accesses=*/true};
      fusion.AddPass<GpuPriorityFusion>(gpu_device_info, cost_analysis_options);
    } else {
      fusion.AddPass<GpuInstructionFusion>(/*may_duplicate=*/false,
                                           gpu_device_info);
      fusion.AddPass<GpuInstructionFusion>(/*may_duplicate=*/true,
                                           gpu_device_info);
      fusion.AddPass<FusionMerger>(gpu_device_info,
                                   get_cuda_compute_capability(),
                                   ShapeSizeBytesFunction());
    }
    // Running CSE affects how many users an op has. This plays a role in what
    // we detect as a tiled transpose fusion.
    fusion.AddPass<HloCSE>(/*is_layout_sensitive=*/true,
                           /*only_fusion_computations=*/true);
    fusion.AddPass<GpuMultiOutputFusion>(gpu_device_info,
                                         get_cuda_compute_capability(),
                                         ShapeSizeBytesFunction());
    fusion.AddPass<HloCSE>(/*is_layout_sensitive=*/true,
                           /*only_fusion_computations=*/true);
    fusion.AddPass<HloDCE>();
    TF_RETURN_IF_ERROR(fusion.Run(hlo_module).status());
  }

  {
    HloPassFix<HloPassPipeline> horizontal_fusion("horizontal fusion");
    horizontal_fusion.AddPass<GpuHorizontalLoopFusion>();
    horizontal_fusion.AddPass<GpuHorizontalInputFusion>(gpu_device_info);
    horizontal_fusion.AddPass<HloCSE>(/*is_layout_sensitive=*/true,
                                      /*only_fusion_computations=*/true);
    horizontal_fusion.AddPass<HloDCE>();
    TF_RETURN_IF_ERROR(horizontal_fusion.Run(hlo_module).status());
  }

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

    int32_t blueconnect_num_devices_per_host =
        debug_options.xla_gpu_all_reduce_blueconnect_num_devices_per_host();
    if (blueconnect_num_devices_per_host > 0) {
      pipeline.AddPass<AllReduceBlueConnect>(blueconnect_num_devices_per_host);
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
        switch (inst->opcode()) {
          case HloOpcode::kAllReduceStart:
            return debug_options.xla_gpu_enable_async_all_reduce();
          case HloOpcode::kAllGatherStart:
            return debug_options.xla_gpu_enable_async_all_gather();
          case HloOpcode::kCollectivePermuteStart:
            return debug_options.xla_gpu_enable_async_collective_permute();
          case HloOpcode::kAsyncStart: {
            auto async_inst = Cast<HloAsyncInstruction>(inst);
            switch (async_inst->async_wrapped_opcode()) {
              case HloOpcode::kReduceScatter:
                return debug_options.xla_gpu_enable_async_reduce_scatter();
              case HloOpcode::kAllToAll:
                return debug_options.xla_gpu_enable_async_all_to_all();
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

    if (!hlo_module->config().use_spmd_partitioning()) {
      pipeline.AddPass<CollectivesScheduleLinearizer>();
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
  // In some cases, we have to place the result of an instruction in a temporary
  // buffer. For instance, the buffer that holds an external parameter is
  // assumed immutable at this point, and should not be reused for output
  // (b/27180329). Therefore, in that case, we set the output to be a copy of
  // the parameter.
  HloPassPipeline pipeline("GPU-ir-emit-prepare");
  AddHloVerifier(
      &pipeline,
      HloVerifierOpts{}.MakeLayoutSensitive().WithInstructionCanChangeLayout(
          LayoutAssignment::InstructionCanChangeLayout),
      /*debug_only=*/true);

  // Copy insertion should be performed immediately before IR emission to avoid
  // inserting unnecessary copies (later pass adds an instruction which
  // materializes the value) or missing a necessary copy (later pass removes an
  // instruction which materializes a value). DCE must be run immediately before
  // (and sometime after) copy insertion, to avoid dead code from interfering
  // with the rewrites.
  pipeline.AddPass<HloDCE>();
  if (hlo_module->config().alias_passthrough_params()) {
    pipeline.AddPass<AliasPassthroughParams>();
  }
  pipeline.AddPass<LoopScheduleLinearizer>(GetCanShareBuffer());

  constexpr int64_t kNoRegionBasedLiveRangeAnalysisLimit = -1;
  pipeline.AddPass<CopyInsertion>(GetCanShareBuffer(),
                                  kNoRegionBasedLiveRangeAnalysisLimit);
  // We are using a sub-pipeline here, so that the verifier only runs after both
  // GpuHorizontalLoopFusion and HloDCE.
  auto& sub_pipeline =
      pipeline.AddPass<HloPassPipeline>("horizontal-loop-fusion-for-copy");
  // To fuse the copy.
  sub_pipeline.AddPass<CopyFusion>();
  sub_pipeline.AddPass<GpuHorizontalLoopFusion>("copy_");
  sub_pipeline.AddPass<HloDCE>();
  pipeline.AddPass<GpuSanitizeConstantNames>();
  return pipeline.Run(hlo_module).status();
}

Status GpuCompiler::OptimizeHloPostLayoutAssignment(
    HloModule* hlo_module, se::StreamExecutor* stream_exec,
    const CompileOptions& options, const GpuTargetConfig& gpu_target_config,
    const AutotuneResults* autotune_results) {
  const DebugOptions& debug_options = hlo_module->config().debug_options();

  {
    HloPassPipeline pipeline("hlo normalization");

    pipeline.AddPass<DotDimensionMerger>();

    // The LayoutAssignment pass may leave behind kCopy instructions which are
    // duplicate or NOPs, so remove them with algebraic simplification and CSE.
    AlgebraicSimplifierOptions options;
    options.set_supports_non_canonical_dots(false);
    options.set_is_layout_sensitive(true);
    options.set_enable_conv_operand_swap(false);
    // "slow" minmax means we propagate nan.
    options.set_minmax_propagate_nan(
        !debug_options.xla_gpu_enable_fast_min_max());
    options.set_enable_unconditional_reduce_of_concat_replacement(false);
    pipeline.AddPass<HloPassFix<AlgebraicSimplifier>>(options);

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
    if (debug_options.xla_gpu_enable_triton_gemm() &&
        std::holds_alternative<se::CudaComputeCapability>(
            gpu_target_config.gpu_version)) {
      auto cuda_compute_capability =
          std::get<se::CudaComputeCapability>(gpu_target_config.gpu_version);
      if (cuda_compute_capability.IsAtLeast(se::CudaComputeCapability::VOLTA)) {
        pipeline.AddPass<GemmRewriterTriton>(gpu_target_config.gpu_version);
      }
    }
    pipeline.AddPass<GemmRewriter>(gpu_target_config.gpu_version);

    // Rewrite GEMMs with broadcasted inputs as strided GEMMs.
    pipeline.AddPass<GemmBroadcastFoldingRewriter>();

    if (debug_options.xla_gpu_normalize_layouts()) {
      pipeline.AddPass<LayoutNormalization>(&NormalizeLayoutForGpuCustomCalls);
      pipeline.AddPass<HloPassFix<AlgebraicSimplifier>>(options);
    }
    pipeline.AddPass<BroadcastCanonicalizer>();

    pipeline.AddPass<ReductionDegenerateDimRemover>();
    pipeline.AddPass<ReductionLayoutNormalizer>();
    // Run Softmax fusion after layout normalization. We expect a default layout
    // in the softmax codegen pipeline. However we should run before
    // ReductionDimensionGrouper, as that makes matching the softmax pattern
    // harder.
    if (debug_options.xla_gpu_enable_triton_softmax_fusion() &&
        std::holds_alternative<se::CudaComputeCapability>(
            gpu_target_config.gpu_version)) {
      pipeline.AddPass<HloPassFix<AlgebraicSimplifier>>(options);
      pipeline.AddPass<SoftmaxRewriterTriton>(gpu_target_config.gpu_version);
    }

    pipeline.AddPass<ReductionDimensionGrouper>();
    pipeline.AddPass<HloPassFix<ReductionSplitter>>();
    pipeline.AddPass<HloPassFix<GpuTreeReductionRewriter>>(
        gpu_target_config.gpu_version);
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

  // Linearize collective schedule under SPMD partitioning if online autotuning
  // of convolutions is enabled.
  if (EnableCollectiveScheduleLinearizerForSpmd(hlo_module, stream_exec)) {
    pipeline.AddPass<CollectivesScheduleLinearizer>(
        [this](const HloModule* module) {
          return RequiresCollectiveScheduleLinearizer(module);
        });
  }

  GpuFloatSupport bf16_support(BF16);
  GpuFloatSupport f8e5m2_support(F8E5M2);
  GpuFloatSupport f8e4m3fn_support(F8E4M3FN);
  FloatSupport f8e4m3b11fnuz_support(F8E4M3B11FNUZ);
  FloatSupport f8e5m2fnuz_support(F8E5M2FNUZ);
  FloatSupport f8e4m3fnuz_support(F8E4M3FNUZ);

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
      sub_pipeline.AddPass<SimplifyFPConversions>();
    }
  };
  // Triton compilation needs normalized operations on bf16 (i.e. converted to
  // f32).
  add_float_normalization(pipeline);

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

  TF_RETURN_IF_ERROR(AddAutotuningPasses(
      &pipeline, hlo_module, stream_exec, debug_options, options,
      gpu_target_config, autotune_results, thread_pool));

  // The Triton autotuner can insert new bf16 reductions that need to be
  // normalized again.
  add_float_normalization(pipeline);

  // Clean up new_tuple described above.
  pipeline.AddPass<TupleSimplifier>();

  {
    // The LayoutAssignment pass may leave behind kCopy instructions which are
    // duplicate or NOPs, so remove them with algebraic simplification and CSE.
    AlgebraicSimplifierOptions options;
    options.set_supports_non_canonical_dots(false);
    options.set_is_layout_sensitive(true);
    options.set_enable_conv_operand_swap(false);
    // "slow" minmax means we propagate nan.
    options.set_minmax_propagate_nan(
        !hlo_module->config().debug_options().xla_gpu_enable_fast_min_max());
    options.set_enable_unconditional_reduce_of_concat_replacement(false);
    pipeline.AddPass<HloPassFix<AlgebraicSimplifier>>(options);
  }

  pipeline.AddPass<HloCSE>(/*is_layout_sensitive=*/true);
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

StatusOr<std::unique_ptr<BufferAssignment>> GpuCompiler::AssignBuffers(
    HloModule* hlo_module, se::StreamExecutor* stream_exec) {
  const GpuDeviceInfo gpu_device_info = GetGpuDeviceInfo(stream_exec);
  TF_RETURN_IF_ERROR(
      ScheduleGpuModule(hlo_module, pointer_size_, gpu_device_info));

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

  CompileModuleResults compile_module_results;
  TF_RETURN_IF_ERROR(CompileModuleToLlvmIrImpl(
      module.get(), &llvm_context, target_triple_, data_layout_,
      stream_exec->platform()->Name(), stream_exec->platform()->id(),
      gpu_device_info,
      stream_exec->GetDeviceDescription().cuda_compute_capability(),
      stream_exec->GetDeviceDescription().rocm_compute_capability(),
      GetCanShareBuffer(), pointer_size_, &compile_module_results,
      stream_exec));

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

  using BackendCompileResult = std::pair<std::string, std::vector<uint8_t>>;
  TF_ASSIGN_OR_RETURN(
      BackendCompileResult backend_result,
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

  auto buffer_assignment_proto = std::make_unique<BufferAssignmentProto>(
      compile_module_results.buffer_assignment->ToProto());

  // Make it shared to be captured in the following lambda.
  std::shared_ptr<const BufferAssignment> buffer_assignment(
      std::move(compile_module_results.buffer_assignment));

  GpuVersion gpu_version = GetGpuVersion(stream_exec);
  TF_ASSIGN_OR_RETURN(
      auto gpu_executable,
      GpuExecutable::Create(
          {std::move(backend_result.first), std::move(backend_result.second),
           gpu_version, std::move(compile_module_results.executable),
           compile_module_results.entry_func_attrs,
           std::move(compile_module_results.constants),
           std::move(compile_module_results.output_info),
           compile_module_results.module_name,
           compile_module_results.output_shape,
           std::move(compile_module_results.allocations),
           module->config()
               .debug_options()
               .xla_gpu_enable_persistent_temp_buffers(),
           std::move(buffer_assignment_proto),
           [buffer_assignment] { return buffer_assignment->ToVerboseString(); },
           std::move(module),
           /*enable_debug_info_manager=*/!options.is_autotuning_compilation}));
  if (embed_ir_in_executable) {
    DCHECK_NE("", ir_module_string_before_opt);
    gpu_executable->set_ir_module_string(ir_module_string_before_opt);
  }

  // Dump computation proto state and buffer assignment for
  // CompiledMemoryAnalysis.
  auto hlo_proto = std::make_unique<HloProto>();
  *hlo_proto->mutable_hlo_module() = gpu_executable->module().ToProto();
  *hlo_proto->mutable_buffer_assignment() = buffer_assignment->ToProto();
  gpu_executable->set_hlo_proto(std::move(hlo_proto));
  gpu_executable->set_debug_info(buffer_assignment->GetStats().ToString());
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

    // Compile the module
    CompileModuleResults compile_module_results;

    const std::any& target_config = options.target_config();
    auto* gpu_target_config = std::any_cast<GpuTargetConfig>(&target_config);

    if (gpu_target_config) {
      // CUDA "CC" major value, -1 if not available.
      se::CudaComputeCapability cuda_compute_capability{-1, -1};
      // ROCm gfx arch,  "gfx000" if not available.
      se::RocmComputeCapability rocm_compute_capability{"gfx000"};
      if (auto* cuda = std::get_if<se::CudaComputeCapability>(
              &gpu_target_config->gpu_version)) {
        cuda_compute_capability = *cuda;
      } else {
        rocm_compute_capability =
            std::get<se::RocmComputeCapability>(gpu_target_config->gpu_version);
      }

      TF_RETURN_IF_ERROR(CompileModuleToLlvmIrImpl(
          module.get(), &llvm_context, target_triple_, data_layout_,
          gpu_target_config->platform_name, options.PlatformId(),
          gpu_target_config->gpu_device_info, cuda_compute_capability,
          rocm_compute_capability, GetCanShareBuffer(), pointer_size_,
          &compile_module_results));
    } else {
      CHECK(options.executor() != nullptr);
      auto stream_exec = options.executor();
      const stream_executor::DeviceDescription& device_description =
          stream_exec->GetDeviceDescription();
      TF_RETURN_IF_ERROR(CompileModuleToLlvmIrImpl(
          module.get(), &llvm_context, target_triple_, data_layout_,
          stream_exec->platform()->Name(), options.PlatformId(),
          GetGpuDeviceInfo(stream_exec),
          device_description.cuda_compute_capability(),
          device_description.rocm_compute_capability(), GetCanShareBuffer(),
          pointer_size_, &compile_module_results));
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
              gpu_target_config->gpu_version, options.executor(),
              {options.device_allocator()}, module.get()));
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

std::optional<bool> GpuCompiler::FusionCanShareBufferHint(
    const HloInstruction* user, const HloInstruction* operand,
    const ShapeIndex& user_index) {
  if (user->opcode() != HloOpcode::kFusion) {
    return std::nullopt;
  }

  // First, do the trivial check: if the fusion operand and the fusion output
  // have a different number of elements or have a different element byte size,
  // the buffer cannot be shared.
  const Shape& user_subshape =
      ShapeUtil::GetSubshape(user->shape(), user_index);
  const Shape& operand_shape = operand->shape();
  const bool shapes_equal = ShapeUtil::Equal(operand_shape, user_subshape);
  if (!shapes_equal) {
    if (!operand_shape.IsArray() || !user_subshape.IsArray()) {
      return false;
    }
    // We cannot share the buffer if the iteration space is not the same.
    if (ShapeUtil::ElementsIn(operand_shape) !=
        ShapeUtil::ElementsIn(user_subshape)) {
      return false;
    }
    // The buffers needed for 'user_subshape' and 'operand_shape' need to have
    // the same size, otherwise they cannot be shared. We already checked that
    // the number of elements are the same, so now we check the number of bytes
    // needed for the element types.
    if (ShapeUtil::ByteSizeOfPrimitiveType(operand_shape.element_type()) !=
        ShapeUtil::ByteSizeOfPrimitiveType(user_subshape.element_type())) {
      return false;
    }
  }

  // We need to make sure that the fusion parameter is accessed in the same
  // iteration order as the fusion output. Also, there should not be two fusion
  // outputs that consume the fusion parameter, because we do not want to share
  // the same fusion operand with two different fusion outputs. To make sure
  // that the iteration order is the same, we only allow ops on the path from
  // fusion parameter to fusion output which are elementwise (no copy) or
  // bitcast or an elementwise dynamic update slice (i.e. with the first operand
  // being on this path).
  HloInstruction* fusion_param =
      user->fused_parameter(user->operand_index(operand));
  HloInstruction* output = user->fused_expression_root();
  for (int64_t o : user_index) {
    output = output->mutable_operand(o);
  }
  const HloInstruction* non_bitcast_root = output;
  if (non_bitcast_root->opcode() == HloOpcode::kBitcast) {
    non_bitcast_root = non_bitcast_root->operand(0);
  }
  std::queue<HloInstruction*> q;
  absl::flat_hash_set<HloInstruction*> visited;
  q.push(fusion_param);
  visited.insert(fusion_param);
  bool found_path_to_output = false;
  while (!q.empty()) {
    HloInstruction* hlo_operand = q.front();
    q.pop();
    if (hlo_operand == output) {
      found_path_to_output = true;
      // The output should have at most 1 user: the tuple op (in case of a
      // multi-output fusion)
      if (hlo_operand->user_count() > 1) {
        return false;
      }
      continue;
    }
    for (HloInstruction* hlo : hlo_operand->users()) {
      if (non_bitcast_root->opcode() == HloOpcode::kDynamicUpdateSlice &&
          hlo->opcode() == HloOpcode::kDynamicSlice &&
          non_bitcast_root->operand(0) == hlo->operand(0) &&
          hlo->shape() == non_bitcast_root->operand(1)->shape()) {
        // We can still share the buffer in this case if the same slice is
        // accessed by the DUS and the DS. If they don't access the same slice,
        // the two slices might partially overlap and read/write the same index
        // at different times, and then we cannot guarantee that we read before
        // it is overwritten. However if both access only a single element,
        // there also can be no race condition.
        if (!ShapeUtil::IsEffectiveScalar(hlo->shape()) ||
            !ShapeUtil::IsEffectiveScalar(
                non_bitcast_root->operand(1)->shape())) {
          // Now compare all the slice start operands of 'hlo' and
          // 'non_bitcast_root'.
          for (int64_t i = 1; i < hlo->operand_count(); ++i) {
            if (hlo->operand(i) != non_bitcast_root->operand(i + 1)) {
              return false;
            }
          }
        }
      } else if ((!hlo->IsElementwiseOnOperand(
                      hlo->operand_index(hlo_operand)) ||
                  hlo->opcode() == HloOpcode::kCopy) &&
                 hlo->opcode() != HloOpcode::kBitcast) {
        // This check also catches the case that we reach a different fusion
        // output, as that fusion output would have a tuple op as user, which we
        // do not allow here.
        // Even if 'hlo' is not elementwise on the operand, it is ok if we are
        // coming from the second operand and 'hlo' is a DynamicUpdateSlice
        // which is the non_bitcast_root. This corresponds to the special case
        // above, where we allow a DynamicSlice if it accesses the exact same
        // slice than the DynamicUpdateSlice. When we are coming from the first
        // operand, IsElementwiseOnOperand() will return true for a
        // DynamicUpdateSlice.
        if (hlo != non_bitcast_root ||
            hlo->opcode() != HloOpcode::kDynamicUpdateSlice ||
            hlo->operand_index(hlo_operand) != 1) {
          return false;
        }
      }
      if (visited.insert(hlo).second) {
        q.push(hlo);
      }
    }
  }
  return found_path_to_output;
}

}  // namespace gpu
}  // namespace xla
