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

#ifndef TENSORFLOW_CORE_KERNELS_BATCHING_UTIL_WARMUP_H_
#define TENSORFLOW_CORE_KERNELS_BATCHING_UTIL_WARMUP_H_

#include <optional>
#include <string>
#include <utility>

#include "absl/container/flat_hash_set.h"
#include "absl/hash/hash.h"
#include "absl/status/statusor.h"
#include "absl/synchronization/mutex.h"
#include "tensorflow/core/kernels/batching_util/warmup.h"
#include "tensorflow/core/protobuf/config.pb.h"
#include "tensorflow/tsl/platform/logging.h"

namespace tensorflow {
namespace serving {

// Global registry for model's warm-up states. Before a model executes warm-up
// requests, it is registered here so that the runtime can distinguish demand
// requests vs. warm-up requests and apply warm-up specific optimizations.
class WarmupStateRegistry {
 public:
  struct Key {
    std::string name;
    int64_t version;

    Key(std::string name, int64_t version)
        : name(std::move(name)), version(version) {}

    template <typename H>
    friend H AbslHashValue(H state, const Key& key) {
      return H::combine(std::move(state), key.name, key.version);
    }

    friend bool operator==(const Key& x, const Key& y) {
      return x.name == y.name && x.version == y.version;
    }
  };

  // RAII handle for registered models.
  class Handle {
   public:
    Handle() = default;

    Handle(const Handle& other) = delete;
    Handle& operator=(const Handle& other) = delete;
    Handle(Handle&& other)
        : key_(std::move(other.key_)), registry_(other.registry_) {
      other.key_.reset();
    }
    Handle& operator=(Handle&& other) {
      if (key_.has_value()) {
        Release();
      }

      key_ = std::move(other.key_);
      other.key_.reset();
      registry_ = other.registry_;
      return *this;
    }

    ~Handle() { Release(); }

    void Release();

   private:
    friend class WarmupStateRegistry;

    // Can only be constructed by `WarmupStateRegistry::Register()`.
    Handle(const Key& key, WarmupStateRegistry* registry)
        : key_(key), registry_(registry) {
      DCHECK(registry_);
    }

    std::optional<Key> key_;
    WarmupStateRegistry* registry_ = nullptr;
  };

  // Registers the given model to be in a warm-up state and associates the given
  // metadata with the model. Returns an RAII handle that unregisters the model
  // at its destruction.
  absl::StatusOr<Handle> Register(const Key& model_key);

  // Return true if the model is in a warm-up state.
  bool Lookup(const Key& model_key);

 private:
  friend class Handle;

  void Unregister(const Key& model_key);

  absl::Mutex mu_;
  // Mapping from model names/versions to executables.
  absl::flat_hash_set<Key> states_ ABSL_GUARDED_BY(&mu_);
};

WarmupStateRegistry& GetGlobalWarmupStateRegistry();

}  // namespace serving
}  // namespace tensorflow

#endif  // TENSORFLOW_CORE_KERNELS_BATCHING_UTIL_WARMUP_H_
