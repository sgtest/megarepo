/* Copyright 2024 The OpenXLA Authors.

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

#ifndef XLA_SERVICE_GPU_NCCL_CLIQUE_KEY_H_
#define XLA_SERVICE_GPU_NCCL_CLIQUE_KEY_H_

#include <array>
#include <cstdint>
#include <functional>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

#include "absl/status/statusor.h"
#include "absl/types/span.h"
#include "xla/service/global_device_id.h"

namespace xla::gpu {

// A standalone library without any dependencies on NCCL that allows us to
// include this header in all of XLA without worrying about NCCL availability.

//===----------------------------------------------------------------------===//
// AsyncStreamKind
//===----------------------------------------------------------------------===//

// We include a stream kind into the NCCL clique key because in XLA we do not
// share communicators for collective operations of different kind (CUDA-graph
// launched, async collectives, sync collectives) as it can lead to dead locks.
//
// We carefully isolate different kinds of collectives using separate
// communicators and guarantee that all collective operations have a total order
// that will not create a deadlock.
//
// See more details in `nccl_clique` library.

enum class AsyncStreamKind : int64_t {
  kCollective = 0,  // Stream for asynchronous collective ops.
  kP2P = 1,         // Stream for P2P Send and Recv ops.
};

constexpr static int64_t kAsyncStreamTotal =
    static_cast<int64_t>(AsyncStreamKind::kP2P) + 1;

// Assigns a unique ID to a stream for asynchronous or synchronous execution.
// These IDs can be used, for example, to look up the NCCL communicator.
inline uint64_t GetStreamId(
    bool is_async, AsyncStreamKind stream_kind = AsyncStreamKind::kCollective) {
  return is_async ? static_cast<int64_t>(stream_kind) + 1 : 0;
}

//===----------------------------------------------------------------------===//
// NcclCliqueKey
//===----------------------------------------------------------------------===//

// Key for naming up a particular NCCL clique. This is just a set of unique
// device IDs (i.e. GPU IDs) and a stream_id. The device IDs must be global
// within a cluster. The stream_id is used to create different NCCL clique and
// communicators for collectives executed on different streams within an
// executable.
class NcclCliqueKey {
 public:
  explicit NcclCliqueKey(std::vector<GlobalDeviceId> devices,
                         int64_t stream_id = 0);

  absl::Span<const GlobalDeviceId> devices() const;

  // Returns the rank of the global device in the clique.
  std::optional<int64_t> rank(GlobalDeviceId id) const;

  std::string ToString() const;

  template <typename H>
  friend H AbslHashValue(H h, const NcclCliqueKey& k);

  friend bool operator==(const NcclCliqueKey& a, const NcclCliqueKey& b);
  friend bool operator<(const NcclCliqueKey& a, const NcclCliqueKey& b);

 private:
  const std::vector<GlobalDeviceId> devices_;
  const int64_t stream_id_;
};

template <typename H>
H AbslHashValue(H h, const NcclCliqueKey& k) {
  return H::combine(std::move(h), k.devices_, k.stream_id_);
}

bool operator==(const NcclCliqueKey& a, const NcclCliqueKey& b);
bool operator<(const NcclCliqueKey& a, const NcclCliqueKey& b);

//===----------------------------------------------------------------------===//
// NcclCliqueId
//===----------------------------------------------------------------------===//

// All collective cliques have a globally unique ID (128 bytes long for NCCL)
// that allows multiple hosts and devices to find each other and agree who is a
// member of a clique. It is a user responsibility to redistribute this id to
// all participating hosts (i.e. JAX uses shared KV store for that). For single
// host collective operations XLA automatically generates a unique id for local
// cliques (cliques consisting of devices visible from a process).

// A globally unique collective clique identifier.
class NcclCliqueId {
 public:
  static constexpr int32_t kSize = 128;

  static absl::StatusOr<NcclCliqueId> FromString(std::string_view str);

  NcclCliqueId();
  explicit NcclCliqueId(char bytes[kSize]);

  absl::Span<const char> data() const;
  std::string ToString() const;

  template <typename H>
  friend H AbslHashValue(H h, const NcclCliqueId& id);

 private:
  std::array<char, kSize> data_;
};

template <typename H>
H AbslHashValue(H h, const NcclCliqueId& id) {
  return H::combine(std::move(h), id.data());
}

// A callback to get a unique clique id (see `ncclUniqueId` documentation).
using NcclCliqueIdCallback =  // NOLINT
    std::function<absl::StatusOr<NcclCliqueId>(const NcclCliqueKey&)>;

}  // namespace xla::gpu

#endif  // XLA_SERVICE_GPU_NCCL_CLIQUE_KEY_H_
