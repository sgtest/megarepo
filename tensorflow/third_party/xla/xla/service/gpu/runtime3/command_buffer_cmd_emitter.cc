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
#include <optional>
#include <utility>

#include "absl/container/inlined_vector.h"
#include "xla/service/buffer_assignment.h"
#include "xla/service/gpu/gemm_thunk.h"
#include "xla/service/gpu/nccl_all_reduce_thunk.h"
#include "xla/service/gpu/runtime3/command_buffer_cmd.h"
#include "xla/service/gpu/runtime3/copy_thunk.h"
#include "xla/service/gpu/runtime3/kernel_thunk.h"
#include "xla/service/gpu/runtime3/memset_thunk.h"
#include "xla/service/gpu/runtime3/sequential_thunk.h"
#include "xla/service/gpu/runtime3/while_thunk.h"
#include "xla/service/gpu/thunk.h"
#include "xla/statusor.h"
#include "xla/util.h"
#include "tsl/platform/statusor.h"

namespace xla::gpu {

using Command = std::unique_ptr<CommandBufferCmd>;

static StatusOr<Command> ConvertKernelThunk(const KernelThunk& thunk) {
  absl::InlinedVector<CommandBufferCmd::MemoryAccess, 4> args_access;
  args_access.reserve(thunk.written().size());
  for (bool written : thunk.written()) {
    args_access.push_back(written ? CommandBufferCmd::MemoryAccess::kWrite
                                  : CommandBufferCmd::MemoryAccess::kRead);
  }
  return std::make_unique<LaunchCmd>(thunk.kernel_name(), thunk.arguments(),
                                     args_access, thunk.launch_dimensions(),
                                     thunk.shmem_bytes());
}

static StatusOr<Command> ConvertCustomKernelThunk(
    const CustomKernelThunk& thunk) {
  absl::InlinedVector<CommandBufferCmd::MemoryAccess, 4> args_access;
  args_access.reserve(thunk.written().size());
  for (bool written : thunk.written()) {
    args_access.push_back(written ? CommandBufferCmd::MemoryAccess::kWrite
                                  : CommandBufferCmd::MemoryAccess::kRead);
  }
  return std::make_unique<CustomKernelLaunchCmd>(thunk.arguments(), args_access,
                                                 thunk.custom_kernel());
}

static StatusOr<Command> ConvertCopyThunk(
    const DeviceToDeviceCopyThunk& thunk) {
  return std::make_unique<MemcpyDeviceToDeviceCmd>(
      thunk.destination(), thunk.source(), thunk.size_bytes());
}

static StatusOr<Command> ConvertMemzeroThunk(const MemzeroThunk& thunk) {
  return std::make_unique<MemzeroCmd>(thunk.destination());
}

static StatusOr<Command> ConvertMemset32Thunk(
    const Memset32BitValueThunk& thunk) {
  return std::make_unique<Memset32Cmd>(thunk.destination(), thunk.value());
}

static StatusOr<Command> ConvertWhileThunk(const WhileThunk& thunk,
                                           bool force_barriers) {
  TF_ASSIGN_OR_RETURN(
      CommandBufferCmdSequence cond_cmds,
      ConvertToCommands(thunk.condition_thunk_sequence()->thunks(),
                        force_barriers));
  TF_ASSIGN_OR_RETURN(
      CommandBufferCmdSequence body_cmds,
      ConvertToCommands(thunk.body_thunk_sequence()->thunks(), force_barriers));
  return std::make_unique<WhileCmd>(thunk.condition_result_buffer(),
                                    std::move(cond_cmds), std::move(body_cmds));
}

static StatusOr<Command> ConvertGemmThunk(const GemmThunk& thunk) {
  std::optional<const BufferAllocation::Slice> workspace = thunk.workspace();
  if (!workspace.has_value()) {
    return InternalError("Gemm thunk does not contain a workspace buffer");
  }
  return std::make_unique<GemmCmd>(thunk.config(), thunk.lhs_buffer(),
                                   thunk.rhs_buffer(), thunk.output_buffer(),
                                   workspace.value(), thunk.deterministic());
}

static StatusOr<Command> ConvertAllReduceStartThunk(
    const NcclAllReduceStartThunk& thunk) {
  return std::make_unique<AllReduceCmd>(thunk.config(), thunk.reduction_kind(),
                                        thunk.buffers());
}

static StatusOr<Command> ConvertThunk(const Thunk& thunk, bool force_barriers) {
  switch (thunk.kind()) {
    case Thunk::Kind::kKernel:
      return ConvertKernelThunk(static_cast<const KernelThunk&>(thunk));
    case Thunk::Kind::kCustomKernel:
      return ConvertCustomKernelThunk(
          static_cast<const CustomKernelThunk&>(thunk));
    case Thunk::Kind::kCopy:
      return ConvertCopyThunk(
          static_cast<const DeviceToDeviceCopyThunk&>(thunk));
    case Thunk::Kind::kMemzero:
      return ConvertMemzeroThunk(static_cast<const MemzeroThunk&>(thunk));
    case Thunk::Kind::kMemset32BitValue:
      return ConvertMemset32Thunk(
          static_cast<const Memset32BitValueThunk&>(thunk));
    case Thunk::Kind::kWhile:
      return ConvertWhileThunk(static_cast<const WhileThunk&>(thunk),
                               force_barriers);
    case Thunk::Kind::kGemm:
      return ConvertGemmThunk(static_cast<const GemmThunk&>(thunk));
    case Thunk::Kind::kNcclAllReduceStart:
      return ConvertAllReduceStartThunk(
          static_cast<const NcclAllReduceStartThunk&>(thunk));
    case Thunk::Kind::kNcclAllReduceDone:
      return Command{};
    default:
      return InternalError("Unsupported thunk kind: %s",
                           Thunk::KindToString(thunk.kind()));
  }
}

StatusOr<CommandBufferCmdSequence> ConvertToCommands(
    const ThunkSequence& sequence, bool force_barriers) {
  CommandBufferCmdSequence cmd_sequence(force_barriers);
  for (const std::unique_ptr<Thunk>& thunk : sequence) {
    TF_ASSIGN_OR_RETURN(Command cmd, ConvertThunk(*thunk, force_barriers));
    if (cmd) cmd_sequence.Append(std::move(cmd));
  }
  return cmd_sequence;
}

}  // namespace xla::gpu
