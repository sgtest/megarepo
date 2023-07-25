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

#ifndef TENSORFLOW_COMPILER_XLA_SERVICE_GPU_HLO_OP_PROFILER_H_
#define TENSORFLOW_COMPILER_XLA_SERVICE_GPU_HLO_OP_PROFILER_H_

#include <cstdint>
#include <memory>

#include "absl/time/time.h"
#include "tensorflow/compiler/xla/hlo/ir/hlo_module.h"
#include "tensorflow/compiler/xla/hlo/ir/hlo_opcode.h"
#include "tensorflow/compiler/xla/service/gpu/gpu_device_info.h"
#include "tensorflow/compiler/xla/service/gpu/hlo_op_profile.pb.h"
#include "tensorflow/compiler/xla/service/hlo_runner.h"
#include "tensorflow/compiler/xla/xla_data.pb.h"

namespace xla {
namespace gpu {

class HloOpProfiler {
  static std::unique_ptr<HloModule> MakeModuleForMeasurements(
      HloOpcode op, PrimitiveType data_type, int64_t n_elements,
      int chain_length);
  StatusOr<absl::Duration> MeasureOpChainDuration(HloOpcode op,
                                                  PrimitiveType data_type,
                                                  int64_t input_size,
                                                  int chain_length);

 public:
  explicit HloOpProfiler(HloRunner& runner)
      : runner_(runner),
        dev_info_(GetGpuDeviceInfo(runner.backend().stream_executors()[0])) {}
  StatusOr<HloInstructionProfile> MeasureClockCyclesPerOp(
      HloOpcode op, bool binary, PrimitiveType data_type, int64_t input_size);

 private:
  // Long chains can be too slow to compile.
  static constexpr int kMaxOpChainLength = 4096;

  HloRunner& runner_;
  const GpuDeviceInfo dev_info_;
};

}  // namespace gpu
}  // namespace xla

#endif  // TENSORFLOW_COMPILER_XLA_SERVICE_GPU_HLO_OP_PROFILER_H_
