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
#ifndef TENSORFLOW_COMPILER_XLA_SERVICE_GPU_AUTOTUNER_UTIL_H_
#define TENSORFLOW_COMPILER_XLA_SERVICE_GPU_AUTOTUNER_UTIL_H_

#include <algorithm>
#include <functional>
#include <memory>
#include <string>
#include <tuple>
#include <utility>
#include <variant>

#include "tensorflow/compiler/xla/autotune_results.pb.h"
#include "tensorflow/compiler/xla/autotuning.pb.h"
#include "tensorflow/compiler/xla/hlo/ir/hlo_computation.h"
#include "tensorflow/compiler/xla/hlo/ir/hlo_instruction.h"
#include "tensorflow/compiler/xla/hlo/ir/hlo_instructions.h"
#include "tensorflow/compiler/xla/hlo/ir/hlo_module.h"
#include "tensorflow/compiler/xla/stream_executor/gpu/redzone_allocator.h"
#include "tensorflow/compiler/xla/stream_executor/stream_executor_pimpl.h"
#include "tensorflow/compiler/xla/types.h"
#include "tensorflow/compiler/xla/xla.pb.h"

namespace xla {
namespace gpu {

struct DeviceConfig {
  se::StreamExecutor* stream_exec;  // never null

  // If the `allocator` parameter is not null, we will use it to allocate temp
  // memory while timing the various convolution algorithms.  If it's null,
  // we'll use the default allocator on the StreamExecutor.
  se::DeviceMemoryAllocator* allocator = nullptr;  // may be null
};

struct DevicelessConfig {
  // The human-readable description of the device.  It can be found by using
  // stream_exec->GetDeviceDescription().model_str() when the stream executor
  // is available.
  std::string model_str;

  // A field to determine the architecture of the device. We only pick an
  // algorithm for non-Ampere architectures.
  se::CudaComputeCapability cuda_compute_capability{0, 0};
};

class AutotuneCacheKey {
 public:
  AutotuneCacheKey(absl::string_view model_str,
                   const HloInstruction& instruction);

  explicit AutotuneCacheKey(absl::string_view model_str,
                            absl::string_view hlo_canonical)
      : model_str_(model_str), hlo_canonical_(hlo_canonical) {}

  absl::string_view GetModelStr() const { return model_str_; }

  absl::string_view GetHlo() const { return hlo_canonical_; }

  template <typename H>
  friend H AbslHashValue(H h, const AutotuneCacheKey& w) {
    return H::combine(std::move(h), w.model_str_, w.hlo_canonical_);
  }

  bool operator==(const AutotuneCacheKey& w) const {
    return model_str_ == w.model_str_ && hlo_canonical_ == w.hlo_canonical_;
  }

  std::string ToString() const {
    return absl::StrFormat("<key model='%s', hlo='%s'>", model_str_,
                           hlo_canonical_);
  }

 private:
  std::string model_str_;
  std::string hlo_canonical_;
};

class AutotuneConfig {
 public:
  bool should_init_buffers() const { return autotune_level_ >= 2; }
  bool should_reinit_output_buffer() const { return autotune_level_ >= 3; }
  bool should_check_correctness() const { return autotune_level_ >= 4; }
  bool should_crash_on_check_failure() const {
    return should_crash_on_check_failure_;
  }

  AutotuneConfig(const std::variant<DeviceConfig, DevicelessConfig>& config,
                 const DebugOptions& debug_options)
      : config_(config),
        autotune_level_(debug_options.xla_gpu_autotune_level()),
        should_crash_on_check_failure_(
            debug_options.xla_gpu_crash_on_verification_failures()),
        exhaustive_tiling_search_(
            debug_options.xla_gpu_exhaustive_tiling_search()) {}

  absl::string_view GetModelStr() const {
    if (auto deviceless_config = std::get_if<DevicelessConfig>(&config_)) {
      return deviceless_config->model_str;
    }

    const auto& device_config = std::get<DeviceConfig>(config_);
    return device_config.stream_exec->GetDeviceDescription().model_str();
  }

  se::StreamExecutor* GetExecutor() const {
    CHECK(std::holds_alternative<DeviceConfig>(config_));
    return std::get<DeviceConfig>(config_).stream_exec;
  }

  se::DeviceMemoryAllocator* GetAllocator() const {
    CHECK(std::holds_alternative<DeviceConfig>(config_));
    auto& cf = std::get<DeviceConfig>(config_);
    return cf.allocator ? cf.allocator : GetExecutor()->GetAllocator();
  }

  StatusOr<se::Stream*> GetStream() const {
    CHECK(std::holds_alternative<DeviceConfig>(config_));
    return GetAllocator()->GetStream(GetExecutor()->device_ordinal());
  }

  se::CudaComputeCapability GetCudaComputeCapability() const {
    if (auto c = std::get_if<DeviceConfig>(&config_)) {
      return c->stream_exec->GetDeviceDescription().cuda_compute_capability();
    }
    return std::get<DevicelessConfig>(config_).cuda_compute_capability;
  }

  bool IsDeviceless() const {
    return std::holds_alternative<DevicelessConfig>(config_);
  }

  bool ExhaustiveTilingSearch() const { return exhaustive_tiling_search_; }

 private:
  std::variant<DeviceConfig, DevicelessConfig> config_;
  int32_t autotune_level_;
  bool should_crash_on_check_failure_;
  bool exhaustive_tiling_search_;
};

using AutotuneNoCacheFn = std::function<StatusOr<AutotuneResult>()>;

struct AutotunerUtil {
  // Create a buffer for a given operation using redzone checker, initialize
  // based on a given rng state.
  static StatusOr<se::DeviceMemoryBase> CreateBuffer(
      se::RedzoneAllocator& allocator, const Shape& shape,
      const AutotuneConfig& config, int64_t& rng_state);

  static StatusOr<AutotuneResult> Autotune(
      const HloInstruction* instr, const AutotuneConfig& config,
      const AutotuneNoCacheFn& autotune_fn);

  // Creates a RedzoneAllocator from a given config. If `force_stream` is
  // provided, than it is used for checking redzones.
  static StatusOr<se::RedzoneAllocator> CreateRedzoneAllocator(
      const AutotuneConfig& config, const DebugOptions& opts,
      se::Stream* force_stream = nullptr);

  // Functions to save/load XLA's autotuning results.
  //
  // This is used for ahead-of-time autotuning.  Specifically:
  //
  // When XLA calls cublas (for matmuls, aka "gemm" or "dot") or cudnn (for
  // convolutions), it usually has to choose an "algorithm" for the particular
  // dot/conv.  XLA queries cublas/cudnn for a list of candidate algorithms.
  // Then it runs all of them and picks the fastest one.  This is what we call
  // "autotuning". It happens in GemmAlgorithmPicker and GpuConvAlgorithmPicker.
  //
  // Autotuning is necessary to get good performance for dot/conv.  But it also
  // has some disadvantages.
  //
  //  - Because it relies on timing data, it is fundamentally nondeterministic.
  //    But even if two algorithms have similar runtimes, our choice of
  //    algorithm may be visible to the user: Different algorithms can have
  //    different numerics, and sometimes they can even have different bugs!
  //
  //  - Trying all the candidate algorithms can be slow, especially if when some
  //    of the candidates are "very bad" and run especially slowly compared to
  //    the optimal candidate.  This slows down compilation.
  //
  // To address the disadvantages above, we allow users to save/restore the
  // autotuning choices that XLA has made, using the functions below.
  //
  // Loading autotuning results does not erase existing autotuning choices, but
  // in the event of a disagreement between the existing data and the new data,
  // the new algorithm is chosen.
  //
  // Note that even if you call LoadAutotuneResults(), if XLA encounters a
  // dot/conv that is *not* covered by the loaded data, it will go ahead and
  // autotune it like normal.  In other words, the behavior of XLA should be
  // identical with or without ahead-of-time autotuning, modulo nondeterminism.
  //
  // This is important if you want to be able to use the same autotuning file
  // with different versions of XLA, because as XLA changes, exactly which
  // dots/convs it wants to run can also change.  For example, XLA might change
  // the conv padding heuristics it uses, and we don't want that to mean that
  // all users of ahead-of-time autotuning are broken.
  static StatusOr<std::string> SerializeAutotuneResults(
      bool as_textproto = false);

  static Status SerializeAutotuneResults(AutotuneResults* results);
  static Status LoadAutotuneResults(absl::string_view data,
                                    bool as_textproto = false);

  static Status LoadAutotuneResults(const AutotuneResults& results);

  // Serializes autotune results into a file.
  //
  // If `file_path` ends with ".txt" or ".textproto", then the textproto format
  // is used, otherwise the binary protobuf format.
  static Status SerializeAutotuneResultsToFile(absl::string_view file_path);

  // Loads autotune results from a file.
  //
  // If `file_path` ends with ".txt" or ".textproto", then the file is
  // considered to be in the textproto format, otherwise the binary protobuf
  // format.
  static Status LoadAutotuneResultsFromFile(absl::string_view file_path);

  static void ClearAutotuneResults();

  // Extracts an HLO instruction into a new HLO module replacing its operands
  // with parameter instructions.
  static std::unique_ptr<HloModule> ExtractInstructionIntoNewModule(
      const HloInstruction& hlo);

  // Extracts an HLO computation into a new HLO module, using its clone as the
  // root computation.
  static std::unique_ptr<HloModule> ExtractComputationIntoNewModule(
      const HloComputation& computation);
};

}  // namespace gpu
}  // namespace xla

#endif  // TENSORFLOW_COMPILER_XLA_SERVICE_GPU_AUTOTUNER_UTIL_H_
