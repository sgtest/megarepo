/* Copyright 2019 The TensorFlow Authors. All Rights Reserved.

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
#ifndef TENSORFLOW_COMPILER_XLA_SERVICE_GPU_GEMM_ALGORITHM_PICKER_H_
#define TENSORFLOW_COMPILER_XLA_SERVICE_GPU_GEMM_ALGORITHM_PICKER_H_

#include <functional>
#include <optional>
#include <string>
#include <string_view>
#include <variant>

#include "absl/strings/string_view.h"
#include "tensorflow/compiler/xla/autotune_results.pb.h"
#include "tensorflow/compiler/xla/autotuning.pb.h"
#include "tensorflow/compiler/xla/hlo/ir/hlo_instructions.h"
#include "tensorflow/compiler/xla/hlo/ir/hlo_module.h"
#include "tensorflow/compiler/xla/service/gpu/autotuner_util.h"
#include "tensorflow/compiler/xla/service/hlo_pass_interface.h"
#include "tensorflow/compiler/xla/stream_executor/device_description.h"
#include "tensorflow/compiler/xla/stream_executor/device_memory_allocator.h"
#include "tensorflow/compiler/xla/stream_executor/stream_executor.h"

#if (defined(GOOGLE_CUDA) && GOOGLE_CUDA)
#include "tensorflow/compiler/xla/service/gpu/gpu_conv_runner.h"
#include "tensorflow/compiler/xla/stream_executor/cuda/cuda_blas_lt.h"
#include "tensorflow/compiler/xla/stream_executor/gpu/redzone_allocator.h"
#endif

namespace xla {
namespace gpu {

StatusOr<AutotuneResult> GetBestBlasAlgorithm(
    se::Stream* stream, se::RedzoneAllocator& allocator,
    std::optional<std::string_view> gemm_str,
    const AutotuneConfig& autotune_config, se::DeviceMemoryBase lhs_buffer,
    se::DeviceMemoryBase rhs_buffer, se::DeviceMemoryBase output_buffer,
    absl::Span<const se::blas::AlgorithmType> algorithms,
    const Shape& output_shape, const HloModuleConfig& hlo_module_config,
    double beta,
    const std::function<StatusOr<se::blas::ProfileResult>(
        const se::blas::AlgorithmType&)>& run_benchmark);

// GemmAlgorithmPicker supports two modes: device and deviceless.
// In device mode, we run autotuning on the device and store autotune results.
// In deviceless mode, we pass in some information related to the device and
// use stored autotune results to rewrite Gemm instructions. If the required
// autotune result is not stored, then algorithm is set to kRuntimeAutotuning.
class GemmAlgorithmPicker : public HloModulePass {
 public:
  explicit GemmAlgorithmPicker(AutotuneConfig config) : config_(config) {}

  absl::string_view name() const override { return "gemm-algorithm-picker"; }

  using HloPassInterface::Run;
  StatusOr<bool> Run(
      HloModule* module,
      const absl::flat_hash_set<absl::string_view>& execution_threads) override;

 private:
  AutotuneConfig config_;
};

}  // namespace gpu
}  // namespace xla

#endif  // TENSORFLOW_COMPILER_XLA_SERVICE_GPU_GEMM_ALGORITHM_PICKER_H_
