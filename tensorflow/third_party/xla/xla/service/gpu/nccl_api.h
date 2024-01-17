/* Copyright 2024 The TensorFlow Authors. All Rights Reserved.

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

#ifndef XLA_SERVICE_GPU_NCCL_API_H_
#define XLA_SERVICE_GPU_NCCL_API_H_

#include <cstddef>
#include <cstdint>

#include "absl/status/status.h"
#include "absl/status/statusor.h"
#include "xla/service/gpu/nccl_clique_key.h"
#include "xla/stream_executor/device_memory.h"
#include "xla/stream_executor/stream.h"
#include "xla/xla_data.pb.h"

namespace xla::gpu {

//===----------------------------------------------------------------------===//
// NcclApi
//===----------------------------------------------------------------------===//

// NcclApi hides implementation detail of collective operations built on top of
// NCCL library so that no other parts of XLA should include nccl.h header
// directly (or indirectly).

struct NcclApi {
  // Forward declarations of opaque structs corresponding to underlying platform
  // types (also defined as opaque structs).
  struct NcclComm;

  // Convenience handles for defining API functions.
  using NcclCommHandle = NcclComm*;

  // Creates a new unique clique id.
  //
  // https://docs.nvidia.com/deeplearning/nccl/user-guide/docs/api/comms.html#ncclgetuniqueid
  static absl::StatusOr<NcclCliqueId> GetUniqueId();

  // Creates a new communicator.
  //
  // https://docs.nvidia.com/deeplearning/nccl/user-guide/docs/api/comms.html#ncclcomminitrank
  static absl::StatusOr<NcclCommHandle> CommInitRank(
      int32_t nranks, const NcclCliqueId& clique_id, int32_t rank);

  // Abort any uncompleted operations and destroys the communicator. Frees
  // resources that are allocated to a communicator object comm.
  //
  // https://docs.nvidia.com/deeplearning/nccl/user-guide/docs/api/comms.html#ncclcommabort
  static absl::Status CommAbort(NcclCommHandle comm);

  // Returns the number of ranks in the NCCL communicator comm.
  //
  // https://docs.nvidia.com/deeplearning/nccl/user-guide/docs/api/comms.html#ncclcommcount
  static absl::StatusOr<int32_t> CommCount(NcclCommHandle comm);

  // Queries the progress and potential errors of asynchronous operations
  //
  // https://docs.nvidia.com/deeplearning/nccl/user-guide/docs/api/comms.html#ncclcommgetasyncerror
  static absl::Status CommGetAsyncError(NcclCommHandle comm);

  // Starts a group call.
  //
  // https://docs.nvidia.com/deeplearning/nccl/user-guide/docs/api/group.html#ncclgroupstart
  static absl::Status GroupStart();

  // Ends a group call.
  //
  // https://docs.nvidia.com/deeplearning/nccl/user-guide/docs/api/group.html#ncclgroupend
  static absl::Status GroupEnd();

  // Gather `count` values from all GPUs into recv_buffer, receiving data from
  // rank `i` at offset `i * sendcount`.
  //
  // https://docs.nvidia.com/deeplearning/nccl/user-guide/docs/api/colls.html#ncclallgather
  static absl::Status AllGather(se::DeviceMemoryBase send_buffer,
                                se::DeviceMemoryBase recv_buffer,
                                PrimitiveType dtype, size_t count,
                                NcclCommHandle comm, se::Stream* stream);
};

//===----------------------------------------------------------------------===//
// NcclApi Handles
//===----------------------------------------------------------------------===//

// TODO(ezhulenev): Remove these aliases once all users migrated to new API.
using NcclCommHandle = NcclApi::NcclCommHandle;  // NOLINT

}  // namespace xla::gpu

#endif  // XLA_SERVICE_GPU_NCCL_API_H_
