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

#include "xla/service/gpu/gemm_thunk.h"

#include <utility>

#include "xla/service/gpu/matmul_utils.h"
#include "xla/service/gpu/thunk.h"
#include "xla/status.h"
#include "xla/stream_executor/device_memory.h"
#include "tsl/platform/logging.h"

namespace xla {
namespace gpu {

GemmThunk::GemmThunk(ThunkInfo thunk_info, GemmConfig config,
                     const BufferAllocation::Slice& lhs_buffer,
                     const BufferAllocation::Slice& rhs_buffer,
                     const BufferAllocation::Slice& output_buffer,
                     bool deterministic)
    : Thunk(Kind::kGemm, thunk_info),
      config_(std::move(config)),
      lhs_buffer_(lhs_buffer),
      rhs_buffer_(rhs_buffer),
      output_buffer_(output_buffer),
      deterministic_(deterministic) {}

Status GemmThunk::ExecuteOnStream(const ExecuteParams& params) {
  VLOG(3) << "Running GEMM thunk";
  const BufferAllocations& allocs = *params.buffer_allocations;
  // TODO(ezhulenev): Pass a correct workspace. For now we ignore it as Thunks
  // are disabled by default, and they do not interact with CUDA graphs.
  se::DeviceMemoryBase workspace(nullptr, 0);
  return RunGemm(config_, allocs.GetDeviceAddress(lhs_buffer_),
                 allocs.GetDeviceAddress(rhs_buffer_),
                 allocs.GetDeviceAddress(output_buffer_), workspace,
                 deterministic_, params.stream);
}

Status GemmThunk::Initialize(se::StreamExecutor* executor,
                             ExecutableSource src) {
  if (!executor->AsBlas()) {
    return absl::InternalError("Failed to initialize BLAS support");
  }
  return OkStatus();
}

}  // namespace gpu
}  // namespace xla
