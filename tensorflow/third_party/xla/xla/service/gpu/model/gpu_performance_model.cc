/* Copyright 2022 The TensorFlow Authors. All Rights Reserved.

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

#include "xla/service/gpu/model/gpu_performance_model.h"

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <optional>
#include <vector>

#include "absl/container/flat_hash_set.h"
#include "absl/log/check.h"
#include "absl/log/log.h"
#include "absl/strings/numbers.h"
#include "absl/synchronization/mutex.h"
#include "absl/time/time.h"
#include "llvm/ADT/STLExtras.h"
#include "xla/hlo/ir/hlo_casting_utils.h"
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/hlo/ir/hlo_instructions.h"
#include "xla/hlo/ir/hlo_opcode.h"
#include "xla/service/gpu/backend_configs.pb.h"
#include "xla/service/gpu/gpu_fusible.h"
#include "xla/service/gpu/hlo_fusion_analysis.h"
#include "xla/service/gpu/hlo_traversal.h"
#include "xla/service/gpu/launch_dimensions.h"
#include "xla/service/gpu/model/gpu_hlo_cost_analysis.h"
#include "xla/service/hlo_dataflow_analysis.h"
#include "xla/shape_util.h"
#include "xla/stream_executor/device_description.h"
#include "xla/util.h"
#include "tsl/platform/status.h"

#if GOOGLE_CUDA
#include "third_party/gpus/cuda/nvml/include/nvml.h"
#endif  // GOOGLE_CUDA
namespace xla {
namespace gpu {

namespace {

// Estimated values in the absence of easy ways to query them.
static constexpr absl::Duration kKernelLaunchOverhead = absl::Microseconds(1);
static constexpr absl::Duration kNcclKernelLaunchOverhead =
    absl::Microseconds(5);
static constexpr float kL2CacheSpeedup = 2.5;
static constexpr float kL1CacheSpeedup = 8;
// A very conservative estimate. L1 size varies because it can be dynamically
// configured as shared memory; there is no easy way to query its actual size;
// also we do not count what occupies cache, but rather claim that what is
// much smaller than the cache size will likely stay in it.
// For reference, it can be up to 256 kB per SM on RTX A6000.
static constexpr float kL1CacheSizePerSM = 2 * 1024;

// Returns whether a fusion uses the parameter at the given index elementwise
// from its root.
bool FusionUsesParameterElementwiseFromRoot(
    const HloInstruction* fusion, int parameter_index,
    const GpuHloCostAnalysis* cost_analysis) {
  return cost_analysis->CommonElementwiseUtilization(
             fusion->fused_parameter(parameter_index),
             fusion->fused_expression_root()) == 1.f;
}

int GetCoalescingWasteFactor(PrimitiveType element_type) {
  int64_t element_size_bytes =
      element_type == PrimitiveType::TUPLE ||
              element_type == PrimitiveType::TOKEN
          ? 4 /* Dummy value. TODO(jreiffers): Model this case. */
          : ShapeUtil::ByteSizeOfPrimitiveType(element_type);
  // Cache line is 128B that is split into 4 sectors of 32B. Default transaction
  // size from DRAM -> L2 = 64 Bytes = 2 sectors, since V100, but it can be also
  // configured.
  // https://developer.download.nvidia.com/video/gputechconf/gtc/2020/presentations/s21819-optimizing-applications-for-nvidia-ampere-gpu-architecture.pdf
  // (page 10).
  constexpr int kDRAMToL2TransactionSizeBytes = 64;
  // Assume we use one element from the cache line and waste the remaining
  // bandwidth. For example, if we're reading f32s, we use 1/16nd of the cache
  // line.
  return kDRAMToL2TransactionSizeBytes / element_size_bytes;
}

// Estimate read time of n_bytes_total bytes from global memory on a
// given GPU. Account for L1 / L2 cache speedup if the input's nominal size
// n_bytes_net is small.
absl::Duration ReadTime(const se::DeviceDescription& gpu_device_info,
                        int64_t num_blocks, int64_t n_bytes_net,
                        int64_t n_bytes_total, PrimitiveType element_type,
                        bool coalesced, bool first_read_from_dram) {
  int waste_factor = coalesced ? 1 : GetCoalescingWasteFactor(element_type);

  // Limit the bandwidth for low occupancy cases. Each SM can issue at most
  // one 32B memory transaction per clock. H100 needs at least 56.8 active SMs
  // (1830 MHz) to saturate the memory bandwidth (3.35 TB/s).
  float per_block_bandwidth = gpu_device_info.clock_rate_ghz() * 1.0e9f * 32;
  float max_bandwidth = num_blocks * per_block_bandwidth;

  if (first_read_from_dram) {
    // The first read of the input buffer always happens from DRAM. If reads are
    // no coaleced, bandwidth is reduced by the waste factor.
    float dram_bandwidth = gpu_device_info.memory_bandwidth() / waste_factor;

    // Two things can happed on re-reading the buffer:
    //   - If the buffer fits into cache, the L1/L2 cache speedup is applied.
    //   - If the buffer doesn't fit, it will be read from DRAM and the same
    //     coalessing waste factor is applied.
    float rest_bandwidth = gpu_device_info.memory_bandwidth();
    if (n_bytes_net < gpu_device_info.l2_cache_size()) {
      rest_bandwidth *= kL2CacheSpeedup;
      if (n_bytes_net < kL1CacheSizePerSM * gpu_device_info.core_count()) {
        rest_bandwidth *= kL1CacheSpeedup;
      }
    } else {
      rest_bandwidth /= waste_factor;
    }

    dram_bandwidth = std::min(dram_bandwidth, max_bandwidth);
    rest_bandwidth = std::min(rest_bandwidth, max_bandwidth);

    // n_bytes_net > n_bytes_total can happend when we compute read time of
    // shared operand. This is a flaw in the interface that should be fixed.
    int64_t n_bytes_read_dram = std::min(n_bytes_net, n_bytes_total);

    // Number of bytes that we be re-read, potentially from cache.
    int64_t n_bytes_read_cache = n_bytes_total - n_bytes_read_dram;

    return absl::Seconds(n_bytes_read_dram / dram_bandwidth) +
           absl::Seconds(n_bytes_read_cache / rest_bandwidth);
  } else {
    float bandwidth = gpu_device_info.memory_bandwidth();
    if (n_bytes_net < gpu_device_info.l2_cache_size()) {
      bandwidth *= kL2CacheSpeedup;
      if (n_bytes_net < kL1CacheSizePerSM * gpu_device_info.core_count()) {
        bandwidth *= kL1CacheSpeedup;
      }
    } else if (!coalesced) {
      bandwidth /= waste_factor;
    }

    bandwidth = std::min(bandwidth, max_bandwidth);
    return absl::Seconds(n_bytes_total / bandwidth);
  }
}

int64_t GetNcclMaxNumChannels(
    GpuPerformanceWithCollectiveModel::CollectiveAlgo algorithm) {
  int64_t max_nchannels = 0;
  switch (algorithm) {
    // Tree and Ring algos share the same max channel number.
    case GpuPerformanceWithCollectiveModel::RING:
    case GpuPerformanceWithCollectiveModel::TREE:
      max_nchannels = GpuPerformanceWithCollectiveModel::kMaxNumChannelsRing;
      break;
  }
  const char* env = std::getenv("NCCL_MAX_NCHANNELS");
  if (env != nullptr) {
    int64_t max_nchannels_from_env;
    if (absl::SimpleAtoi(env, &max_nchannels_from_env)) {
      max_nchannels = std::min(max_nchannels_from_env, max_nchannels);
    }
  }
  return max_nchannels;
}

int64_t GetMinNumberOfChannels(
    GpuPerformanceWithCollectiveModel::CollectiveAlgo algorithm) {
  int64_t min_nchannels = 0;
  switch (algorithm) {
    // Tree and Ring algos share the same min channel number.
    case GpuPerformanceWithCollectiveModel::RING:
    case GpuPerformanceWithCollectiveModel::TREE:
      min_nchannels = 1;
      break;
  }
  const char* env = std::getenv("NCCL_MIN_NCHANNELS");
  if (env != nullptr) {
    int64_t min_nchannels_from_env;
    if (absl::SimpleAtoi(env, &min_nchannels_from_env)) {
      min_nchannels = std::min(min_nchannels_from_env, min_nchannels);
    }
  }
  return min_nchannels;
}

int GetNumThreads(int warp_size, int min_num_threads, int max_num_threads,
                  int default_num_threads) {
  int threads_from_env = default_num_threads;
  const char* env = std::getenv("NCCL_NTHREADS");
  if (env != nullptr) {
    CHECK(absl::SimpleAtoi(env, &threads_from_env));
  }
  int num_threads = threads_from_env;
  if (num_threads > 0) {
    if (num_threads % warp_size != 0) {
      num_threads = max_num_threads;
    } else if (num_threads > max_num_threads) {
      num_threads = max_num_threads;
    } else if (num_threads < min_num_threads) {
      num_threads = min_num_threads;
    }
  } else {
    num_threads = default_num_threads;
  }
  return num_threads;
}

float GetMaxSysBwFromGpu(const se::CudaComputeCapability cc,
                         const double* bandwidths_table) {
  switch (cc.major) {
    case se::CudaComputeCapability::VOLTA:
      return bandwidths_table[0];
    case se::CudaComputeCapability::AMPERE:
      return bandwidths_table[1];
    case se::CudaComputeCapability::HOPPER:
      return bandwidths_table[2];
  }
  return -1;
}

// Uses HloFusionAnalysis for computing the actual number of threads and blocks
// that the IR emitter will use.
LaunchDimensions EstimateFusionLaunchDimensions(
    int64_t estimated_num_threads,
    const std::optional<HloFusionAnalysis>& fusion_analysis,
    const se::DeviceDescription& device_info) {
  if (fusion_analysis) {
    // TODO(jreiffers): This is the wrong place for this DUS analysis.
    const HloInstruction* dus = nullptr;
    for (const auto* root : fusion_analysis->fusion_roots()) {
      if (root->opcode() == HloOpcode::kDynamicUpdateSlice) {
        dus = root;
      } else if (root->opcode() == HloOpcode::kBitcast &&
                 root->operand(0)->opcode() == HloOpcode::kDynamicUpdateSlice) {
        dus = root->operand(0);
      } else {
        dus = nullptr;
        break;
      }
    }

    if (dus) {
      if (auto dims =
              CalculateLaunchDimensions(dus->operand(1)->shape(), device_info);
          dims.ok()) {
        return dims.value();
      }
    }

    auto launch_dimensions = fusion_analysis->GetLaunchDimensions();
    if (launch_dimensions.ok()) return *launch_dimensions;
  }
  int64_t block_size = 128;  // Result for default LaunchDimensionsConfig.
  int64_t num_blocks = CeilOfRatio(estimated_num_threads, block_size);
  return LaunchDimensions(num_blocks, block_size);
}

// Returns true if all input reads are coalesced. If consumer is not nullptr,
// producer and consumer are considered as one fusion, otherwise it's only the
// producer.
//
// This is a crude heuristic until we get proper tile analysis.
bool IsReadCoalesced(const std::optional<HloFusionAnalysis>& fusion_analysis,
                     const GpuPerformanceModelOptions& config,
                     const HloInstruction* producer,
                     const HloInstruction* consumer = nullptr) {
  if (!config.consider_coalescing) return true;

  auto analyzed_kind_or_reduction =
      fusion_analysis ? fusion_analysis->GetEmitterFusionKind()
                      : HloFusionAnalysis::EmitterFusionKind::kReduction;

  // Transposing minor dimension breaks coalescing.
  if (analyzed_kind_or_reduction !=
      HloFusionAnalysis::EmitterFusionKind::kTranspose) {
    auto is_broadcast = [&](const HloInstruction* instr) {
      while (true) {
        if (instr->opcode() == HloOpcode::kBroadcast) return true;
        if (instr->operand_count() != 1) return false;
        if (instr->opcode() != HloOpcode::kBitcast && !instr->IsElementwise()) {
          return false;
        }
        instr = instr->operand(0);
      }
    };

    auto is_bad_transpose = [&](const HloInstruction* instr) {
      if (instr->opcode() == HloOpcode::kFusion) {
        for (auto* instr : instr->fused_instructions()) {
          // Hack: we allow transposes of broadcasts.
          if (TransposesMinorDimension(instr) &&
              !is_broadcast(instr->operand(0))) {
            return true;
          }
        }
        return false;
      }
      return TransposesMinorDimension(instr);
    };

    if (is_bad_transpose(producer)) return false;
    if (consumer && is_bad_transpose(consumer)) return false;
  }

  // Fusing two row reductions breaks coalescing.
  if (analyzed_kind_or_reduction ==
          HloFusionAnalysis::EmitterFusionKind::kReduction &&
      IsInputFusibleReduction(*producer) && consumer &&
      IsInputFusibleReduction(*consumer)) {
    return false;
  }

  return true;
}

}  // namespace

std::optional<EstimateRunTimeData> GpuPerformanceModelCache::Get(
    const HloInstruction& instruction) {
  absl::MutexLock lock(&mutex_);

  auto it = instruction_runtime_data_.find(HloInstructionAdaptor(instruction));
  if (it != instruction_runtime_data_.end()) {
    return it->second;
  }
  return std::nullopt;
}

std::optional<absl::Duration> GpuPerformanceModelCache::Get(
    const HloInstruction& producer, const HloInstruction& consumer) {
  absl::MutexLock lock(&mutex_);

  auto it = fusion_runtime_data_.find(HloInstructionAdaptor(producer));
  if (it != fusion_runtime_data_.end()) {
    auto jt = it->second.find(HloInstructionAdaptor(consumer));
    if (jt != it->second.end()) {
      return jt->second;
    }
  }
  return std::nullopt;
}

void GpuPerformanceModelCache::Set(const HloInstruction& instruction,
                                   const EstimateRunTimeData& runtime_data) {
  absl::MutexLock lock(&mutex_);

  instruction_runtime_data_[HloInstructionAdaptor(instruction)] = runtime_data;
}

void GpuPerformanceModelCache::Set(const HloInstruction& producer,
                                   const HloInstruction& consumer,
                                   absl::Duration runtime) {
  absl::MutexLock lock(&mutex_);
  fusion_runtime_data_[HloInstructionAdaptor(producer)]
                      [HloInstructionAdaptor(consumer)] = runtime;
}

void GpuPerformanceModelCache::Invalidate(const HloInstruction& instruction) {
  absl::MutexLock lock(&mutex_);
  HloInstructionAdaptor adaptor(instruction);

  // Remove runtime data for the instruction.
  instruction_runtime_data_.erase(adaptor);

  // Remove cache for all producer-consumer pairs where the instruction is
  // producer.
  fusion_runtime_data_.erase(adaptor);

  // Iterate through operands to find all producer-consumer pairs where
  // instruction is consumer and remove them from cache.
  for (auto* operand : instruction.operands()) {
    auto it = fusion_runtime_data_.find(HloInstructionAdaptor(*operand));
    if (it != fusion_runtime_data_.end()) {
      it->second.erase(adaptor);
    }
  }
}

/*static*/ EstimateRunTimeData
GpuPerformanceModel::EstimateRunTimeForInstruction(
    const HloInstruction* instr, const GpuHloCostAnalysis* cost_analysis,
    const GpuPerformanceModelOptions& config) {
  const se::DeviceDescription* device_info = cost_analysis->device_info_;

  int64_t flops = cost_analysis->flop_count(*instr);
  int64_t bytes_written = cost_analysis->output_bytes_accessed(*instr);
  int64_t bytes_read = cost_analysis->bytes_accessed(*instr) - bytes_written;

  // Use the analysis cache if present.
  // TODO(jreiffers): Remove this once all callers use a cache.
  std::optional<HloFusionAnalysis> local_analysis =
      config.fusion_analysis_cache
          ? std::nullopt
          : AnalyzeFusion(*instr, *cost_analysis->device_info_);
  const auto& fusion_analysis = config.fusion_analysis_cache
                                    ? config.fusion_analysis_cache->Get(*instr)
                                    : local_analysis;
  LaunchDimensions launch_dimensions = EstimateFusionLaunchDimensions(
      ShapeUtil::ElementsInRecursive(instr->shape()), fusion_analysis,
      *device_info);
  int64_t num_threads = launch_dimensions.launch_bound();

  absl::Duration compute_time = ComputeTime(*device_info, flops, num_threads);
  absl::Duration read_time = ProducerInputAccessTime(
      cost_analysis, *device_info, launch_dimensions.num_blocks(),
      /*producer=*/instr, fusion_analysis, config);
  absl::Duration write_time =
      absl::Seconds(1.0f * bytes_written / device_info->memory_bandwidth());
  absl::Duration exec_time = std::max(compute_time, read_time + write_time);

  if (VLOG_IS_ON(8)) {
    LOG(INFO) << "FLOPs: " << flops;
    LOG(INFO) << "Bytes read: " << bytes_read;
    LOG(INFO) << "Bytes written: " << bytes_written;
    LOG(INFO) << "Num threads:" << num_threads;
    LOG(INFO) << "Compute time: " << compute_time;
    LOG(INFO) << "Input read time: " << read_time;
    LOG(INFO) << "Output write time: " << write_time;
  }

  return {flops, bytes_written, num_threads, write_time, exec_time};
}

/*static*/ EstimateRunTimeData
GpuPerformanceModel::EstimateRunTimeForInstructionCached(
    const HloInstruction* instr, const GpuHloCostAnalysis* cost_analysis,
    const GpuPerformanceModelOptions& config) {
  if (config.gpu_performance_model_cache) {
    if (auto cached_result = config.gpu_performance_model_cache->Get(*instr)) {
      return *cached_result;
    }
  }

  auto runtime_data =
      EstimateRunTimeForInstruction(instr, cost_analysis, config);

  if (config.gpu_performance_model_cache) {
    config.gpu_performance_model_cache->Set(*instr, runtime_data);
  }

  return runtime_data;
}

// Returns utilization of operand by instruction. Returns 0, if the operand is
// not used by the instruction.
float GetOperandUtilization(const GpuHloCostAnalysis* cost_analysis,
                            const HloInstruction* instr,
                            const HloInstruction* operand) {
  if (!instr->IsUserOf(operand)) {
    return 0.f;
  }

  return cost_analysis->operand_utilization(*instr,
                                            instr->operand_index(operand));
}

// Returns utilization `overlap` between a common operand of producer and
// consumer on merge. `utilization > 0` means that the operand will be accessed
// more efficiently after fusion.
//
// Currently covers two cases:
// 1) Producer has to use the common operand elementwise from its root if it is
//    a fusion or just be an elementwise instruction.
// 2) Consumer has to have common elementwise roots for the producer and the
//    common operand if it is a fusion or just be an elementwise instruction.
float GetCommonUtilization(const GpuHloCostAnalysis* cost_analysis,
                           const HloInstruction* producer,
                           int64_t producer_idx_of_operand,
                           const HloInstruction* consumer) {
  const auto* operand = producer->operand(producer_idx_of_operand);

  if (!consumer || !consumer->IsUserOf(operand)) {
    return 0.f;
  }

  if (producer->IsElementwise() ||
      (producer->opcode() == HloOpcode::kFusion &&
       FusionUsesParameterElementwiseFromRoot(producer, producer_idx_of_operand,
                                              cost_analysis))) {
    if (consumer->opcode() == HloOpcode::kFusion) {
      int64_t consumer_idx_of_common_operand = consumer->operand_index(operand);
      int64_t consumer_idx_of_producer = consumer->operand_index(producer);
      return cost_analysis->CommonElementwiseUtilization(
          consumer->fused_parameter(consumer_idx_of_common_operand),
          consumer->fused_parameter(consumer_idx_of_producer));
    } else {
      if (consumer->IsElementwise()) {
        return 1.f;
      }
    }
  }
  return 0.f;
}

// Returns utilization of operand after producer and consumer are fused
// together. `GetCommonUtilization` works only for a limited set of elementwise
// cases.
// TODO(shyshkov): Combine logic from GpuHloCostAnalysis with boundary function
// to properly calculate utilization.
float GetSharedUtilization(const GpuHloCostAnalysis* cost_analysis,
                           const HloInstruction* producer,
                           const HloInstruction* consumer,
                           const HloInstruction* operand) {
  float producer_utilization_by_consumer =
      GetOperandUtilization(cost_analysis, consumer, producer);

  float operand_utilization_by_producer =
      GetOperandUtilization(cost_analysis, producer, operand);

  float operand_utilization_by_consumer =
      GetOperandUtilization(cost_analysis, consumer, operand);

  float common_utilization =
      producer->IsUserOf(operand)
          ? GetCommonUtilization(cost_analysis, producer,
                                 producer->operand_index(operand), consumer)
          : 0.f;

  return producer_utilization_by_consumer * operand_utilization_by_producer +
         operand_utilization_by_consumer - common_utilization;
}

// Tells input access time of the producer alone if fused_consumer
// is not specified. Otherwise estimates the access time to producer's
// inputs as if it is fused into the consumer.
/*static*/ absl::Duration GpuPerformanceModel::ProducerInputAccessTime(
    const GpuHloCostAnalysis* cost_analysis,
    const se::DeviceDescription& gpu_device_info, int64_t num_blocks,
    const HloInstruction* producer,
    const std::optional<HloFusionAnalysis>& fusion_analysis,
    const GpuPerformanceModelOptions& config,
    const HloInstruction* fused_consumer) {
  absl::Duration ret = absl::ZeroDuration();
  float producer_output_utilization =
      fused_consumer
          ? GetOperandUtilization(cost_analysis, fused_consumer, producer)
          : 1.f;

  // TODO(jreiffers): We should be checking each operand.
  bool coalesced =
      IsReadCoalesced(fusion_analysis, config, producer, fused_consumer);
  for (int i = 0; i < producer->operand_count(); ++i) {
    // Information about data read taking into account utilization.
    // If `operand_utilization` is 0, `operand_bytes_accessed` should be also 0.
    int64_t operand_bytes_accessed =
        cost_analysis->operand_bytes_accessed(*producer, i);
    float operand_utilization =
        cost_analysis->operand_utilization(*producer, i);

    // An estimate how much data would need to fit into L1/L2 cache to speed up
    // the operand access.
    // If `operand_utilization` < 1, only a part of the full operand size should
    // be read. Otherwise, `operand_bytes_accessed / operand_utilization` is the
    // size of the operand without reuse.
    int64_t n_bytes_net = std::llround(operand_bytes_accessed /
                                       std::max(operand_utilization, 1.0f));

    // Look if common operand of producer and consumer will be accessed more
    // efficiently on merge.
    float common_utilization = GetCommonUtilization(
        cost_analysis, producer, /*producer_idx_of_operand=*/i, fused_consumer);

    const auto& operand_shape = producer->operand(i)->shape();

    CHECK_LE(common_utilization, producer_output_utilization);
    float n_bytes_total = operand_bytes_accessed *
                          (producer_output_utilization - common_utilization);
    ret += ReadTime(gpu_device_info, num_blocks, /*n_bytes_net=*/n_bytes_net,
                    n_bytes_total, operand_shape.element_type(), coalesced,
                    config.first_read_from_dram);
  }
  return ret;
}

absl::Duration GpuPerformanceModel::ComputeTime(
    const se::DeviceDescription& gpu_device_info, int64_t flops,
    int64_t num_threads) {
  int64_t fpu_count =
      gpu_device_info.core_count() * gpu_device_info.fpus_per_core();
  int64_t n_threads_active = std::min(num_threads, fpu_count);
  int64_t flop_per_ns_per_fpu = gpu_device_info.clock_rate_ghz() * /*fma:*/ 2;
  int64_t flop_per_ns_effective = flop_per_ns_per_fpu * n_threads_active;
  return absl::Nanoseconds(1.0f * flops / flop_per_ns_effective);
}

absl::Duration GpuPerformanceModel::EstimateUnfusedExecTime(
    const HloInstruction* producer, const EstimateRunTimeData& producer_runtime,
    const GpuHloCostAnalysis* cost_analysis,
    const GpuPerformanceModelOptions& config,
    const std::vector<HloInstruction*>& fused_consumers,
    const std::vector<EstimateRunTimeData>& consumer_runtimes) {
  const se::DeviceDescription* device_info = cost_analysis->device_info_;

  absl::Duration time_unfused =
      kKernelLaunchOverhead * (fused_consumers.size() + 1) +
      producer_runtime.exec_time;

  if (config.calculate_full_priority) {
    for (const auto& consumer_runtime : consumer_runtimes) {
      time_unfused += consumer_runtime.exec_time;
    }
    return time_unfused;
  }

  for (const HloInstruction* fused_consumer : fused_consumers) {
    VLOG(8) << "Unfused consumer: " << fused_consumer->name();
    float utilization_by_this_consumer =
        GetOperandUtilization(cost_analysis, fused_consumer, producer);

    // Use the analysis cache if present.
    // TODO(jreiffers): Remove this once all callers use a cache.
    std::optional<HloFusionAnalysis> local_analysis =
        config.fusion_analysis_cache
            ? std::nullopt
            : AnalyzeFusion(*fused_consumer, *device_info);
    const auto& analysis_unfused =
        config.fusion_analysis_cache
            ? config.fusion_analysis_cache->Get(*fused_consumer)
            : local_analysis;

    LaunchDimensions launch_dimensions_unfused = EstimateFusionLaunchDimensions(
        ShapeUtil::ElementsInRecursive(fused_consumer->shape()),
        analysis_unfused, *device_info);

    int64_t n_bytes_total = std::llround(producer_runtime.bytes_written *
                                         utilization_by_this_consumer);
    int64_t n_bytes_net =
        std::min(producer_runtime.bytes_written, n_bytes_total);

    bool coalesced =
        IsReadCoalesced(analysis_unfused, config, /*producer=*/fused_consumer);
    auto read_time_unfused = ReadTime(
        *device_info, launch_dimensions_unfused.num_blocks(), n_bytes_net,
        n_bytes_total, fused_consumer->shape().element_type(), coalesced,
        config.first_read_from_dram);

    VLOG(10) << "  Read time unfused: " << read_time_unfused;
    time_unfused += read_time_unfused;
  }

  return time_unfused;
}

/*static*/ absl::Duration GpuPerformanceModel::EstimateRunTimeForFusion(
    const HloInstruction* producer, const HloInstruction* consumer,
    const EstimateRunTimeData& producer_runtime,
    const EstimateRunTimeData& consumer_runtime,
    const LaunchDimensions& launch_dimensions,
    float utilization_by_this_consumer, const GpuHloCostAnalysis* cost_analysis,
    const std::optional<HloFusionAnalysis>& fusion_analysis,
    const GpuPerformanceModelOptions& config) {
  const se::DeviceDescription* device_info = cost_analysis->device_info_;

  int64_t fused_flops = producer_runtime.flops * utilization_by_this_consumer +
                        consumer_runtime.flops;

  absl::Duration compute_time =
      ComputeTime(*device_info, fused_flops, launch_dimensions.launch_bound());

  absl::flat_hash_set<const HloInstruction*> fusion_operands;
  for (auto* operand : producer->operands()) {
    fusion_operands.insert(operand);
  }
  for (auto* operand : consumer->operands()) {
    if (operand != producer) {
      fusion_operands.insert(operand);
    }
  }

  absl::Duration read_time;
  for (const auto* operand : fusion_operands) {
    float operand_utilization =
        GetSharedUtilization(cost_analysis, producer, consumer, operand);

    int64_t operand_size = cost_analysis->GetShapeSize(operand->shape());

    int64_t n_bytes_total = std::llround(operand_size * operand_utilization);
    int64_t n_bytes_net = std::min(operand_size, n_bytes_total);

    bool coalesced =
        IsReadCoalesced(fusion_analysis, config, producer, consumer);

    read_time +=
        ReadTime(*device_info, launch_dimensions.num_blocks(), n_bytes_net,
                 n_bytes_total, operand->shape().element_type(), coalesced,
                 config.first_read_from_dram);
  }

  return std::max(compute_time, read_time + consumer_runtime.write_time);
}

absl::Duration GpuPerformanceModel::EstimateFusedExecTime(
    const HloInstruction* producer, const EstimateRunTimeData& producer_runtime,
    const GpuHloCostAnalysis* cost_analysis,
    const GpuPerformanceModelOptions& config,
    const std::vector<HloInstruction*>& fused_consumers,
    const std::vector<EstimateRunTimeData>& consumer_runtimes,
    bool multi_output) {
  const se::DeviceDescription* device_info = cost_analysis->device_info_;

  absl::Duration exec_time_fused =
      kKernelLaunchOverhead * fused_consumers.size();
  for (auto [idx, fused_consumer] : llvm::enumerate(fused_consumers)) {
    VLOG(8) << "Fused consumer: " << fused_consumer->name();

    if (config.calculate_full_priority && config.gpu_performance_model_cache) {
      if (auto fusion_runtime = config.gpu_performance_model_cache->Get(
              *producer, *fused_consumer)) {
        exec_time_fused += *fusion_runtime;
        continue;
      }
    }

    float utilization_by_this_consumer = cost_analysis->operand_utilization(
        *fused_consumer, fused_consumer->operand_index(producer));

    std::optional<HloFusionAnalysis> local_analysis_fused =
        config.fusion_analysis_cache
            ? std::nullopt
            : AnalyzeProducerConsumerFusion(*producer, *fused_consumer,
                                            *device_info);
    const auto& analysis_fused =
        config.fusion_analysis_cache
            ? config.fusion_analysis_cache->Get(*producer, *fused_consumer)
            : local_analysis_fused;

    LaunchDimensions launch_dimensions_fused = EstimateFusionLaunchDimensions(
        producer_runtime.num_threads * utilization_by_this_consumer,
        analysis_fused, *device_info);

    // The original model ignores consumer computation and output writes. The
    // main goal of the model is to compare estimates of fused and unfused
    // cases. Since epilog of the consumers remains unchanged in both bases, we
    // only consider duplication of the producer computation and repeated access
    // to producer inputs.
    //
    // With `calculate_full_priority`, consumer computation and full read time
    // is accounted in the priority.
    if (config.calculate_full_priority) {
      auto fusion_runtime = EstimateRunTimeForFusion(
          producer, fused_consumer, producer_runtime, consumer_runtimes[idx],
          launch_dimensions_fused, utilization_by_this_consumer, cost_analysis,
          analysis_fused, config);
      exec_time_fused += fusion_runtime;
      if (config.gpu_performance_model_cache) {
        config.gpu_performance_model_cache->Set(*producer, *fused_consumer,
                                                fusion_runtime);
      }
      continue;
    }

    absl::Duration compute_time_by_this_consumer = ComputeTime(
        *device_info, producer_runtime.flops * utilization_by_this_consumer,
        launch_dimensions_fused.launch_bound());

    // Here, we assume that the read is distributed over all the threads in the
    // launch grid. Usually this is the case, but not always: for example, a
    // reduce -> broadcast -> elementwise fusion will recompute the reduce. We
    // don't currently have an analysis that is able to detect these cases.
    absl::Duration input_access_time_by_this_consumer = ProducerInputAccessTime(
        cost_analysis, *device_info, launch_dimensions_fused.num_blocks(),
        producer, analysis_fused, config, fused_consumer);
    VLOG(10) << "  Compute time by consumer: " << compute_time_by_this_consumer;
    VLOG(10) << "  Input access time by consumer: "
             << input_access_time_by_this_consumer;

    exec_time_fused += std::max(compute_time_by_this_consumer,
                                input_access_time_by_this_consumer);
  }

  // Multi-output fusion still writes the initial output of the producer.
  // For now assume that the producer's output does not need to be recomputed.
  if (multi_output) {
    exec_time_fused += producer_runtime.write_time;
  }

  return exec_time_fused;
}

GpuPerformanceModel::RunTimes GpuPerformanceModel::EstimateRunTimes(
    const HloInstruction* producer, const GpuHloCostAnalysis* cost_analysis,
    const GpuPerformanceModelOptions& config,
    std::vector<HloInstruction*> fused_consumers, bool multi_output) {
  VLOG(8) << "Producer: " << producer->name();
  if (producer->opcode() == HloOpcode::kFusion) {
    VLOG(10) << producer->fused_instructions_computation()->ToString();
  }

  EstimateRunTimeData producer_runtime =
      EstimateRunTimeForInstructionCached(producer, cost_analysis, config);

  std::vector<EstimateRunTimeData> consumer_runtimes;
  if (config.calculate_full_priority) {
    consumer_runtimes.reserve(fused_consumers.size());
    for (auto* consumer : fused_consumers) {
      consumer_runtimes.push_back(
          EstimateRunTimeForInstructionCached(consumer, cost_analysis, config));
    }
  }

  absl::Duration time_unfused =
      EstimateUnfusedExecTime(producer, producer_runtime, cost_analysis, config,
                              fused_consumers, consumer_runtimes);

  absl::Duration time_fused =
      EstimateFusedExecTime(producer, producer_runtime, cost_analysis, config,
                            fused_consumers, consumer_runtimes, multi_output);

  int64_t fused_consumer_count = fused_consumers.size();
  float total_producer_utilization = 0;

  for (const HloInstruction* fused_consumer : fused_consumers) {
    float utilization_by_this_consumer = cost_analysis->operand_utilization(
        *fused_consumer, fused_consumer->operand_index(producer));
    total_producer_utilization += utilization_by_this_consumer;
  }

  if (VLOG_IS_ON(8)) {
    LOG(INFO) << "Consumer count: " << fused_consumer_count;
    LOG(INFO) << "Utilization of producer output: "
              << total_producer_utilization;
    LOG(INFO) << "Unfused time: " << time_unfused;
    LOG(INFO) << "Fused time: " << time_fused;
  }

  return {time_unfused, time_fused};
}

void GpuPerformanceModel::RecordEstimatedRunTime(
    HloInstruction* instruction, const GpuHloCostAnalysis* cost_analysis,
    const GpuPerformanceModelOptions& config) {
  DCHECK(Cast<const HloFusionInstruction>(instruction)) << "expected fusion";
  DCHECK(cost_analysis != nullptr) << "expected cost analysis";

  EstimateRunTimeData data =
      EstimateRunTimeForInstructionCached(instruction, cost_analysis, config);
  double cycles = absl::ToDoubleNanoseconds(data.exec_time) *
                  cost_analysis->device_info_->clock_rate_ghz();

  auto backend_config = instruction->backend_config<FusionBackendConfig>();
  TF_CHECK_OK(backend_config.status()) << instruction->ToString();
  backend_config->mutable_reification_cost()->set_end_to_end_cycles(cycles);
  TF_CHECK_OK(instruction->set_backend_config(*backend_config));

  VLOG(8) << "RecordEstimatedRunTime: " << instruction->ToString();
}

// Returns NVLink bw in GB/s
/*static*/
float GpuPerformanceWithCollectiveModel::GetNvlinkBw(
    se::CudaComputeCapability compute_capability) {
  return compute_capability.IsAtLeast(se::CudaComputeCapability::HOPPER)
             ? kSm90NvlinkBandwidth
         : compute_capability.IsAtLeast(se::CudaComputeCapability::AMPERE)
             ? kSm80NvlinkBandwidth
         : compute_capability.IsAtLeast(se::CudaComputeCapability::VOLTA)
             ? kSm70NvlinkBandwidth
         : compute_capability.IsAtLeast(se::CudaComputeCapability::PASCAL_)
             ? kSm60NvlinkBandwidth
             : kSm80NvlinkBandwidth;
}

/*static*/ bool GpuPerformanceWithCollectiveModel::InitNvml() {
#if GOOGLE_CUDA
  void* libhandle = dlopen("libnvidia-ml.so.1", RTLD_NOW);
  CHECK(libhandle != nullptr) << "Failed to open libnvidia-ml.so.1";

  struct SymbolEntry {
    void** functor;
    char const* name;
  };

  std::vector<SymbolEntry> symbols = {
      {(void**)&xla_nvmlInit, "nvmlInit_v2"},
      {(void**)&xla_nvmlShutdown, "nvmlShutdown"},
      {(void**)&xla_nvmlDeviceGetHandleByIndex, "nvmlDeviceGetHandleByIndex"},
      {(void**)&xla_nvmlDeviceGetNvLinkCapability,
       "nvmlDeviceGetNvLinkCapability"},
  };
  for (SymbolEntry se : symbols) {
    *se.functor = dlsym(libhandle, se.name);
  }
  nvmlReturn_t init_result = xla_nvmlInit();
  return init_result == NVML_SUCCESS;
#else
  return false;
#endif  // GOOGLE_CUDA
}

/*static*/ bool GpuPerformanceWithCollectiveModel::ShutdownNvml() {
#if GOOGLE_CUDA
  nvmlReturn_t shutdown_result = xla_nvmlShutdown();
  return shutdown_result == NVML_SUCCESS;
#else
  return false;
#endif  // GOOGLE_CUDA
}

/*static*/ uint32_t
GpuPerformanceWithCollectiveModel::CheckIfNvlinkSupportsP2P() {
#if GOOGLE_CUDA
  // We will use nvml library to detect nvlink capability
  // to see if it supports p2p communication.
  // We first load libnvidia-ml.so and assign symbols to function pointers
  // to avoid linking errors.
  // Then gpu 0 will be used to query for nvlink capability, note that
  // we only look at link 0 of gpu 0 since all other links are assumed
  // to have the same capability.
  CHECK(InitNvml()) << "NVML init failed.";
  nvmlDevice_t nvml_device;
  nvmlReturn_t get_device_result =
      xla_nvmlDeviceGetHandleByIndex(0, &nvml_device);
  CHECK(get_device_result == NVML_SUCCESS);

  uint32_t supported_p2p = 0;

  nvmlReturn_t nvlink_cap_result = xla_nvmlDeviceGetNvLinkCapability(
      nvml_device, /*nvlink link number*/ 0, NVML_NVLINK_CAP_P2P_SUPPORTED,
      &supported_p2p);
  CHECK(nvlink_cap_result == NVML_SUCCESS);
  CHECK(ShutdownNvml()) << "NVML shutdown failed.";
  return supported_p2p;
#else
  return 0;
#endif  // GOOGLE_CUDA
}

/*static*/ absl::Duration
GpuPerformanceWithCollectiveModel::ComputeAllreduceTime(
    const HloInstruction& instr, const GpuHloCostAnalysis* cost_analysis,
    const se::DeviceDescription& gpu_device_info) {
  // We use nccl group call to launch multiple allreduces so launch overhead
  // only occurs once.
  absl::Duration total_time = kNcclKernelLaunchOverhead;
  stream_executor::CudaComputeCapability compute_cap =
      gpu_device_info.cuda_compute_capability();

  int64_t size_of_speed_array = kIntraNodeSpeeds.size();
  int64_t size_of_sm90_speed_array = kIntraNodeSpeedsSm90.size();

  int num_speeds = compute_cap.major >= se::CudaComputeCapability::HOPPER
                       ? size_of_sm90_speed_array
                       : size_of_speed_array;
  const double* speeds = compute_cap.major >= se::CudaComputeCapability::HOPPER
                             ? kIntraNodeSpeedsSm90.data()
                             : kIntraNodeSpeeds.data();

  int speed_index = 0;
  float max_sys_bw =
      GetMaxSysBwFromGpu(compute_cap, kLowLatencyMaxBandwidths.data());

  CHECK_GT(max_sys_bw, 0);

  while ((speed_index < num_speeds - 1) && speeds[speed_index] > max_sys_bw) {
    speed_index++;
  }
  float bw_intra_node = speeds[speed_index];
  int64_t num_devices = cost_analysis->NumOfDevices(instr);

  int64_t min_nchannels =
      std::max(num_devices, GetMinNumberOfChannels(CollectiveAlgo::RING));
  int64_t num_channels =
      std::max(min_nchannels, GetNcclMaxNumChannels(CollectiveAlgo::RING));
  int default_threads =
      (bw_intra_node * num_channels <= kPciBandwidth) ? 256 : kLL128NumThreads;

  int warp_size = gpu_device_info.threads_per_warp();
  int num_threads = GetNumThreads(warp_size, kLL128NumThreads / 4,
                                  kLL128NumThreads, default_threads);

  // Since channels are pipelined together, compute time will only occur as in a
  // single channel.
  absl::Duration compute_time_per_channel =
      ComputeTime(gpu_device_info,
                  cost_analysis->flop_count(instr) / num_channels, num_threads);
  total_time += compute_time_per_channel;

  uint32_t supported_p2p = CheckIfNvlinkSupportsP2P();

  if (supported_p2p == 0) {
    VLOG(8) << "Nvlink doesn't support p2p communication. Model will "
               "continue using default system bandwidth.";
  } else {
    VLOG(8) << "Nvlink supports p2p communication, setting intra node "
               "bandwidth to nvlink bw.";
    bw_intra_node = GetNvlinkBw(compute_cap);
  }

  double bus_bandwidth = bw_intra_node * num_channels;

  // Get per channel LL128 ring bandwidth
  double per_channel_ring_ll128_Bw =
      GetMaxSysBwFromGpu(compute_cap, kPerChannelMaxRingLL128Bandwidths.data());

  bus_bandwidth = std::min(bus_bandwidth * kRingAlgorithmDiscountFactor,
                           num_channels * per_channel_ring_ll128_Bw);
  double actual_bandwidth = bus_bandwidth * cost_analysis->ScalingRatio(instr);

  absl::Duration communication_time = absl::Microseconds(
      cost_analysis->bytes_accessed(instr) / (1e6 * actual_bandwidth));
  total_time += communication_time;
  return total_time;
}

/*static*/ absl::Duration
GpuPerformanceWithCollectiveModel::ComputeCollectiveTime(
    const HloInstruction& instr, const GpuHloCostAnalysis* cost_analysis,
    const se::DeviceDescription& gpu_device_info) {
  if (cost_analysis->NumOfDevices(instr) == 1) {
    VLOG(8) << "Returning only kernel launch overhead for a single partition.";
    return kNcclKernelLaunchOverhead;
  }

  if (HloDataflowAnalysis::IsAsynchronousOperationDone(instr.opcode())) {
    VLOG(8) << "Returning 0 cost for async done op " << instr.name();
    return absl::ZeroDuration();
  }
  switch (instr.opcode()) {
    case HloOpcode::kAllReduce:
    case HloOpcode::kAllReduceStart:
      return ComputeAllreduceTime(instr, cost_analysis, gpu_device_info);
    default: {
      LOG(WARNING)
          << "Runtime estimate for " << instr.name()
          << " not implemented. Returning only the kernel launch time.";
      return kNcclKernelLaunchOverhead;
    }
  }
}

}  // namespace gpu
}  // namespace xla
