/* Copyright 2015 The OpenXLA Authors.

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

// Implements the StreamExecutor interface by passing through to its
// implementation_ value (in pointer-to-implementation style), which
// implements StreamExecutorInterface.

#include "xla/stream_executor/stream_executor_pimpl.h"

#include <atomic>
#include <cstddef>
#include <cstdint>
#include <memory>
#include <optional>
#include <string>
#include <tuple>
#include <utility>
#include <variant>

#include "absl/functional/any_invocable.h"
#include "absl/log/check.h"
#include "absl/log/log.h"
#include "absl/status/status.h"
#include "absl/status/statusor.h"
#include "absl/strings/str_cat.h"
#include "absl/strings/str_format.h"
#include "absl/synchronization/mutex.h"
#include "absl/types/span.h"
#include "xla/stream_executor/allocator_stats.h"
#include "xla/stream_executor/blas.h"
#include "xla/stream_executor/command_buffer.h"
#include "xla/stream_executor/device_description.h"
#include "xla/stream_executor/device_memory_allocator.h"
#include "xla/stream_executor/dnn.h"
#include "xla/stream_executor/event.h"
#include "xla/stream_executor/fft.h"
#include "xla/stream_executor/kernel.h"
#include "xla/stream_executor/kernel_spec.h"
#include "xla/stream_executor/launch_dim.h"
#include "xla/stream_executor/memory_allocation.h"
#include "xla/stream_executor/module_spec.h"
#include "xla/stream_executor/platform.h"
#include "xla/stream_executor/stream.h"
#include "xla/stream_executor/stream_executor_interface.h"
#include "xla/tsl/util/env_var.h"
#include "tsl/platform/errors.h"
#include "tsl/platform/numbers.h"
#include "tsl/platform/status.h"
#include "tsl/platform/statusor.h"

namespace stream_executor {

// Get per-device memory limit in bytes. Returns 0 if
// TF_PER_DEVICE_MEMORY_LIMIT_MB environment variable is not set.
static int64_t GetMemoryLimitBytes() {
  int64_t value;
  TF_CHECK_OK(
      tsl::ReadInt64FromEnvVar("TF_PER_DEVICE_MEMORY_LIMIT_MB", 0, &value));
  return value * (1ll << 20);
}

StreamExecutor::StreamExecutor(const Platform* platform)
    : platform_(platform),
      memory_limit_bytes_(GetMemoryLimitBytes()),
      allocator_(this) {}

const DeviceDescription& StreamExecutor::GetDeviceDescription() const {
  absl::MutexLock lock(&mu_);
  if (device_description_ != nullptr) {
    return *device_description_;
  }

  device_description_ = CreateDeviceDescription().value();
  return *device_description_;
}

absl::StatusOr<DeviceMemoryBase> StreamExecutor::GetUntypedSymbol(
    const std::string& symbol_name, ModuleHandle module_handle) {
  // If failed to get the symbol, opaque/bytes are unchanged. Initialize them to
  // be nullptr/0 for consistency with DeviceMemory semantics.
  void* opaque = nullptr;
  size_t bytes = 0;
  if (GetSymbol(symbol_name, module_handle, &opaque, &bytes)) {
    return DeviceMemoryBase(opaque, bytes);
  }

  return absl::NotFoundError(
      absl::StrCat("Check if module containing symbol ", symbol_name,
                   " is loaded (module_handle = ",
                   reinterpret_cast<uintptr_t>(module_handle.id()), ")"));
}

absl::Status StreamExecutor::SynchronousMemcpyD2H(
    const DeviceMemoryBase& device_src, int64_t size, void* host_dst) {
  return SynchronousMemcpy(host_dst, device_src, size);
}

absl::Status StreamExecutor::SynchronousMemcpyH2D(
    const void* host_src, int64_t size, DeviceMemoryBase* device_dst) {
  return SynchronousMemcpy(device_dst, host_src, size);
}

absl::StatusOr<std::unique_ptr<Stream>> StreamExecutor::CreateStream(
    std::optional<std::variant<StreamPriority, int>> priority) {
  auto stream = std::make_unique<Stream>(this);
  TF_RETURN_IF_ERROR(stream->Initialize(priority));
  return std::move(stream);
}

StreamExecutorMemoryAllocator::StreamExecutorMemoryAllocator(
    StreamExecutor* executor)
    : DeviceMemoryAllocator(executor->platform()) {
  stream_executors_ = {executor};
}

StreamExecutorMemoryAllocator::StreamExecutorMemoryAllocator(
    const Platform* platform,
    absl::Span<StreamExecutor* const> stream_executors)
    : DeviceMemoryAllocator(platform),
      stream_executors_(stream_executors.begin(), stream_executors.end()) {}

absl::StatusOr<OwningDeviceMemory> StreamExecutorMemoryAllocator::Allocate(
    int device_ordinal, uint64_t size, bool retry_on_failure,
    int64_t memory_space) {
  TF_ASSIGN_OR_RETURN(StreamExecutor * executor,
                      GetStreamExecutor(device_ordinal));
  DeviceMemoryBase result =
      executor->AllocateArray<uint8_t>(size, memory_space);
  if (size > 0 && result == nullptr) {
    return absl::ResourceExhaustedError(absl::StrFormat(
        "Failed to allocate request for %s (%uB) on device ordinal %d",
        tsl::strings::HumanReadableNumBytes(size), size, device_ordinal));
  }
  VLOG(3) << absl::StreamFormat("Allocated %s (%uB) on device ordinal %d: %p",
                                tsl::strings::HumanReadableNumBytes(size), size,
                                device_ordinal, result.opaque());
  return OwningDeviceMemory(result, device_ordinal, this);
}

absl::Status StreamExecutorMemoryAllocator::Deallocate(int device_ordinal,
                                                       DeviceMemoryBase mem) {
  if (!mem.is_null()) {
    TF_ASSIGN_OR_RETURN(StreamExecutor * executor,
                        GetStreamExecutor(device_ordinal));
    VLOG(3) << absl::StreamFormat("Freeing %p on device ordinal %d",
                                  mem.opaque(), device_ordinal);
    executor->Deallocate(&mem);
  }
  return absl::OkStatus();
}

absl::StatusOr<StreamExecutor*>
StreamExecutorMemoryAllocator::GetStreamExecutor(int device_ordinal) const {
  if (device_ordinal < 0) {
    return absl::InvalidArgumentError(absl::StrFormat(
        "device ordinal value (%d) must be non-negative", device_ordinal));
  }
  for (StreamExecutor* se : stream_executors_) {
    if (se->device_ordinal() == device_ordinal) {
      return se;
    }
  }
  return absl::NotFoundError(
      absl::StrFormat("Device %s:%d present but not supported",
                      platform()->Name(), device_ordinal));
}

bool StreamExecutorMemoryAllocator::AllowsAsynchronousDeallocation() const {
  return false;
}

absl::StatusOr<Stream*> StreamExecutorMemoryAllocator::GetStream(
    int device_ordinal) {
  CHECK(!AllowsAsynchronousDeallocation())
      << "The logic below only works for synchronous allocators";
  TF_ASSIGN_OR_RETURN(StreamExecutor * executor,
                      GetStreamExecutor(device_ordinal));
  absl::MutexLock lock(&mutex_);
  if (!streams_.count(device_ordinal)) {
    auto p = streams_.emplace(std::piecewise_construct,
                              std::forward_as_tuple(device_ordinal),
                              std::forward_as_tuple(executor));
    TF_RETURN_IF_ERROR(p.first->second.Initialize());
    return &p.first->second;
  }
  return &streams_.at(device_ordinal);
}

}  // namespace stream_executor
