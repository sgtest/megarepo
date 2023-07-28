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

#include "tensorflow/compiler/xla/service/gpu/openxla/kernel.h"

#include "absl/container/inlined_vector.h"
#include "tensorflow/compiler/xla/service/gpu/openxla/hal.h"
#include "tensorflow/compiler/xla/service/gpu/openxla/vm.h"
#include "tensorflow/compiler/xla/service/gpu/stream_executor_util.h"
#include "tensorflow/compiler/xla/service/service_executable_run_options.h"
#include "tensorflow/compiler/xla/stream_executor/device_memory.h"

namespace xla::gpu {

//===-----------------------------------------------------------------------===/
// XLA:GPU kernel dispatch API
//===-----------------------------------------------------------------------===/

Status DispatchKernel(const vm::ExecutionContext& ctx, const vm::Kernel& kernel,
                      iree_hal_allocator_t* device_allocator,
                      absl::Span<iree_hal_buffer_view_t*> args,
                      LaunchDimensions dims) {
  se::Stream* stream = ctx.run_options->stream();
  se::StreamExecutor* executor = stream->parent();

  // TODO(ezhulenev): Keep a cache of loaded kernels for each executor.
  TF_ASSIGN_OR_RETURN(
      std::unique_ptr<se::KernelBase> kernel_base,
      CreateKernel(kernel.kernel_name, args.size(), ctx.executable_source.ptx,
                   ctx.executable_source.cubin, executor,
                   kernel.shared_memory_bytes));

  absl::InlinedVector<se::DeviceMemoryBase, 8> device_args;
  for (iree_hal_buffer_view_t* arg : args) {
    TF_ASSIGN_OR_RETURN(device_args.emplace_back(),
                        GetDeviceMemory(device_allocator, arg));
  }

  return ExecuteKernelOnStream(*kernel_base, device_args, dims, stream);
}

//===-----------------------------------------------------------------------===/
// XLA:GPU custom module kernel dispatch API
//===-----------------------------------------------------------------------===/

namespace vm {

// TODO(ezhulenev): We need to find a way to pass original Status back to the
// caller preserving the location and stack frame. Can we use some diagnostic
// side channel via the ExecutionContext?
static iree::Status FromStatus(Status status) {
  if (status.ok()) return iree_ok_status();

  // TODO(ezhulenev): Convert from ABSL to IREE error code.
  std::string err = status.ToString();
  return iree_make_status(IREE_STATUS_INTERNAL, "internal error: %s",
                          err.c_str());
}

KernelAPI::KernelAPI(iree_hal_allocator_t* device_allocator)
    : device_allocator_(device_allocator) {}

iree::StatusOr<iree::vm::ref<Kernel>> KernelAPI::KernelCreate(
    iree_string_view_t kernel_name, int32_t shared_memory_bytes) {
  auto ref = iree::vm::make_ref<Kernel>();
  ref->kernel_name = std::string(kernel_name.data, kernel_name.size);
  ref->shared_memory_bytes = shared_memory_bytes;
  return ref;
}

iree::Status KernelAPI::KernelDispatch(
    iree::vm::ref<ExecutionContext> ctx, iree::vm::ref<Kernel> kernel,
    iree::vm::ref<iree_vm_list_t> args, int32_t workgroup_size_x,
    int32_t workgroup_size_y, int32_t workgroup_size_z, int32_t workload_size_x,
    int32_t workload_size_y, int32_t workload_size_z) {
  // Kernel launch dimensions + shared memory requirement.
  LaunchDimensions launch_dimensions(
      {workload_size_x, workload_size_y, workload_size_z},
      {workgroup_size_x, workgroup_size_y, workgroup_size_z});
  launch_dimensions.SetSharedMemBytes(kernel->shared_memory_bytes);

  IREE_ASSIGN_OR_RETURN(auto buffer_views, GetBufferViewVector(args.get()));
  return FromStatus(DispatchKernel(*ctx, *kernel, device_allocator_,
                                   {buffer_views.data(), buffer_views.size()},
                                   launch_dimensions));
}

}  // namespace vm
}  // namespace xla::gpu

//===----------------------------------------------------------------------===//
// Register types with IREE VM
//===----------------------------------------------------------------------===//

IREE_VM_DEFINE_TYPE_ADAPTERS(kernel, xla::gpu::vm::Kernel);
