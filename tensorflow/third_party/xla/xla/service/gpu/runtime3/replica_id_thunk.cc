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

#include "xla/service/gpu/runtime3/replica_id_thunk.h"

#include "absl/status/status.h"
#include "xla/service/global_device_id.h"

namespace xla {
namespace gpu {

absl::Status ReplicaOrPartitionIdThunk::ExecuteOnStream(
    const ExecuteParams& params) {
  auto dest_addr = params.buffer_allocations->GetDeviceAddress(dest_);

  GlobalDeviceId global_device_id = params.nccl_params.global_device_id();
  TF_ASSIGN_OR_RETURN(
      const DeviceAssignment::LogicalID logical_id,
      params.nccl_params.device_assn()->LogicalIdForDevice(global_device_id));
  int id = kind() == Kind::kReplicaId ? logical_id.replica_id
                                      : logical_id.computation_id;
  params.stream->ThenMemset32(&dest_addr, id, /*size=*/4);
  return absl::OkStatus();
}

}  // namespace gpu
}  // namespace xla
