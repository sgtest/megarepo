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

#ifndef TENSORFLOW_COMPILER_XLA_SERVICE_GPU_OPENXLA_KERNEL_H_
#define TENSORFLOW_COMPILER_XLA_SERVICE_GPU_OPENXLA_KERNEL_H_

#include <cstdint>

#include "third_party/iree/runtime/src/iree/hal/api.h"  // IWYU pragma: keep
#include "third_party/iree/runtime/src/iree/vm/api.h"   // IWYU pragma: keep
#include "tensorflow/compiler/xla/service/gpu/launch_dimensions.h"
#include "tensorflow/compiler/xla/service/gpu/openxla/vm.h"

namespace xla::gpu {

namespace vm {

//===-----------------------------------------------------------------------===/
// XLA:GPU kernel API custom types
//===-----------------------------------------------------------------------===/

struct Kernel : public iree::vm::RefObject<Kernel> {
  std::string kernel_name;
  int32_t shared_memory_bytes;
};

}  // namespace vm

//===-----------------------------------------------------------------------===/
// XLA:GPU kernel API
//===-----------------------------------------------------------------------===/

Status DispatchKernel(const vm::ExecutionContext& ctx, const vm::Kernel& kernel,
                      iree_hal_allocator_t* device_allocator,
                      absl::Span<iree_hal_buffer_view_t*> args,
                      LaunchDimensions dims);

//===-----------------------------------------------------------------------===/
// XLA:GPU custom module kernel dispatch API
//===-----------------------------------------------------------------------===/

namespace vm {

class KernelAPI {
 public:
  explicit KernelAPI(iree_hal_allocator_t* device_allocator);

  iree::StatusOr<iree::vm::ref<Kernel>> KernelCreate(
      iree_string_view_t kernel_name, int32_t shared_memory_bytes);

  // Dispatches device kernel with given buffers and parameters.
  iree::Status KernelDispatch(iree::vm::ref<ExecutionContext> ctx,
                              iree::vm::ref<Kernel> kernel,
                              iree::vm::ref<iree_vm_list_t> args,
                              // Workgroup size (block size)
                              int32_t workgroup_size_x,
                              int32_t workgroup_size_y,
                              int32_t workgroup_size_z,
                              // Workload size (grid size)
                              int32_t workload_size_x, int32_t workload_size_y,
                              int32_t workload_size_z);

 private:
  iree_hal_allocator_t* device_allocator_;
};

}  // namespace vm
}  // namespace xla::gpu

//===----------------------------------------------------------------------===//
// Register types with IREE VM
//===----------------------------------------------------------------------===//

IREE_VM_DECLARE_TYPE_ADAPTERS(kernel, xla::gpu::vm::Kernel);

#endif  // TENSORFLOW_COMPILER_XLA_SERVICE_GPU_OPENXLA_KERNEL_H_
