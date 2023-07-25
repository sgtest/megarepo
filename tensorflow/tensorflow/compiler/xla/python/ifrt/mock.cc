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

#include "tensorflow/compiler/xla/python/ifrt/mock.h"

#include <functional>
#include <memory>
#include <optional>
#include <utility>

namespace xla {
namespace ifrt {

char MockArray::ID = 0;
char MockClient::ID = 0;
char MockCompiler::ID = 0;
char MockExecutable::ID = 0;
char MockLoadedExecutable::ID = 0;
char MockHostCallback::ID = 0;
char MockLoadedHostCallback::ID = 0;
char MockSharding::ID = 0;

// LINT.IfChange(MockArrayDelegation)
MockArray::MockArray(tsl::RCReference<xla::ifrt::Array> delegated)
    : delegated_(std::move(delegated)) {
  ON_CALL(*this, GetReadyFuture).WillByDefault([this]() {
    return delegated_->GetReadyFuture();
  });
  ON_CALL(*this, Delete).WillByDefault([this]() {
    return delegated_->Delete();
  });
  ON_CALL(*this, IsDeleted).WillByDefault([this]() {
    return delegated_->IsDeleted();
  });
  ON_CALL(*this, DebugString).WillByDefault([this]() {
    return delegated_->DebugString();
  });
  ON_CALL(*this, dtype).WillByDefault([this]() { return delegated_->dtype(); });
  ON_CALL(*this, shape).WillByDefault([this]() -> const Shape& {
    return delegated_->shape();
  });
  ON_CALL(*this, sharding).WillByDefault([this]() -> const Sharding& {
    return delegated_->sharding();
  });
  ON_CALL(*this, shared_ptr_sharding).WillByDefault([this]() {
    return delegated_->shared_ptr_sharding();
  });
  ON_CALL(*this, DisassembleIntoSingleDeviceArrays)
      .WillByDefault([this](ArrayCopySemantics semantics) {
        return delegated_->DisassembleIntoSingleDeviceArrays(semantics);
      });
  ON_CALL(*this, FullyReplicatedShard)
      .WillByDefault([this](ArrayCopySemantics semantics) {
        return delegated_->FullyReplicatedShard(semantics);
      });
  ON_CALL(*this, CopyToHostBuffer)
      .WillByDefault(
          [this](void* data,
                 std::optional<absl::Span<const int64_t>> byte_strides,
                 ArrayCopySemantics semantics) {
            return delegated_->CopyToHostBuffer(data, byte_strides, semantics);
          });
  ON_CALL(*this, Reshard)
      .WillByDefault([this](std::shared_ptr<const Sharding> new_sharding,
                            ArrayCopySemantics semantics) {
        return delegated_->Reshard(std::move(new_sharding), semantics);
      });
}
// LINT.ThenChange()

// LINT.IfChange(MockClientDelegation)
MockClient::MockClient(std::unique_ptr<xla::ifrt::Client> delegated)
    : delegated_(std::move(delegated)) {
  ON_CALL(*this, MakeArrayFromHostBuffer)
      .WillByDefault([this](
                         const void* data, DType dtype, Shape shape,
                         std::optional<absl::Span<const int64_t>> byte_strides,
                         std::shared_ptr<const Sharding> sharding,
                         HostBufferSemantics semantics,
                         std::function<void()> on_done_with_host_buffer) {
        return delegated_->MakeArrayFromHostBuffer(
            data, dtype, std::move(shape), byte_strides, std::move(sharding),
            semantics, std::move(on_done_with_host_buffer));
      });
  ON_CALL(*this, AssembleArrayFromSingleDeviceArrays)
      .WillByDefault([this](Shape shape,
                            std::shared_ptr<const Sharding> sharding,
                            absl::Span<tsl::RCReference<Array>> arrays,
                            ArrayCopySemantics semantics) {
        return delegated_->AssembleArrayFromSingleDeviceArrays(
            std::move(shape), std::move(sharding), arrays, semantics);
      });
  ON_CALL(*this, MakeTuple)
      .WillByDefault([this](absl::Span<tsl::RCReference<Value>> values) {
        return delegated_->MakeTuple(values);
      });

  ON_CALL(*this, runtime_type).WillByDefault([this]() {
    return delegated_->runtime_type();
  });
  ON_CALL(*this, platform_name).WillByDefault([this]() {
    return delegated_->platform_name();
  });
  ON_CALL(*this, platform_version).WillByDefault([this]() {
    return delegated_->platform_version();
  });
  ON_CALL(*this, platform_id).WillByDefault([this]() {
    return delegated_->platform_id();
  });
  ON_CALL(*this, device_count).WillByDefault([this]() {
    return delegated_->device_count();
  });
  ON_CALL(*this, addressable_device_count).WillByDefault([this]() {
    return delegated_->addressable_device_count();
  });
  ON_CALL(*this, devices).WillByDefault([this]() {
    return delegated_->devices();
  });
  ON_CALL(*this, addressable_devices).WillByDefault([this]() {
    return delegated_->addressable_devices();
  });
  ON_CALL(*this, process_index).WillByDefault([this]() {
    return delegated_->process_index();
  });
  ON_CALL(*this, GetDefaultDeviceAssignment)
      .WillByDefault([this](int num_replicas, int num_partitions) {
        return delegated_->GetDefaultDeviceAssignment(num_replicas,
                                                      num_partitions);
      });
  ON_CALL(*this, LookupDevice).WillByDefault([this](int device_id) {
    return delegated_->LookupDevice(device_id);
  });
  ON_CALL(*this, GetDefaultCompiler).WillByDefault([this]() {
    return delegated_->GetDefaultCompiler();
  });
}
// LINT.ThenChange()

// LINT.IfChange(MockDeviceDelegation)
MockDevice::MockDevice(Device* delegated) : delegated_(delegated) {
  ON_CALL(*this, client).WillByDefault([this]() {
    return delegated_->client();
  });
  ON_CALL(*this, IsAddressable).WillByDefault([this]() {
    return delegated_->IsAddressable();
  });
  ON_CALL(*this, description)
      .WillByDefault([this]() -> const xla::PjRtDeviceDescription& {
        return delegated_->description();
      });
  ON_CALL(*this, id).WillByDefault([this]() { return delegated_->id(); });
  ON_CALL(*this, process_index).WillByDefault([this]() {
    return delegated_->process_index();
  });
  ON_CALL(*this, local_hardware_id).WillByDefault([this]() {
    return delegated_->local_hardware_id();
  });
  ON_CALL(*this, device_kind).WillByDefault([this]() {
    return delegated_->device_kind();
  });
  ON_CALL(*this, DebugString).WillByDefault([this]() {
    return delegated_->DebugString();
  });
  ON_CALL(*this, ToString).WillByDefault([this]() {
    return delegated_->ToString();
  });
  ON_CALL(*this, Attributes).WillByDefault([this]() {
    return delegated_->Attributes();
  });
  ON_CALL(*this, CreateAsyncTrackingEvent)
      .WillByDefault([this](absl::string_view description) {
        return delegated_->CreateAsyncTrackingEvent(description);
      });
  ON_CALL(*this, TransferToInfeed)
      .WillByDefault([this](const LiteralSlice& literal) {
        return delegated_->TransferToInfeed(literal);
      });
  ON_CALL(*this, TransferFromOutfeed)
      .WillByDefault([this](MutableBorrowingLiteral literal) {
        return delegated_->TransferFromOutfeed(std::move(literal));
      });
  ON_CALL(*this, default_memory_space).WillByDefault([this]() {
    return delegated_->default_memory_space();
  });
  ON_CALL(*this, GetAllocatorStats).WillByDefault([this]() {
    return delegated_->GetAllocatorStats();
  });
  ON_CALL(*this, memory_spaces).WillByDefault([this]() {
    return delegated_->memory_spaces();
  });
}
// LINT.ThenChange()

}  // namespace ifrt
}  // namespace xla
