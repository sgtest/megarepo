/* Copyright 2019 The OpenXLA Authors.

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

#ifndef XLA_SERVICE_GPU_NCCL_COLLECTIVE_THUNK_H_
#define XLA_SERVICE_GPU_NCCL_COLLECTIVE_THUNK_H_

#include <cstddef>
#include <cstdint>
#include <memory>
#include <optional>
#include <string>
#include <type_traits>
#include <vector>

#include "absl/base/thread_annotations.h"
#include "absl/container/flat_hash_map.h"
#include "absl/functional/function_ref.h"
#include "absl/status/status.h"
#include "absl/status/statusor.h"
#include "absl/synchronization/mutex.h"
#include "mlir/IR/BuiltinOps.h"  // from @llvm-project
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/hlo/ir/hlo_instructions.h"
#include "xla/service/buffer_assignment.h"
#include "xla/service/collective_ops_utils.h"
#include "xla/service/global_device_id.h"
#include "xla/service/gpu/buffer_allocations.h"
#include "xla/service/gpu/gpu_executable_run_options.h"
#include "xla/service/gpu/ir_emission_utils.h"
#include "xla/service/gpu/nccl_api.h"
#include "xla/service/gpu/nccl_clique.h"
#include "xla/service/gpu/nccl_clique_key.h"
#include "xla/service/gpu/thunk.h"
#include "xla/service/llvm_ir/llvm_util.h"
#include "xla/shape.h"
#include "xla/status.h"
#include "xla/stream_executor/device_memory.h"
#include "xla/stream_executor/event.h"
#include "xla/stream_executor/stream.h"
#include "xla/translate/mhlo_to_hlo/attribute_exporter.h"
#include "xla/xla_data.pb.h"

namespace xla {
namespace gpu {

class NcclClique;

struct NcclCollectiveConfig {
  int64_t operand_count;
  std::vector<PrimitiveType> operand_element_type;
  std::vector<ReplicaGroup> replica_groups;
  RendezvousKey::CollectiveOpKind collective_op_kind;
  int64_t op_id;
  CollectiveOpGroupMode group_mode;

  template <typename OpT>
  void SetCollectiveOpKindAndID(OpT op);
  void SetCollectiveOpKindAndID(const HloCollectivePermuteInstruction* instr);
  bool IsDegenerate(int64_t replica_count, int64_t partition_count) const;
};

template <typename OpT>
void NcclCollectiveConfig::SetCollectiveOpKindAndID(OpT op) {
  if (op.getChannelId()) {
    collective_op_kind = RendezvousKey::kCrossModule;
    op_id = static_cast<int64_t>(op.getChannelId()->getHandle());
  } else {
    collective_op_kind = RendezvousKey::kCrossReplica;
    mlir::ModuleOp parent = op->template getParentOfType<mlir::ModuleOp>();
    mlir::IntegerAttr unique_id =
        parent->getAttrOfType<mlir::IntegerAttr>("hlo.unique_id");
    op_id = static_cast<int64_t>(unique_id.getInt());
  }
}

NcclCollectiveConfig GetNcclCollectiveConfig(
    const HloInstruction* hlo, std::optional<bool> use_global_device_ids);

template <typename OpT>
NcclCollectiveConfig GetNcclCollectiveConfigForMlir(
    OpT op, std::optional<bool> use_global_device_ids) {
  NcclCollectiveConfig config;
  config.operand_count = op.getInputs().size();
  config.operand_element_type.reserve(config.operand_count);
  for (int i = 0; i < config.operand_count; i++) {
    const Shape shape = GetShape(op.getInputs()[i]);
    config.operand_element_type.push_back(shape.element_type());
  }
  config.replica_groups = ConvertReplicaGroups(op.getReplicaGroups()).value();
  config.SetCollectiveOpKindAndID(op);
  config.group_mode = GetCollectiveOpGroupMode(op.getChannelId().has_value(),
                                               use_global_device_ids)
                          .value();
  return config;
}

//===----------------------------------------------------------------------===//
// NcclCollectiveThunk
//===----------------------------------------------------------------------===//

// Thunk base class for NCCL collective operations.
class NcclCollectiveThunk : public Thunk {
 public:
  NcclCollectiveThunk(Kind kind, ThunkInfo thunk_info, const NcclApi* nccl_api,
                      bool is_sync);

  struct Buffer {
    int64_t element_count;
    BufferAllocation::Slice source_buffer;
    BufferAllocation::Slice destination_buffer;
    int64_t source_memory_space;
    int64_t destination_memory_space;
    mlir::Value source_value;
    mlir::Value destination_value;
  };

  class AsyncExecutor {
   public:
    // Executes the function on the async communications stream and records a
    // completion event.
    absl::Status Execute(
        absl::FunctionRef<Status(const ExecuteParams&, se::Stream&,
                                 NcclApi::NcclCommHandle)>
            fn,
        const ExecuteParams& params, NcclApi::NcclCommHandle comm,
        AsyncStreamKind stream_kind);
    // Blocks the compute stream until async communication is complete.
    absl::Status Await(const ExecuteParams& params);

   private:
    absl::Mutex mu_;
    // Store done events (by device ordinal) for the done thunk to wait on.
    absl::flat_hash_map<int, se::Event> done_events_ ABSL_GUARDED_BY(mu_);
  };

  // Returns whether NCCL operations appear possible to perform; e.g. if we
  // haven't done a build with the CUDA compiler enabled, we can't compile the
  // NCCL header, and thus this will be false.
  //
  // When this is false, the ExecuteOnStream() call will simply return a status
  // error.
  static bool NcclIsEnabled();
  static absl::Status CheckImplementable();

  // Logging support.
  static std::string GetDeviceString(const NcclExecuteParams& params);

  AsyncExecutor* async_executor() { return async_.get(); }
  absl::Status ExecuteOnStream(const ExecuteParams& params) override;

 protected:
  virtual absl::Status RunNcclCollective(const ExecuteParams& params,
                                         se::Stream& stream,
                                         NcclApi::NcclCommHandle comm) = 0;
  virtual const NcclCollectiveConfig& config() const = 0;
  virtual AsyncStreamKind GetAsyncStreamKind() const {
    return AsyncStreamKind::kCollective;
  }

 private:
  bool IsAsync() const { return async_ != nullptr; }
  int64_t GetStreamId() const {
    return xla::gpu::GetStreamId(IsAsync(), GetAsyncStreamKind());
  }

  const NcclApi* nccl_api_;

#if XLA_ENABLE_XCCL
  bool first_call_to_execute_ = true;
#endif                                    // XLA_ENABLE_XCCL
  std::unique_ptr<AsyncExecutor> async_;  // null if not async.
};

//===----------------------------------------------------------------------===//
// NcclCollectiveDoneThunk
//===----------------------------------------------------------------------===//

class NcclCollectiveDoneThunk : public Thunk {
 public:
  NcclCollectiveDoneThunk(Thunk::Kind kind, ThunkInfo thunk_info,
                          NcclCollectiveThunk::AsyncExecutor& async);

  absl::Status ExecuteOnStream(const ExecuteParams& params) override;

 private:
  NcclCollectiveThunk::AsyncExecutor& async_;
};

absl::Status IsValidOperand(mlir::Value operand, Thunk::Kind reduction_op);

absl::Status IsValidOperand(Shape shape, Thunk::Kind reduction_op);

template <typename NcclThunkType, typename OpT>
absl::Status AddOpDescription(absl::Status status, OpT op,
                              int64_t replica_count, int64_t partition_count) {
  if (status.ok()) {
    return status;
  }
  CollectiveOpGroupMode group_mode = NcclThunkType::GetGroupMode(op);

  int64_t operand_count = 0;
  std::string str;

  if constexpr (std::is_base_of_v<HloInstruction, std::remove_pointer_t<OpT>>) {
    operand_count = op->operand_count();
    str = op->ToString();
  } else {
    operand_count = op->getNumOperands() / 2;
    str = llvm_ir::DumpToString(op.getOperation());
  }

  return Status(
      status.code(),
      absl::StrFormat(
          "%s\n"
          "%s with replica_count: %d, partition_count: %d, group_mode: %s, "
          "operand_count: %d\n%s",
          status.message(), NcclThunkType::GetHloOpName(), replica_count,
          partition_count, CollectiveOpGroupModeToString(group_mode),
          operand_count, str));
}

//===----------------------------------------------------------------------===//

size_t GetNumLocalParticipants(
    const std::vector<GlobalDeviceId>& participants,
    const std::vector<GlobalDeviceId>* local_devices);  // may be null

#if XLA_ENABLE_XCCL
// TODO(hanbinyoon): Consider moving to nccl_utils.h when deprecating Thunks.
absl::StatusOr<NcclComm::Lock> LockNcclComm(
    const NcclExecuteParams& params,
    const std::vector<ReplicaGroup>& replica_groups,
    CollectiveOpGroupMode group_mode, int64_t op_id, int64_t stream_id,
    bool enable_clique_optimization);
#endif  // XLA_ENABLE_XCCL

struct DeviceBufferPair {
  PrimitiveType element_type;
  int64_t element_count;
  se::DeviceMemoryBase source_buffer;
  se::DeviceMemoryBase destination_buffer;
  // TODO(b/320767790): Remove once memory space added to DeviceMemoryBase.
  int64_t source_memory_space;
  int64_t destination_memory_space;
};

absl::StatusOr<std::vector<DeviceBufferPair>> ConvertToDeviceBuffers(
    const Thunk::ExecuteParams& params,
    const std::vector<NcclCollectiveThunk::Buffer>& buffers,
    const std::vector<PrimitiveType>& element_types);

absl::StatusOr<std::vector<DeviceBufferPair>> ConvertToDeviceBuffers(
    const BufferAllocations* buffer_allocations,
    const std::vector<NcclCollectiveThunk::Buffer>& buffers,
    const std::vector<PrimitiveType>& element_types);

// Registers buffers allocated in collective memory (see ncclMemAlloc) with a
// communicator to enable zero-copy collectives.
//
// https://docs.nvidia.com/deeplearning/nccl/user-guide/docs/usage/bufferreg.html
Status MaybeRegisterBuffers(int device_ordinal,
                            const std::vector<DeviceBufferPair>& buffers,
                            NcclApi::NcclCommHandle comm);

}  // namespace gpu
}  // namespace xla

#endif  // XLA_SERVICE_GPU_NCCL_COLLECTIVE_THUNK_H_
