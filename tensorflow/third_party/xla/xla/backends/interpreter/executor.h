/* Copyright 2017 The OpenXLA Authors.

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

// Declares the XlaInterpreterExecutor class, which is a CPU-only implementation
// of the StreamExecutor interface. For now, this is used for testing and to
// examine the performance of host-based StreamExecutor code.
#ifndef XLA_BACKENDS_INTERPRETER_EXECUTOR_H_
#define XLA_BACKENDS_INTERPRETER_EXECUTOR_H_

#include <memory>

#include "absl/functional/any_invocable.h"
#include "absl/types/span.h"
#include "xla/shape_util.h"
#include "xla/stream_executor/blas.h"
#include "xla/stream_executor/device_description.h"
#include "xla/stream_executor/device_memory.h"
#include "xla/stream_executor/event.h"
#include "xla/stream_executor/host/host_stream.h"
#include "xla/stream_executor/kernel.h"
#include "xla/stream_executor/kernel_spec.h"
#include "xla/stream_executor/launch_dim.h"
#include "xla/stream_executor/stream.h"
#include "xla/stream_executor/stream_executor.h"
#include "xla/stream_executor/stream_executor_internal.h"
#include "xla/xla_data.pb.h"

namespace stream_executor {
namespace interpreter {

using Args = absl::Span<const DeviceMemoryBase>;

class XlaInterpreterExecutor : public internal::StreamExecutorInterface {
 public:
  XlaInterpreterExecutor() = default;

  absl::Status Init(int device_ordinal) override {
    device_ordinal_ = device_ordinal;
    return absl::OkStatus();
  }

  int device_ordinal() const override { return device_ordinal_; };
  absl::Status GetKernel(const MultiKernelLoaderSpec &spec,
                         Kernel *kernel) override {
    return tsl::errors::Unimplemented("Not Implemented");
  }
  absl::Status Launch(Stream *stream, const ThreadDim &thread_dims,
                      const BlockDim &block_dims, const Kernel &kernel,
                      const KernelArgs &args) override {
    return tsl::errors::Unimplemented("Not Implemented");
  }

  DeviceMemoryBase Allocate(uint64_t size, int64_t memory_space) override;
  void Deallocate(DeviceMemoryBase *mem) override;

  void *HostMemoryAllocate(uint64_t size) override { return new char[size]; }
  void HostMemoryDeallocate(void *mem) override {
    delete[] static_cast<char *>(mem);
  }
  bool HostMemoryRegister(void *mem, uint64_t size) override { return true; }
  bool HostMemoryUnregister(void *mem) override { return true; }

  absl::Status Memcpy(Stream *stream, void *host_dst,
                      const DeviceMemoryBase &dev_src, uint64_t size) override;
  absl::Status Memcpy(Stream *stream, DeviceMemoryBase *dev_dst,
                      const void *host_src, uint64_t size) override;
  bool MemcpyDeviceToDevice(Stream *stream, DeviceMemoryBase *pop_dst,
                            const DeviceMemoryBase &host_src,
                            uint64_t size) override {
    return false;
  }

  absl::Status MemZero(Stream *stream, DeviceMemoryBase *location,
                       uint64_t size) override {
    return tsl::errors::Internal("Interpreter can not memzero");
  }
  absl::Status Memset(Stream *stream, DeviceMemoryBase *location,
                      uint8_t pattern, uint64_t size) override {
    return tsl::errors::Internal("Interpreter can not memset");
  }
  absl::Status Memset32(Stream *stream, DeviceMemoryBase *location,
                        uint32_t pattern, uint64_t size) override {
    return tsl::errors::Internal("Interpreter can not memset");
  }

  // No "synchronize all activity" implemented for this platform at the moment.
  bool SynchronizeAllActivity() override { return true; }
  absl::Status SynchronousMemZero(DeviceMemoryBase *location,
                                  uint64_t size) override {
    return tsl::errors::Internal("Interpreter can not memzero");
  }

  absl::Status SynchronousMemSet(DeviceMemoryBase *location, int value,
                                 uint64_t size) override {
    return tsl::errors::Internal("Interpreter can not memset");
  }

  absl::Status SynchronousMemcpy(DeviceMemoryBase *dev_dst,
                                 const void *host_src, uint64_t size) override;
  absl::Status SynchronousMemcpy(void *host_dst,
                                 const DeviceMemoryBase &dev_src,
                                 uint64_t size) override;
  absl::Status SynchronousMemcpyDeviceToDevice(DeviceMemoryBase *pop_dst,
                                               const DeviceMemoryBase &pop_src,
                                               uint64_t size) override {
    return absl::Status{absl::StatusCode::kUnimplemented, ""};
  }

  bool HostCallback(Stream *stream,
                    absl::AnyInvocable<absl::Status() &&> callback) override;

  absl::Status AllocateEvent(Event *event) override { return absl::OkStatus(); }

  absl::Status DeallocateEvent(Event *event) override {
    return absl::OkStatus();
  }

  absl::Status RecordEvent(Stream *stream, Event *event) override {
    return absl::Status{absl::StatusCode::kUnimplemented, "RecordEvent"};
  }

  absl::Status WaitForEvent(Stream *stream, Event *event) override {
    return absl::Status{absl::StatusCode::kUnimplemented, "WaitForEvent"};
  }

  Event::Status PollForEventStatus(Event *event) override {
    return Event::Status::kError;
  }

  bool AllocateStream(Stream *stream) override { return true; }
  void DeallocateStream(Stream *stream) override {}
  bool CreateStreamDependency(Stream *dependent, Stream *other) override;

  absl::Status BlockHostUntilDone(Stream *stream) override;

  bool DeviceMemoryUsage(int64_t *free, int64_t *total) const override {
    return false;
  }

  absl::StatusOr<std::unique_ptr<DeviceDescription>> CreateDeviceDescription()
      const override {
    return CreateDeviceDescription(0);
  }

  static absl::StatusOr<std::unique_ptr<DeviceDescription>>
  CreateDeviceDescription(int device_ordinal);

  absl::Status EnablePeerAccessTo(StreamExecutorInterface *other) override {
    return absl::OkStatus();
  }

  bool CanEnablePeerAccessTo(StreamExecutorInterface *other) override {
    return true;
  }

  std::unique_ptr<internal::EventInterface> CreateEventImplementation()
      override {
    return nullptr;
  }

  std::unique_ptr<internal::StreamInterface> GetStreamImplementation()
      override {
    return std::make_unique<host::HostStream>();
  }

 private:
  // The device ordinal value that this executor was initialized with; recorded
  // for use in getting device metadata. Immutable post-initialization.
  int device_ordinal_;

  DeviceMemoryBase AllocateSingleOutput(const xla::Shape &shape);

  absl::StatusOr<DeviceMemoryBase> AllocateOutputBuffer(
      const xla::Shape &shape);
};

}  // namespace interpreter
}  // namespace stream_executor

#endif  // XLA_BACKENDS_INTERPRETER_EXECUTOR_H_
