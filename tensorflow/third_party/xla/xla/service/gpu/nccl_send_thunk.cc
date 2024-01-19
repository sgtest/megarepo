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

#include "xla/service/gpu/nccl_send_thunk.h"

#include <cstdint>
#include <optional>
#include <string>
#include <vector>

#include "absl/status/status.h"
#include "absl/strings/string_view.h"
#include "xla/mlir_hlo/lhlo/IR/lhlo_ops.h"
#include "xla/service/collective_ops_utils.h"
#include "xla/service/gpu/nccl_api.h"
#include "xla/service/gpu/nccl_collective_thunk.h"
#include "xla/stream_executor/stream.h"
#include "tsl/platform/errors.h"

namespace xla {
namespace gpu {

using mlir::lmhlo::SendOp;

namespace impl {

NcclP2PConfig GetNcclP2PConfig(SendOp op, int64_t replica_count,
                               int64_t partition_count) {
  return GetNcclP2PConfigForSendRecv(op, replica_count, partition_count);
}

absl::Status CheckImplementable(SendOp op) {
  return IsValidOperand(op.getInputs()[0], Thunk::kNcclSend);
}

}  // namespace impl

NcclSendThunk::NcclSendThunk(ThunkInfo thunk_info, NcclApi* nccl_api, SendOp op,
                             int64_t replica_count, int64_t partition_count,
                             const Buffer& buffer)
    : NcclCollectiveThunk(Thunk::kNcclSend, thunk_info, nccl_api,
                          /*is_sync=*/false),
      config_(GetNcclP2PConfig(op, replica_count, partition_count)),
      buffer_(buffer) {}

/*static*/ NcclP2PConfig NcclSendThunk::GetNcclP2PConfig(
    SendOp op, int64_t replica_count, int64_t partition_count) {
  return impl::GetNcclP2PConfig(op, replica_count, partition_count);
}

/*static*/ absl::Status NcclSendThunk::CheckImplementable(
    mlir::lmhlo::SendOp op, int64_t replica_count, int64_t partition_count) {
  return AddOpDescription<NcclSendThunk>(impl::CheckImplementable(op), op,
                                         replica_count, partition_count);
}

/*static*/ CollectiveOpGroupMode NcclSendThunk::GetGroupMode(SendOp op) {
  return GetGroupModeForSendRecv(op);
}

absl::Status NcclSendThunk::RunNcclCollective(const ExecuteParams& params,
                                              se::Stream& stream,
                                              NcclApi::NcclCommHandle comm) {
  TF_ASSIGN_OR_RETURN(
      std::vector<DeviceBufferPair> device_buffers,
      ConvertToDeviceBuffers(params, {buffer_},
                             config_.config.operand_element_type));
  TF_RET_CHECK(device_buffers.size() == 1) << "Expected one buffer pair.";

  GlobalDeviceId global_device_id = params.nccl_params.global_device_id();

  TF_ASSIGN_OR_RETURN(
      const DeviceAssignment::LogicalID current_logical_id,
      params.nccl_params.device_assn()->LogicalIdForDevice(global_device_id));
  const int64_t current_id =
      config_.config.group_mode == CollectiveOpGroupMode::kCrossReplica
          ? current_logical_id.replica_id
          : current_logical_id.computation_id;
  std::string device_string = GetDeviceString(params.nccl_params);

  const NcclP2PConfig::SourceTargetMapEntry source_target =
      NcclP2PConfig::GetSourceTarget(config_.id_to_source_target, current_id);

  return ::xla::gpu::RunSend(nccl_api(), source_target, device_buffers[0],
                             stream, comm, device_string, current_id);
}

absl::Status RunSend(NcclApi* nccl_api,
                     NcclP2PConfig::SourceTargetMapEntry source_target,
                     DeviceBufferPair& buffer, se::Stream& stream,
                     NcclApi::NcclCommHandle comm,
                     absl::string_view device_string, int64_t current_id) {
  // Determine the target IDs for this instance. The target ID is the ID
  // to which this instance will copy its data.
  int device_ordinal = stream.parent()->device_ordinal();
  VLOG(3) << "Performing collective permute from device ordinal: "
          << device_ordinal << "current_id " << current_id;

  const std::optional<int64_t> target_id = source_target.target;
  se::DeviceMemoryBase src_addr = buffer.source_buffer;

  VLOG(3) << absl::StreamFormat("%s : id = %d, target_id = %d", device_string,
                                current_id, target_id.value_or(-1));

  // Send source buffer to target peer if needed.
  if (target_id) {
    TF_RETURN_IF_ERROR(nccl_api->Send(src_addr, buffer.element_type,
                                      buffer.element_type, *target_id, comm,
                                      &stream));
  }

  return absl::OkStatus();
}

}  // namespace gpu
}  // namespace xla
