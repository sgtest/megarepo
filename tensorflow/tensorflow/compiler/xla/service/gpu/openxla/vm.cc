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

#include "tensorflow/compiler/xla/service/gpu/openxla/vm.h"

#include <vector>

namespace xla::gpu::vm {

using iree::StatusOr;

//===----------------------------------------------------------------------===//
// Helper functions to work with VM lists
//===----------------------------------------------------------------------===//

StatusOr<std::vector<int64_t>> GetI64Vector(const iree_vm_list_t* list) {
  iree_host_size_t size = iree_vm_list_size(list);
  std::vector<int64_t> values(size);
  for (iree_host_size_t i = 0; i < size; ++i) {
    iree_vm_value_t value;
    IREE_RETURN_IF_ERROR(
        iree_vm_list_get_value_as(list, i, IREE_VM_VALUE_TYPE_I64, &value));
    values[i] = value.i64;
  }
  return values;
}

}  // namespace xla::gpu::vm

//===----------------------------------------------------------------------===//
// Register types with IREE VM
//===----------------------------------------------------------------------===//

IREE_VM_DEFINE_TYPE_ADAPTERS(execution_context, xla::gpu::vm::ExecutionContext);
