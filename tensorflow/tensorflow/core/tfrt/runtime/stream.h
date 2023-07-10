/* Copyright 2023 The TensorFlow Authors. All Rights Reserved.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.  See
the License for the specific language governing permissions and limitations
under the License.
==============================================================================*/
#ifndef TENSORFLOW_CORE_TFRT_RUNTIME_STREAM_H_
#define TENSORFLOW_CORE_TFRT_RUNTIME_STREAM_H_

#include <cstdint>
#include <functional>
#include <memory>
#include <optional>
#include <string>
#include <utility>

#include "absl/container/flat_hash_map.h"
#include "absl/functional/any_invocable.h"
#include "absl/status/statusor.h"
#include "absl/strings/str_format.h"
#include "mlir/IR/BuiltinOps.h"  // from @llvm-project
#include "tensorflow/core/framework/tensor.h"
#include "tensorflow/core/tfrt/runtime/channel.h"
#include "tensorflow/tsl/platform/env.h"

namespace tensorflow {
namespace tfrt_stub {

template <typename Derived>
struct SafeId {
  SafeId() : id(0) {}
  explicit constexpr SafeId(int64_t id) : id(id) {}

  using Base = SafeId;

  int64_t id;

  friend bool operator==(const Derived& x, const Derived& y) {
    return x.id == y.id;
  }

  template <typename Sink>
  friend void AbslStringify(Sink& sink, const Derived& x) {
    absl::Format(&sink, "%d", x.id);
  }

  template <typename H>
  friend H AbslHashValue(H h, const Derived& x) {
    return H::combine(std::move(h), x.id);
  }
};

struct StreamedResult {
  absl::flat_hash_map<std::string, tensorflow::Tensor> tensors;
  absl::Time enqueued_time;
};

struct StreamCallbackId : SafeId<StreamCallbackId> {
  using Base::Base;
};

struct StepId : SafeId<StepId> {
  using Base::Base;

  bool valid() const { return id != 0; }
  static constexpr StepId GetInvalidStepId() { return StepId(0); }
};

class StreamInterface {
 public:
  explicit StreamInterface(std::string controller_address)
      : controller_address_(std::move(controller_address)) {}
  virtual ~StreamInterface() = default;

  absl::string_view controller_address() const { return controller_address_; }

  virtual void RecordDequeueLatency(absl::string_view model_name,
                                    absl::Duration latency) {}

  virtual void RecordCallbackLatency(absl::string_view model_name,
                                     absl::Duration latency) {}

 private:
  std::string controller_address_;
};

class ScopedStreamCallback;

class StreamInterfaceFactory {
 public:
  void Register(absl::AnyInvocable<
                absl::StatusOr<std::unique_ptr<StreamInterface>>() const>
                    interface_factory) {
    absl::MutexLock lock(&mu_);
    interface_factory_ = std::move(interface_factory);
  }

  absl::StatusOr<std::unique_ptr<StreamInterface>> CreateStreamInterface()
      const {
    absl::MutexLock lock(&mu_);
    return interface_factory_();
  }

 private:
  mutable absl::Mutex mu_;
  absl::AnyInvocable<absl::StatusOr<std::unique_ptr<StreamInterface>>() const>
      interface_factory_ ABSL_GUARDED_BY(mu_) = []() {
        return absl::InternalError(
            "The factory for StreamInterface is not registered.");
      };
};

// Returns the global factory for the stream interface. The factory for the
// stream interface must be registered first before calling
// GetGlobalStreamCallbackRegistry().
StreamInterfaceFactory& GetGlobalStreamInterfaceFactory();

// Mapping from tuples of (callback_id, step_id) to callback states. The mapping
// is stored in a global variable so that it can be shared between
// `ScopedStreamCallback` and `InvokeStreamCallbackOp`.
//
// This class is thread-safe.
class StreamCallbackRegistry {
 public:
  explicit StreamCallbackRegistry(std::unique_ptr<StreamInterface> interface)
      : interface_(std::move(interface)) {
    DCHECK(interface_);
  }

  // Registers a callback under the given id. A stream callback is uniquely
  // identified by a tuple of a callback id (unique to each executable) and a
  // step id (unique to each invocation of a given executable). Returns an RAII
  // object that removes the callback from the registry on its deallocation, or
  // an error if the id already exists in the registry.
  //
  // If a program runs `tf.PwStreamResults` with a matching callback/step id,
  // `callback` will be called with the arguments of `tf.PwStreamResults`.
  //
  // All invocations to `callback` are handled serially by a single thread, so
  // `callback` doesn't need to be thread-safe even if multiple
  // `tf.PwStreamResults` ops may run concurrently.
  absl::StatusOr<ScopedStreamCallback> Register(
      absl::string_view model_name, StreamCallbackId callback_id,
      StepId step_id,
      absl::AnyInvocable<
          void(absl::flat_hash_map<std::string, tensorflow::Tensor>)>
          callback);

  absl::Status Write(StreamCallbackId callback_id, StepId step_id,
                     StreamedResult result);

  StreamInterface& stream_interface() const { return *interface_; }

 private:
  friend class ScopedStreamCallback;

  struct CallbackState {
    std::unique_ptr<tsl::Thread> thread;
    UnboundedChannel<StreamedResult> channel;
  };

  std::unique_ptr<CallbackState> Unregister(StreamCallbackId callback_id,
                                            StepId step_id);

  std::unique_ptr<StreamInterface> interface_;

  mutable absl::Mutex mu_;
  absl::flat_hash_map<std::pair<StreamCallbackId, StepId>,
                      std::unique_ptr<CallbackState>>
      stream_callbacks_ ABSL_GUARDED_BY(mu_);
};

// Returns the global registry for the stream callbacks. The stream interface
// must have been registered through GetGlobalStreamInterfaceFactory() before
// calling this function.
StreamCallbackRegistry& GetGlobalStreamCallbackRegistry();

// Creates a new stream callback id and rewrites the given module with
// information required to trigger this callback remotely. Returns the callback
// id, or `std::nullopt` if the module has no stream outputs.
absl::StatusOr<std::optional<StreamCallbackId>> CreateStreamCallbackId(
    absl::string_view model_name, mlir::ModuleOp module);

// Implements an RAII object that registers a callback to be called on receiving
// streamed tensors.
class ScopedStreamCallback {
 public:
  ScopedStreamCallback() = default;

  // Moveable but not copyable.
  ScopedStreamCallback(ScopedStreamCallback&& other);
  ScopedStreamCallback& operator=(ScopedStreamCallback&& other);

  ~ScopedStreamCallback() { Unregister(); }

 private:
  friend class StreamCallbackRegistry;

  explicit ScopedStreamCallback(StreamCallbackRegistry* registry,
                                StreamCallbackId callback_id, StepId step_id)
      : registry_(registry), callback_id_(callback_id), step_id_(step_id) {}

  void Unregister();

  StreamCallbackRegistry* registry_ = nullptr;
  std::optional<StreamCallbackId> callback_id_;
  StepId step_id_ = StepId::GetInvalidStepId();
};

}  // namespace tfrt_stub
}  // namespace tensorflow

#endif  // TENSORFLOW_CORE_TFRT_RUNTIME_STREAM_H_
