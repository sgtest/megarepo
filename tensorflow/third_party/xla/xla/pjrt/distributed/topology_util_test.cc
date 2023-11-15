/* Copyright 2020 The TensorFlow Authors. All Rights Reserved.

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

#include "xla/pjrt/distributed/topology_util.h"

#include <string>
#include <string_view>
#include <vector>

#include "absl/container/flat_hash_map.h"
#include "absl/status/status.h"
#include "absl/synchronization/mutex.h"
#include "absl/time/time.h"
#include "xla/pjrt/distributed/protocol.pb.h"
#include "xla/status.h"
#include "xla/statusor.h"
#include "tsl/lib/core/status_test_util.h"
#include "tsl/platform/env.h"
#include "tsl/platform/test.h"
#include "tsl/platform/threadpool.h"

namespace xla {
namespace {

TEST(TopologyTest, BuildGlobalTopology) {
  std::vector<LocalTopologyProto> locals(2);
  DeviceProto* d0 = locals[0].add_devices();
  d0->set_local_device_ordinal(0);
  DeviceProto* d1 = locals[0].add_devices();
  d1->set_local_device_ordinal(0);
  DeviceProto* d2 = locals[1].add_devices();
  d2->set_local_device_ordinal(0);
  DeviceProto* d3 = locals[1].add_devices();
  d3->set_local_device_ordinal(1);

  GlobalTopologyProto global =
      BuildGlobalTopology(absl::Span<LocalTopologyProto>(locals));
  EXPECT_EQ(global.nodes_size(), 2);
  EXPECT_EQ(global.nodes()[0].devices_size(), 2);
  EXPECT_EQ(global.nodes()[1].devices_size(), 2);
}

TEST(TopologyTest, ExchangeTopology) {
  int num_nodes = 2;
  std::vector<LocalTopologyProto> locals(num_nodes);
  DeviceProto* d0 = locals[0].add_devices();
  d0->set_local_device_ordinal(0);
  DeviceProto* d1 = locals[0].add_devices();
  d1->set_local_device_ordinal(0);
  DeviceProto* d2 = locals[1].add_devices();
  d2->set_local_device_ordinal(0);
  DeviceProto* d3 = locals[1].add_devices();
  d3->set_local_device_ordinal(1);

  absl::Mutex mu;
  absl::flat_hash_map<std::string, std::string> kv;

  auto kv_get = [&](std::string_view key,
                    absl::Duration timeout) -> xla::StatusOr<std::string> {
    absl::MutexLock lock(&mu);
    auto ready = [&]() { return kv.contains(key); };
    if (mu.AwaitWithTimeout(absl::Condition(&ready), timeout)) {
      return kv[key];
    }
    return absl::NotFoundError("key not found");
  };

  auto kv_put = [&](std::string_view key,
                    std::string_view value) -> xla::Status {
    absl::MutexLock lock(&mu);
    kv[key] = value;
    return absl::OkStatus();
  };

  std::vector<GlobalTopologyProto> globals(num_nodes);
  {
    tsl::thread::ThreadPool thread_pool(tsl::Env::Default(), "TestPool",
                                        num_nodes);
    for (int i = 0; i < num_nodes; i++) {
      thread_pool.Schedule([&, i] {
        TF_ASSERT_OK(ExchangeTopologies(
            /*platform=*/"cuda", /*node_id=*/i, num_nodes,
            /*get_local_topology_timeout=*/
            absl::Seconds(10), /*get_global_topology_timeout=*/
            absl::Seconds(10), kv_get, kv_put, locals[i], &globals[i]));
      });
    }
  }
  for (const GlobalTopologyProto& global : globals) {
    EXPECT_EQ(global.nodes_size(), 2);
    EXPECT_EQ(global.nodes()[0].devices_size(), 2);
    EXPECT_EQ(global.nodes()[1].devices_size(), 2);
  }
}

}  // namespace
}  // namespace xla
