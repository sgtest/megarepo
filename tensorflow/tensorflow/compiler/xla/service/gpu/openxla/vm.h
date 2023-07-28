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

#ifndef TENSORFLOW_COMPILER_XLA_SERVICE_GPU_OPENXLA_VM_H_
#define TENSORFLOW_COMPILER_XLA_SERVICE_GPU_OPENXLA_VM_H_

#include <vector>

#include "third_party/iree/runtime/src/iree/vm/api.h"  // IWYU pragma: keep

namespace xla {

class DebugOptions;
class ServiceExecutableRunOptions;

namespace gpu::vm {

//===----------------------------------------------------------------------===//
// Execution context of a single XLA invocation
//===----------------------------------------------------------------------===//

// We use XLA:GPU execution context to pass XLA:GPU invocation details to all
// runtime APIs. For example through `run_options` pointer we get access to
// the current compute stream, stream borrower, parent executor, etc.
struct ExecutionContext : public iree::vm::RefObject<ExecutionContext> {
  ExecutionContext(const ServiceExecutableRunOptions* run_options,
                   const DebugOptions* debug_options)
      : run_options(run_options), debug_options(debug_options) {}

  const ServiceExecutableRunOptions* run_options;
  const DebugOptions* debug_options;
};

//===----------------------------------------------------------------------===//
// Helper functions to work with VM lists
//===----------------------------------------------------------------------===//

iree::StatusOr<std::vector<int64_t>> GetI64Vector(const iree_vm_list_t* list);

}  // namespace gpu::vm
}  // namespace xla

//===----------------------------------------------------------------------===//
// Register types with IREE VM
//===----------------------------------------------------------------------===//

IREE_VM_DECLARE_TYPE_ADAPTERS(execution_context,
                              xla::gpu::vm::ExecutionContext);

#endif  // TENSORFLOW_COMPILER_XLA_SERVICE_GPU_OPENXLA_VM_H_
