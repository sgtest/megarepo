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

#include "xla/service/gpu/runtime3/custom_call_thunk.h"

#include <optional>
#include <string>
#include <utility>
#include <vector>

#include "absl/strings/str_format.h"
#include "xla/executable_run_options.h"
#include "xla/ffi/api/c_api.h"
#include "xla/ffi/call_frame.h"
#include "xla/ffi/ffi_api.h"
#include "xla/hlo/ir/hlo_computation.h"
#include "xla/service/buffer_assignment.h"
#include "xla/service/custom_call_status.h"
#include "xla/service/custom_call_status_internal.h"
#include "xla/service/gpu/thunk.h"
#include "xla/service/service_executable_run_options.h"
#include "xla/status.h"
#include "xla/stream_executor/device_memory.h"
#include "xla/util.h"

#if GOOGLE_CUDA || TENSORFLOW_USE_ROCM
#include "xla/stream_executor/gpu/gpu_stream.h"
#endif

namespace xla {
namespace gpu {

using xla::ffi::CallFrame;
using xla::ffi::CallFrameBuilder;
using xla::ffi::CallOptions;

CustomCallThunk::CustomCallThunk(ThunkInfo thunk_info,
                                 CustomCallTarget call_target,
                                 std::vector<std::optional<Slice>> operands,
                                 std::vector<std::optional<Slice>> results,
                                 const std::string& opaque)
    : Thunk(Thunk::kCustomCall, thunk_info),
      operands_(std::move(operands)),
      results_(std::move(results)),
      call_target_(std::move(call_target)),
      opaque_(opaque) {}

CustomCallThunk::CustomCallThunk(ThunkInfo thunk_info, XLA_FFI_Handler* handler,
                                 std::vector<std::optional<Slice>> operands,
                                 std::vector<std::optional<Slice>> results,
                                 AttributesMap attributes,
                                 const HloComputation* called_computation)
    : Thunk(Thunk::kCustomCall, thunk_info),
      operands_(std::move(operands)),
      results_(std::move(results)),
      handler_(std::move(handler)),
      attributes_(std::move(attributes)),
      called_computation_(called_computation) {}

Status CustomCallThunk::ExecuteCustomCall(const ExecuteParams& params) {
  // gpu_stream is CUstream or e.g. the equivalent type in ROCm.
  std::vector<void*> buffers;
  buffers.reserve(operands_.size() + results_.size());
  for (auto& slices : {operands_, results_}) {
    for (const std::optional<Slice>& slice : slices) {
      if (!slice.has_value()) {
        buffers.push_back(nullptr);
        continue;
      }

      if (!slice->slice.allocation())
        return InternalError("custom call input missing buffer allocation");

      buffers.push_back(
          params.buffer_allocations->GetDeviceAddress(slice->slice).opaque());
    }
  }

#if GOOGLE_CUDA || TENSORFLOW_USE_ROCM
  auto gpu_stream = se::gpu::AsGpuStreamValue(params.stream);
  XlaCustomCallStatus custom_call_status;
  call_target_(gpu_stream, buffers.data(), opaque_.data(), opaque_.size(),
               &custom_call_status);
  auto message = CustomCallStatusGetMessage(&custom_call_status);
  if (message) {
    return InternalError("CustomCall failed: %s", *message);
  } else {
    return OkStatus();
  }
#else   //  GOOGLE_CUDA || TENSORFLOW_USE_ROCM
  return Unavailable(
      "Custom calls on GPU are not supported in this configuration. Please "
      "build with --config=cuda or --config=rocm");
#endif  //   GOOGLE_CUDA || TENSORFLOW_USE_ROCM
}

Status CustomCallThunk::ExecuteFfiHandler(const ExecuteParams& params) {
  // TODO(ezhulenev): This is not the most optimal approach, as we'll be doing
  // a lot of extra allocation on every call. We have to keep attributes
  // separate from arguments, as they do not change after thunk is constructed.
  CallFrameBuilder builder;

  for (auto& slices : {operands_, results_}) {
    for (const std::optional<Slice>& slice : slices) {
      // TODO(ezhulenev): Add a token argument type to XLA:FFI.
      if (!slice.has_value()) {
        return InternalError("FFI handlers do not support tokens (yet)!");
      }

      if (!slice->slice.allocation())
        return InternalError("custom call input missing buffer allocation");

      builder.AddBufferArg(
          params.buffer_allocations->GetDeviceAddress(slice->slice),
          slice->shape.element_type(), slice->shape.dimensions());
    }
  }

  CallFrameBuilder::AttributesBuilder attrs;
  attrs.Append(attributes_);

  builder.AddAttributes(attrs.Build());
  CallFrame call_frame = builder.Build();

  // TODO(ezhulenev): Remove `ServiceExecutableRunOptions` from FFI handler
  // execution context, as apparently it's not easily accessible from Thunk.
  ExecutableRunOptions run_options;
  run_options.set_stream(params.stream);
  ServiceExecutableRunOptions service_run_options(run_options);

  CallOptions options = {&service_run_options, called_computation_};
  return Call(handler_, call_frame, options);
}

Status CustomCallThunk::ExecuteOnStream(const ExecuteParams& params) {
  return handler_ ? ExecuteFfiHandler(params) : ExecuteCustomCall(params);
}

}  // namespace gpu
}  // namespace xla
