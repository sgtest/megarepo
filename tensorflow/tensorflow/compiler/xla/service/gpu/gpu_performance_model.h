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

#ifndef TENSORFLOW_COMPILER_XLA_SERVICE_GPU_GPU_PERFORMANCE_MODEL_H_
#define TENSORFLOW_COMPILER_XLA_SERVICE_GPU_GPU_PERFORMANCE_MODEL_H_

#include <optional>
#include <vector>

#include "absl/time/time.h"
#include "tensorflow/compiler/xla/service/gpu/gpu_hlo_cost_analysis.h"

namespace xla {
namespace gpu {

class GpuPerformanceModel {
 public:
  struct RunTimes {
    absl::Duration time_unfused;
    absl::Duration time_fused;
  };
  static RunTimes EstimateRunTimes(
      const HloInstruction* producer, const GpuHloCostAnalysis* cost_analysis,
      bool use_experimental_block_size = false,
      std::optional<se::CudaComputeCapability> cc = std::nullopt,
      std::vector<HloInstruction*> fused_users = {}, bool multi_output = false);

  // Writes estimated execution time to FusionBackendConfig.reification_cost.
  static void RecordEstimatedRunTime(HloInstruction* instruction,
                                     const GpuHloCostAnalysis* cost_analysis);
};

}  // namespace gpu
}  // namespace xla

#endif  // TENSORFLOW_COMPILER_XLA_SERVICE_GPU_GPU_PERFORMANCE_MODEL_H_
