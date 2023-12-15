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

#ifndef XLA_SERVICE_GPU_RUNTIME3_COMMAND_BUFFER_CMD_H_
#define XLA_SERVICE_GPU_RUNTIME3_COMMAND_BUFFER_CMD_H_

#include <cstdint>
#include <memory>
#include <string>
#include <vector>

#include "absl/container/flat_hash_map.h"
#include "absl/container/flat_hash_set.h"
#include "absl/container/inlined_vector.h"
#include "absl/types/span.h"
#include "xla/service/buffer_assignment.h"
#include "xla/service/gpu/buffer_allocations.h"
#include "xla/service/gpu/kernels/custom_kernel.h"
#include "xla/service/gpu/launch_dimensions.h"
#include "xla/service/gpu/matmul_utils.h"
#include "xla/service/gpu/runtime3/command_buffer_allocations.h"
#include "xla/service/gpu/thunk.h"
#include "xla/status.h"
#include "xla/stream_executor/command_buffer.h"
#include "xla/stream_executor/device_memory.h"
#include "xla/stream_executor/kernel.h"
#include "xla/stream_executor/stream_executor.h"

namespace xla::gpu {

using OwnedKernel = std::unique_ptr<se::Kernel>;

//===----------------------------------------------------------------------===//
// CommandBufferCmd
//===----------------------------------------------------------------------===//

// CommandBufferCmd is an abstract command that creates or updates command
// buffer by recording commands into it.
class CommandBufferCmd {
 public:
  enum class MemoryAccess { kRead, kWrite };

  // BufferUsage tracks memory access type for a buffer slice, so that we can
  // correctly insert command buffer barriers to avoid read/write conflicts.
  struct BufferUsage {
    BufferUsage(BufferAllocation::Slice slice, MemoryAccess access)
        : slice(slice), access(access) {}

    template <typename H>
    friend H AbslHashValue(H h, const BufferUsage& buffer) {
      return H::combine(std::move(h), buffer.slice, buffer.access);
    }

    bool operator==(const BufferUsage& other) const {
      return slice == other.slice && access == other.access;
    }

    BufferAllocation::Slice slice;
    MemoryAccess access;
  };

  using ExecutableSource = Thunk::ExecutableSource;
  using BufferUsageVector = absl::InlinedVector<BufferUsage, 4>;

  // Run time parameters required for recording commands into the command
  // buffer. For example when we emit command buffer cmd sequence from an HLO
  // module, we only know the buffer slices required for HLO operations, but the
  // concrete device pointers become available only at run time.
  //
  // For allocations that performed through command buffer Allocate command, the
  // target addresses are tracked by command buffer runtime. To record command
  // that consumes buffers allocated inside command buffer, user should specify
  // the target address as se::DeviceMemoryBase{nullptr, size}.
  struct RecordParams {
    se::StreamExecutor* executor;
    const BufferAllocations* buffer_allocations;
  };

  // Prepares a command for recording on a given executor. We split it into a
  // separate function to allow expensive initialization (e.g. device kernel
  // loading) to happen before a command buffer thunk execution.
  virtual Status Initialize(se::StreamExecutor* executor,
                            ExecutableSource source) {
    return OkStatus();
  }

  // Records command into the command buffer.
  virtual Status Record(const RecordParams& params,
                        se::CommandBuffer* command_buffer) = 0;

  // Returns all buffers used by the cmd. These will be used to track cmd
  // updates, thus they need to be consistent across calls to the function.
  virtual BufferUsageVector buffers() = 0;

  virtual ~CommandBufferCmd() = default;
};

//===----------------------------------------------------------------------===//
// CommandBufferCmdSequence
//===----------------------------------------------------------------------===//

// A sequence of command buffer commands that create or update a command buffer.
// You can think of CommandBufferCmdSequence as a mini interpreter whose sole
// purpose is to manipulate command buffers at run time.
class CommandBufferCmdSequence {
 public:
  CommandBufferCmdSequence() = default;

  enum class RecordMode {
    // In exclusive mode no one else is recording commands into the command
    // buffer argument, and cmd sequence is responsible for updating command
    // buffer state: finalizing after all commands recorded, and
    // switching to update state before recording updates.
    kExclusive,

    // In conditional mode multiple cmd sequences can be recorded into the
    // command buffer argument, and with command buffer state managed externally
    // cmd sequence should not finalize or update it. This mode is used when
    // command buffer cmd sequence is recorded into conditional command buffers
    // owned by the parent command buffer.
    kConditional
  };

  void Append(std::unique_ptr<CommandBufferCmd> cmd);

  template <typename T, typename... Args>
  void Emplace(Args... args) {
    Append(std::make_unique<T>(std::forward<Args>(args)...));
  }

  // Initialized all commands added to a sequence.
  Status Initialize(se::StreamExecutor* executor,
                    CommandBufferCmd::ExecutableSource source);

  // Records all commands added to a sequence into the given command buffer.
  Status Record(const CommandBufferCmd::RecordParams& params,
                se::CommandBuffer* command_buffer,
                RecordMode mode = RecordMode::kExclusive);

  // Returns buffers referenced by commands in this sequence.
  const absl::flat_hash_set<CommandBufferCmd::BufferUsage>& buffers() const;

  // Returns buffer allocations indices referenced by commands in this sequence.
  const absl::flat_hash_set<BufferAllocation::Index>& allocs_indices() const;

 private:
  std::vector<std::unique_ptr<CommandBufferCmd>> commands_;

  // Buffers referenced by commands in this sequence.
  absl::flat_hash_set<CommandBufferCmd::BufferUsage> buffers_;

  // Buffer allocations indices referenced by commands in this sequence.
  absl::flat_hash_set<BufferAllocation::Index> allocs_indices_;
};

//===----------------------------------------------------------------------===//
// LaunchCmd
//===----------------------------------------------------------------------===//

class LaunchCmd : public CommandBufferCmd {
 public:
  LaunchCmd(std::string kernel_name,
            absl::Span<const BufferAllocation::Slice> args,
            absl::Span<const MemoryAccess> args_access, LaunchDimensions dims,
            int64_t shmem_bytes);

  Status Initialize(se::StreamExecutor* executor,
                    ExecutableSource source) override;

  Status Record(const RecordParams& params,
                se::CommandBuffer* command_buffer) override;

  BufferUsageVector buffers() override;

 private:
  std::string kernel_name_;
  std::vector<BufferAllocation::Slice> args_;
  std::vector<MemoryAccess> args_access_;
  LaunchDimensions dims_;
  int64_t shmem_bytes_;

  absl::flat_hash_map<se::StreamExecutor*, OwnedKernel> kernels_;
};

//===----------------------------------------------------------------------===//
// CustomKenelLaunchCmd
//===----------------------------------------------------------------------===//

class CustomKernelLaunchCmd : public CommandBufferCmd {
 public:
  CustomKernelLaunchCmd(absl::Span<const BufferAllocation::Slice> args,
                        absl::Span<const MemoryAccess> args_access,
                        CustomKernel custom_kernel);

  Status Initialize(se::StreamExecutor* executor,
                    ExecutableSource source) override;

  Status Record(const RecordParams& params,
                se::CommandBuffer* command_buffer) override;

  BufferUsageVector buffers() override;

 private:
  std::vector<BufferAllocation::Slice> args_;
  std::vector<MemoryAccess> args_access_;
  CustomKernel custom_kernel_;

  absl::flat_hash_map<se::StreamExecutor*, OwnedKernel> kernels_;
};
//===----------------------------------------------------------------------===//
// MemcpyDeviceToDeviceCmd
//===----------------------------------------------------------------------===//

class MemcpyDeviceToDeviceCmd : public CommandBufferCmd {
 public:
  MemcpyDeviceToDeviceCmd(BufferAllocation::Slice dst,
                          BufferAllocation::Slice src, int64_t num_bytes);

  Status Record(const RecordParams& params,
                se::CommandBuffer* command_buffer) override;

  BufferUsageVector buffers() override;

 private:
  BufferAllocation::Slice dst_;
  BufferAllocation::Slice src_;
  int64_t num_bytes_;
};

//===----------------------------------------------------------------------===//
// IfCmd
//===----------------------------------------------------------------------===//

class IfCmd : public CommandBufferCmd {
 public:
  IfCmd(BufferAllocation::Slice pred, CommandBufferCmdSequence then_commands);

  Status Initialize(se::StreamExecutor* executor,
                    ExecutableSource source) override;

  Status Record(const RecordParams& params,
                se::CommandBuffer* command_buffer) override;

  BufferUsageVector buffers() override;

 private:
  BufferAllocation::Slice pred_;
  CommandBufferCmdSequence then_commands_;
};

//===----------------------------------------------------------------------===//
// IfElseCmd
//===----------------------------------------------------------------------===//

class IfElseCmd : public CommandBufferCmd {
 public:
  IfElseCmd(BufferAllocation::Slice pred,
            CommandBufferCmdSequence then_commands,
            CommandBufferCmdSequence else_commands);

  Status Initialize(se::StreamExecutor* executor,
                    ExecutableSource source) override;

  Status Record(const RecordParams& params,
                se::CommandBuffer* command_buffer) override;

  BufferUsageVector buffers() override;

 private:
  BufferAllocation::Slice pred_;
  CommandBufferCmdSequence then_commands_;
  CommandBufferCmdSequence else_commands_;
};

//===----------------------------------------------------------------------===//
// CaseCmd
//===----------------------------------------------------------------------===//

class CaseCmd : public CommandBufferCmd {
 public:
  CaseCmd(BufferAllocation::Slice index,
          std::vector<CommandBufferCmdSequence> branches_commands);

  Status Initialize(se::StreamExecutor* executor,
                    ExecutableSource source) override;

  Status Record(const RecordParams& params,
                se::CommandBuffer* command_buffer) override;

  BufferUsageVector buffers() override;

 private:
  BufferAllocation::Slice index_;
  std::vector<CommandBufferCmdSequence> branches_commands_;
};

//===----------------------------------------------------------------------===//
// ForCmd
//===----------------------------------------------------------------------===//

class ForCmd : public CommandBufferCmd {
 public:
  ForCmd(int32_t num_iterations, BufferAllocation::Slice loop_counter,
         CommandBufferCmdSequence body_commands);

  Status Initialize(se::StreamExecutor* executor,
                    ExecutableSource source) override;

  Status Record(const RecordParams& params,
                se::CommandBuffer* command_buffer) override;

  BufferUsageVector buffers() override;

 private:
  int32_t num_iterations_;
  BufferAllocation::Slice loop_counter_;
  CommandBufferCmdSequence body_commands_;
};

//===----------------------------------------------------------------------===//
// WhileCmd
//===----------------------------------------------------------------------===//

class WhileCmd : public CommandBufferCmd {
 public:
  WhileCmd(BufferAllocation::Slice pred, CommandBufferCmdSequence cond_commands,
           CommandBufferCmdSequence body_commands);

  Status Initialize(se::StreamExecutor* executor,
                    ExecutableSource source) override;

  Status Record(const RecordParams& params,
                se::CommandBuffer* command_buffer) override;

  BufferUsageVector buffers() override;

 private:
  BufferAllocation::Slice pred_;
  CommandBufferCmdSequence cond_commands_;
  CommandBufferCmdSequence body_commands_;
};

//===----------------------------------------------------------------------===//
// AllocateCmd
//===----------------------------------------------------------------------===//

class AllocateCmd : public CommandBufferCmd {
 public:
  AllocateCmd(BufferAllocation allocation);

  // After calling this function, the allocated memory is tracked in
  // CommandBuffer object.
  Status Record(const RecordParams& params,
                se::CommandBuffer* command_buffer) override;

  BufferUsageVector buffers() override;

 private:
  BufferAllocation allocation_;
};

//===----------------------------------------------------------------------===//
// FreeCmd
//===----------------------------------------------------------------------===//

class FreeCmd : public CommandBufferCmd {
 public:
  explicit FreeCmd(BufferAllocation allocation);

  // After calling this function, the allocated memory address for dst
  // BufferAllocation is freed, no update is required.
  Status Record(const RecordParams& params,
                se::CommandBuffer* command_buffer) override;

  BufferUsageVector buffers() override;

 private:
  BufferAllocation allocation_;
};

//===----------------------------------------------------------------------===//
// GemmCmd
//===----------------------------------------------------------------------===//

class GemmCmd : public CommandBufferCmd {
 public:
  GemmCmd(GemmConfig config, const BufferAllocation::Slice& lhs_buffer,
          const BufferAllocation::Slice& rhs_buffer,
          const BufferAllocation::Slice& output_buffer,
          const BufferAllocation::Slice& workspace, bool deterministic);

  Status Initialize(se::StreamExecutor* executor,
                    ExecutableSource source) override;

  Status Record(const RecordParams& params,
                se::CommandBuffer* command_buffer) override;

  BufferUsageVector buffers() override;

 private:
  const GemmConfig config_;
  const BufferAllocation::Slice lhs_buffer_;
  const BufferAllocation::Slice rhs_buffer_;
  const BufferAllocation::Slice output_buffer_;
  const BufferAllocation::Slice workspace_;
  // Whether to run deterministically.
  const bool deterministic_;
};

}  // namespace xla::gpu

#endif  // XLA_SERVICE_GPU_RUNTIME3_COMMAND_BUFFER_CMD_H_
