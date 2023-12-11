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

#include "xla/service/gpu/runtime3/command_buffer_cmd_emitter.h"

#include <memory>
#include <utility>

#include "xla/service/gpu/copy_thunk.h"
#include "xla/service/gpu/kernel_thunk.h"
#include "xla/service/gpu/runtime3/command_buffer_cmd.h"
#include "xla/service/gpu/runtime3/sequential_thunk.h"
#include "xla/service/gpu/runtime3/while_thunk.h"
#include "xla/service/gpu/thunk.h"
#include "xla/statusor.h"
#include "xla/util.h"
#include "tsl/platform/statusor.h"

namespace xla::gpu {

using Command = std::unique_ptr<CommandBufferCmd>;

static StatusOr<Command> ConvertKernelThunk(const KernelThunk& thunk) {
  return std::make_unique<LaunchCmd>(thunk.kernel_name(), thunk.arguments(),
                                     thunk.launch_dimensions(),
                                     thunk.shmem_bytes());
}

static StatusOr<Command> ConvertCopyThunk(
    const DeviceToDeviceCopyThunk& thunk) {
  return std::make_unique<MemcpyDeviceToDeviceCmd>(
      thunk.destination(), thunk.source(), thunk.size_bytes());
}

static StatusOr<Command> ConvertWhileThunk(const WhileThunk& thunk) {
  TF_ASSIGN_OR_RETURN(
      CommandBufferCmdSequence cond_cmds,
      ConvertToCommands(thunk.condition_thunk_sequence()->thunks()));
  TF_ASSIGN_OR_RETURN(CommandBufferCmdSequence body_cmds,
                      ConvertToCommands(thunk.body_thunk_sequence()->thunks()));
  return std::make_unique<WhileCmd>(thunk.condition_result_buffer(),
                                    std::move(cond_cmds), std::move(body_cmds));
}

static StatusOr<Command> ConvertThunk(const Thunk& thunk) {
  switch (thunk.kind()) {
    case Thunk::Kind::kKernel:
      return ConvertKernelThunk(static_cast<const KernelThunk&>(thunk));
    case Thunk::Kind::kCopy:
      return ConvertCopyThunk(
          static_cast<const DeviceToDeviceCopyThunk&>(thunk));
    case Thunk::Kind::kWhile:
      return ConvertWhileThunk(static_cast<const WhileThunk&>(thunk));
    default:
      return InternalError("Unsupported thunk kind: %s",
                           Thunk::KindToString(thunk.kind()));
  }
}

StatusOr<CommandBufferCmdSequence> ConvertToCommands(
    const ThunkSequence& sequence) {
  CommandBufferCmdSequence cmd_sequence;
  for (const std::unique_ptr<Thunk>& thunk : sequence) {
    TF_ASSIGN_OR_RETURN(Command cmd, ConvertThunk(*thunk));
    cmd_sequence.Append(std::move(cmd));
  }
  return cmd_sequence;
}

}  // namespace xla::gpu
