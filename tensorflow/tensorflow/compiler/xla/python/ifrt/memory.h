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

#ifndef TENSORFLOW_COMPILER_XLA_PYTHON_IFRT_MEMORY_H_
#define TENSORFLOW_COMPILER_XLA_PYTHON_IFRT_MEMORY_H_

#include <optional>
#include <string>

#include "absl/strings/string_view.h"
#include "tensorflow/compiler/xla/pjrt/pjrt_client.h"

namespace xla {
namespace ifrt {

// Short-term alias to reuse `xla::PjRtMemorySpace` without a separate abstract
// type.
using Memory = ::xla::PjRtMemorySpace;

// `MemoryKind` uniquely identifies a group of memory spaces with a
// platform-dependent string. When no specific memory kind is chosen, the
// platform should use the default memory kind for a platform's device that is
// being used.
class MemoryKind {
 public:
  // Creates `MemoryKind` with no memory kind chosen.
  MemoryKind() = default;

  // Creates `MemoryKind` from a platform-dependent identifier of a memory kind.
  // `MemoryKind` will be stable even after the string referenced by
  // `memory_kind` is deallocated.
  explicit MemoryKind(std::optional<absl::string_view> memory_kind);

  bool operator==(const MemoryKind& other) const {
    // Use a pointer comparison. *memory_kind_ always points to a deduplicated
    // string.
    if (!memory_kind_.has_value() && !other.memory_kind_.has_value()) {
      return true;
    }
    if (memory_kind_.has_value() && other.memory_kind_.has_value() &&
        memory_kind_->data() == other.memory_kind_->data()) {
      return true;
    }
    return false;
  }
  bool operator!=(const MemoryKind& other) const { return !(*this == other); }

  template <typename H>
  friend H AbslHashValue(H h, const MemoryKind& memory_kind) {
    return H::combine(std::move(h), memory_kind.memory_kind_);
  }

  // Returns a platform-dependent identifier of a memory kind.
  std::optional<absl::string_view> memory_kind() const { return memory_kind_; }

  std::string DebugString() const;

 private:
  std::optional<absl::string_view> memory_kind_;
};

}  // namespace ifrt
}  // namespace xla

#endif  // TENSORFLOW_COMPILER_XLA_PYTHON_IFRT_MEMORY_H_
