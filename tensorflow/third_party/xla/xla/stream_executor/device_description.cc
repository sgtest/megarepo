/* Copyright 2015 The TensorFlow Authors. All Rights Reserved.

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

#include "xla/stream_executor/device_description.h"

#include <cstdint>
#include <memory>
#include <string>

#include "tsl/lib/math/math_util.h"

namespace stream_executor {

static const uint64_t kUninitializedUint64 = -1ULL;
/* static */ const char *DeviceDescription::kUndefinedString = "<undefined>";

DeviceDescription::DeviceDescription()
    : device_vendor_(kUndefinedString),
      platform_version_(kUndefinedString),
      driver_version_(kUndefinedString),
      runtime_version_(kUndefinedString),
      pci_bus_id_(kUndefinedString),
      name_(kUndefinedString),
      model_str_(kUndefinedString),
      thread_dim_limit_(kUninitializedUint64, kUninitializedUint64,
                        kUninitializedUint64),
      block_dim_limit_(kUninitializedUint64, kUninitializedUint64,
                       kUninitializedUint64),
      threads_per_core_limit_(kUninitializedUint64),
      threads_per_block_limit_(kUninitializedUint64),
      threads_per_warp_(kUninitializedUint64),
      registers_per_core_limit_(kUninitializedUint64),
      registers_per_block_limit_(kUninitializedUint64),
      device_address_bits_(kUninitializedUint64),
      device_memory_size_(kUninitializedUint64),
      memory_bandwidth_(kUninitializedUint64),
      shared_memory_per_core_(kUninitializedUint64),
      shared_memory_per_block_(kUninitializedUint64),
      clock_rate_ghz_(-1.0),
      numa_node_(-1),
      core_count_(-1),
      ecc_enabled_(false) {}

namespace internal {

DeviceDescriptionBuilder::DeviceDescriptionBuilder()
    : device_description_(new DeviceDescription) {}

}  // namespace internal

CudaComputeCapability DeviceDescription::cuda_compute_capability() const {
  return cuda_compute_capability_;
}

RocmComputeCapability DeviceDescription::rocm_compute_capability() const {
  return rocm_compute_capability_;
}

bool ThreadDimOk(const DeviceDescription &device_description,
                 const ThreadDim &thread_dim) {
  const int64_t total_threads = thread_dim.x * thread_dim.y * thread_dim.z;
  const int64_t threads_per_block_limit =
      device_description.threads_per_block_limit();
  if (total_threads > threads_per_block_limit) {
    VLOG(2) << "exceeded total-thread-per-block limit: " << total_threads
            << " vs limit " << threads_per_block_limit;
    return false;
  }

  const auto &limit = device_description.thread_dim_limit();
  bool ok = thread_dim.x <= limit.x && thread_dim.y <= limit.y &&
            thread_dim.z <= limit.z;
  if (!ok) {
    VLOG(2) << "thread dim " << thread_dim.ToString()
            << " exceeds limit constraints of " << limit.ToString();
  }
  return ok;
}

void CalculateDimensionality(const DeviceDescription &device_description,
                             int64_t element_count, int64_t *threads_per_block,
                             int64_t *block_count) {
  *threads_per_block = device_description.threads_per_block_limit();
  *block_count = tsl::MathUtil::CeilOfRatio(element_count, *threads_per_block);
  if (*block_count == 1) {
    CHECK_LE(element_count, *threads_per_block);
    *threads_per_block = element_count;
  }
}

}  // namespace stream_executor
