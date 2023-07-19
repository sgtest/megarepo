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

#include "tensorflow/compiler/xla/service/gpu/fused_mha_thunk.h"

#include <memory>
#include <string>
#include <utility>

#include "absl/strings/str_cat.h"
#include "tensorflow/compiler/xla/hlo/ir/hlo_casting_utils.h"
#include "tensorflow/compiler/xla/service/gpu/gpu_conv_runner.h"
#include "tensorflow/compiler/xla/service/gpu/ir_emission_utils.h"
#include "tensorflow/compiler/xla/stream_executor/stream_executor.h"
#include "tensorflow/compiler/xla/types.h"
#include "tensorflow/compiler/xla/util.h"
#include "tensorflow/tsl/platform/logging.h"

namespace xla {
namespace gpu {

FusedMHAThunk::FusedMHAThunk(
    ThunkInfo thunk_info, GpufMHAConfig config,
    BufferAllocation::Slice lhs_bmm1, BufferAllocation::Slice rhs_bmm1,
    BufferAllocation::Slice rhs_bmm2, BufferAllocation::Slice output,
    BufferAllocation::Slice scratch, BufferAllocation::Slice mask,
    BufferAllocation::Slice bias, BufferAllocation::Slice activation)
    : Thunk(Kind::kFusedMHA, thunk_info),
      lhs_bmm1_buffer_(lhs_bmm1),
      rhs_bmm1_buffer_(rhs_bmm1),
      rhs_bmm2_buffer_(rhs_bmm2),
      output_buffer_(output),
      scratch_buffer_(scratch),
      mask_buffer_(mask),
      bias_buffer_(bias),
      activation_buffer_(activation),
      config_(std::move(config)) {}

FusedMultiHeadedAttentionRunner& FusedMHAThunk::GetOrCreateRunner(
    const stream_executor::Stream* stream) {
  absl::MutexLock lock(&mu_);
  auto it = runner_cache_.find(stream);
  if (it == runner_cache_.end()) {
    it = runner_cache_
             .insert({stream, std::make_unique<FusedMultiHeadedAttentionRunner>(
                                  config_)})
             .first;
  }
  return *it->second;
}

Status FusedMHAThunk::ExecuteOnStream(const ExecuteParams& params) {
  const auto& buffer_allocations = *params.buffer_allocations;
  se::DeviceMemoryBase lhs_bmm1_buffer =
      buffer_allocations.GetDeviceAddress(lhs_bmm1_buffer_);
  se::DeviceMemoryBase rhs_bmm1_buffer =
      buffer_allocations.GetDeviceAddress(rhs_bmm1_buffer_);
  se::DeviceMemoryBase rhs_bmm2_buffer =
      buffer_allocations.GetDeviceAddress(rhs_bmm2_buffer_);
  se::DeviceMemoryBase output_buffer =
      buffer_allocations.GetDeviceAddress(output_buffer_);
  se::DeviceMemoryBase scratch_buffer =
      buffer_allocations.GetDeviceAddress(scratch_buffer_);

  se::DeviceMemoryBase mask_buffer;
  if (mask_buffer_.allocation() != nullptr) {
    mask_buffer = buffer_allocations.GetDeviceAddress(mask_buffer_);
  }
  se::DeviceMemoryBase bias_buffer;
  if (bias_buffer_.allocation() != nullptr) {
    bias_buffer = buffer_allocations.GetDeviceAddress(bias_buffer_);
  }

  se::DeviceMemoryBase activation_buffer;
  if (activation_buffer_.allocation() != nullptr) {
    activation_buffer = buffer_allocations.GetDeviceAddress(activation_buffer_);
  }

  RunFusedMHAOptions opts;
  opts.runner_cache = &GetOrCreateRunner(params.stream);

  TF_RETURN_IF_ERROR(RunGpuFMHA(config_, lhs_bmm1_buffer, rhs_bmm1_buffer,
                                rhs_bmm2_buffer, output_buffer, scratch_buffer,
                                mask_buffer, bias_buffer, activation_buffer,
                                params.stream, opts));
  if (!params.stream->ok()) {
    return InternalError("FusedMHAThunk::ExecuteOnStream failed.");
  }
  return OkStatus();
}
FusedMHABackwardThunk::FusedMHABackwardThunk(
    ThunkInfo thunk_info, GpufMHABackwardConfig config,
    BufferAllocation::Slice bmm1_grad_gemm1_rhs,
    BufferAllocation::Slice bmm1_grad_gemm2_rhs,
    BufferAllocation::Slice bmm2_grad_gemm1_lhs,
    BufferAllocation::Slice bmm2_grad_gemm2_rhs,
    BufferAllocation::Slice d_output, BufferAllocation::Slice scratch,
    BufferAllocation::Slice d_bmm1_lhs, BufferAllocation::Slice d_bmm1_rhs,
    BufferAllocation::Slice d_bmm2_rhs, BufferAllocation::Slice d_s,
    BufferAllocation::Slice mask, BufferAllocation::Slice d_bias)
    : Thunk(Kind::kFusedMHA, thunk_info),
      bmm1_grad_gemm1_rhs_buffer_(bmm1_grad_gemm1_rhs),
      bmm1_grad_gemm2_rhs_buffer_(bmm1_grad_gemm2_rhs),
      bmm2_grad_gemm1_lhs_buffer_(bmm2_grad_gemm1_lhs),
      bmm2_grad_gemm2_rhs_buffer_(bmm2_grad_gemm2_rhs),
      d_output_buffer_(d_output),
      scratch_buffer_(scratch),
      d_bmm1_lhs_buffer_(d_bmm1_lhs),
      d_bmm1_rhs_buffer_(d_bmm1_rhs),
      d_bmm2_rhs_buffer_(d_bmm2_rhs),
      d_s_buffer_(d_s),
      mask_buffer_(mask),
      d_bias_buffer_(d_bias),
      config_(std::move(config)) {}

FusedMultiHeadedAttentionBackwardRunner&
FusedMHABackwardThunk::GetOrCreateRunner(
    const stream_executor::Stream* stream) {
  absl::MutexLock lock(&mu_);
  auto it = runner_cache_.find(stream);
  if (it == runner_cache_.end()) {
    it = runner_cache_
             .insert({stream,
                      std::make_unique<FusedMultiHeadedAttentionBackwardRunner>(
                          config_)})
             .first;
  }
  return *it->second;
}

Status FusedMHABackwardThunk::ExecuteOnStream(const ExecuteParams& params) {
  const auto& buffer_allocations = *params.buffer_allocations;
  se::DeviceMemoryBase bmm1_grad_gemm1_rhs_buffer =
      buffer_allocations.GetDeviceAddress(bmm1_grad_gemm1_rhs_buffer_);

  se::DeviceMemoryBase bmm1_grad_gemm2_rhs_buffer =
      buffer_allocations.GetDeviceAddress(bmm1_grad_gemm2_rhs_buffer_);

  se::DeviceMemoryBase bmm2_grad_gemm1_lhs_buffer =
      buffer_allocations.GetDeviceAddress(bmm2_grad_gemm1_lhs_buffer_);

  se::DeviceMemoryBase bmm2_grad_gemm2_rhs_buffer =
      buffer_allocations.GetDeviceAddress(bmm2_grad_gemm2_rhs_buffer_);

  se::DeviceMemoryBase d_output_buffer =
      buffer_allocations.GetDeviceAddress(d_output_buffer_);

  se::DeviceMemoryBase scratch_buffer =
      buffer_allocations.GetDeviceAddress(scratch_buffer_);

  se::DeviceMemoryBase d_bmm1_lhs_buffer =
      buffer_allocations.GetDeviceAddress(d_bmm1_lhs_buffer_);

  se::DeviceMemoryBase d_bmm1_rhs_buffer =
      buffer_allocations.GetDeviceAddress(d_bmm1_rhs_buffer_);

  se::DeviceMemoryBase d_bmm2_rhs_buffer =
      buffer_allocations.GetDeviceAddress(d_bmm2_rhs_buffer_);

  se::DeviceMemoryBase d_S_buffer =
      buffer_allocations.GetDeviceAddress(d_s_buffer_);

  se::DeviceMemoryBase mask_buffer;
  if (mask_buffer_.allocation() != nullptr) {
    mask_buffer = buffer_allocations.GetDeviceAddress(mask_buffer_);
  }

  se::DeviceMemoryBase d_bias_buffer;
  if (d_bias_buffer_.allocation() != nullptr) {
    d_bias_buffer = buffer_allocations.GetDeviceAddress(d_bias_buffer_);
  }
  RunFusedMHABackwardOptions opts;

  opts.runner_cache = &GetOrCreateRunner(params.stream);

  TF_RETURN_IF_ERROR(RunGpuFMHABackward(
      config_, bmm1_grad_gemm1_rhs_buffer, bmm1_grad_gemm2_rhs_buffer,
      bmm2_grad_gemm1_lhs_buffer, bmm2_grad_gemm2_rhs_buffer, d_output_buffer,
      scratch_buffer, d_bmm1_lhs_buffer, d_bmm1_rhs_buffer, d_bmm2_rhs_buffer,
      d_S_buffer, mask_buffer, d_bias_buffer, params.stream, opts));
  if (!params.stream->ok()) {
    return InternalError("FusedMHABackwardThunk::ExecuteOnStream failed.");
  }
  return OkStatus();
}

}  // namespace gpu
}  // namespace xla
