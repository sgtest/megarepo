/* Copyright 2023 The OpenXLA Authors.

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

#include "xla/stream_executor/command_buffer.h"

#include <memory>
#include <utility>

#include "absl/functional/any_invocable.h"
#include "absl/status/status.h"
#include "absl/status/statusor.h"
#include "xla/stream_executor/device_memory.h"
#include "xla/stream_executor/kernel.h"
#include "xla/stream_executor/stream_executor.h"
#include "xla/stream_executor/stream_executor_internal.h"
#include "tsl/platform/errors.h"
#include "tsl/platform/statusor.h"

namespace stream_executor {

absl::StatusOr<std::unique_ptr<CommandBuffer>> CommandBuffer::Create(
    StreamExecutor* executor, Mode mode) {
  return executor->implementation()->CreateCommandBuffer(mode);
}

absl::StatusOr<std::unique_ptr<CommandBuffer>> CommandBuffer::Trace(
    StreamExecutor* executor,
    absl::AnyInvocable<absl::Status(Stream*)> function, Mode mode) {
  Stream stream(executor);
  if (stream.Init(); !stream.ok())
    return absl::InternalError(
        "Failed to initialize stream for command buffer tracing");

  return Trace(executor, &stream, std::move(function), mode);
}

absl::StatusOr<std::unique_ptr<CommandBuffer>> CommandBuffer::Trace(
    StreamExecutor* executor, Stream* stream,
    absl::AnyInvocable<absl::Status(Stream*)> function, Mode mode) {
  if (stream == nullptr)
    return absl::InvalidArgumentError(
        "Can't trace command buffer on a null stream");

  // Prepare an empty command buffer instance.
  TF_ASSIGN_OR_RETURN(std::unique_ptr<CommandBuffer> command_buffer,
                      CommandBuffer::Create(executor, mode));

  // Trace and finalize the command buffer.
  TF_RETURN_IF_ERROR(
      command_buffer->Trace(stream, [&]() { return function(stream); }));
  TF_RETURN_IF_ERROR(command_buffer->Finalize());

  return command_buffer;
}

bool CommandBuffer::SupportsConditionalCommands(const Platform* platform) {
  // TODO(ezhulenev): We should extend a Platform with a way to query
  // implemented StreamExecutor features, for now we know that only CUDA
  // platform supports conditional commands in command buffers.
#if defined(STREAM_EXECUTOR_CUDA_ENABLE_GRAPH_CONDITIONAL)
  return platform->Name() == "CUDA";
#endif
  return false;
}

}  // namespace stream_executor
