/* Copyright 2024 The OpenXLA Authors.

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

#ifndef XLA_SERVICE_GPU_TRITON_TEST_UTILS_H_
#define XLA_SERVICE_GPU_TRITON_TEST_UTILS_H_

#include <cstdint>
#include <memory>
#include <vector>

#include "absl/status/status.h"
#include "absl/strings/string_view.h"
#include "xla/hlo/ir/hlo_computation.h"
#include "xla/service/gpu/ir_emitter_triton.h"
#include "xla/service/gpu/matmul_utils.h"
#include "xla/service/gpu/tests/gpu_codegen_test.h"
#include "xla/stream_executor/device_description.h"

namespace xla::gpu {

class TritonTest : public GpuCodegenTest {
 public:
  stream_executor::CudaComputeCapability GetCudaComputeCapability() {
    return backend()
        .default_stream_executor()
        ->GetDeviceDescription()
        .cuda_compute_capability();
  }

  const stream_executor::GpuComputeCapability& GpuComputeComp() {
    return device_desc().gpu_compute_capability();
  }

  bool SkipBF16Tests();
  stream_executor::GpuComputeCapability CudaAmpereOrRocm();

 protected:
  const stream_executor::DeviceDescription& device_desc() {
    return backend().default_stream_executor()->GetDeviceDescription();
  }
};

class TritonFilecheckTest : public TritonTest {
 public:
  absl::Status CreateTritonIrAndFileCheck(
      absl::string_view hlo_text, const TritonGemmConfig& config,
      std::vector<int64_t> output_tile_sizes, TritonIrEmitter emitter,
      absl::string_view triton_fusion_name,
      absl::string_view filecheck_pattern);

  absl::Status CreateTritonIrAndFileCheck(
      const HloComputation& computation, const TritonGemmConfig& config,
      std::vector<int64_t> output_tile_sizes, TritonIrEmitter emitter,
      absl::string_view filecheck_pattern);
};

}  //  namespace xla::gpu

#endif  // XLA_SERVICE_GPU_TRITON_TEST_UTILS_H_
