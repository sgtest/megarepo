/* Copyright 2024 The OpenXLA Authors.

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

#include "xla/stream_executor/host/host_kernel.h"

#include <atomic>
#include <cstddef>
#include <cstdint>
#include <memory>
#include <utility>

#include "absl/base/optimization.h"
#include "absl/container/inlined_vector.h"
#include "absl/status/status.h"
#include "absl/types/span.h"
#include "xla/stream_executor/device_memory.h"
#include "xla/stream_executor/host/host_kernel_c_api.h"
#include "xla/stream_executor/launch_dim.h"
#include "xla/tsl/concurrency/async_value_ref.h"
#include "xla/tsl/concurrency/ref_count.h"
#include "tsl/platform/logging.h"
#include "tsl/platform/threadpool.h"

namespace stream_executor::host {

static absl::InlinedVector<SE_HOST_KernelArg, 8> ConvertBuffersToKernelArgs(
    absl::Span<const DeviceMemoryBase> buffers) {
  absl::InlinedVector<SE_HOST_KernelArg, 8> args(buffers.size());
  for (size_t i = 0; i < buffers.size(); ++i) {
    args[i].data = const_cast<void*>(buffers[i].opaque());
    args[i].size = buffers[i].size();
  }
  return args;
}

// Non-reference-counted async value ref for host kernels executed inline.
using CompletionEvent = HostKernel::CompletionEvent;
static tsl::AsyncValueRef<CompletionEvent> ReadyCompletionEvent() {
  static tsl::AsyncValueOwningRef<CompletionEvent>* event = [] {
    auto* storage = new tsl::internal::AsyncValueStorage<CompletionEvent>();
    return new tsl::AsyncValueOwningRef<CompletionEvent>(
        tsl::MakeAvailableAsyncValueRef<CompletionEvent>(*storage, 1ull));
  }();
  return event->AsRef();
}

namespace {
// Keep a state of an in-flight asynchronous kernel execution on a heap to keep
// it alive until the last task is done.
class HostKernelExecuteState
    : public tsl::ReferenceCounted<HostKernelExecuteState> {
 public:
  HostKernelExecuteState(HostKernel::TaskRunner task_runner,
                         HostKernel::KernelFunction* function,
                         ThreadDim thread_dims,
                         absl::Span<const DeviceMemoryBase> buffers);

  // Calls a task with index `task_index` synchronously.
  void CallSync(uint64_t task_index);

  // Calls tasks in the [start_index, end_index) range asynchronously using task
  // runner to schedule work. Executes a single task in the caller thread.
  void CallAsync(uint64_t start_index, uint64_t end_index);

  tsl::AsyncValueRef<CompletionEvent> completion_event() const {
    return event_;
  }

 private:
  // Converts linear task index in [0, num_tasks) to (x, y, z) coordinate. We
  // assume that `x` is the fastest iterating dimension.
  SE_HOST_KernelThread Delinearize(uint64_t task_index);

  HostKernel::TaskRunner task_runner_;
  size_t num_tasks_;

  SE_HOST_Kernel* kernel_;
  SE_HOST_KernelThreadDim thread_dims_;
  absl::InlinedVector<SE_HOST_KernelArg, 8> args_;

  std::atomic<int64_t> counter_;
  tsl::AsyncValueRef<CompletionEvent> event_;
};

}  // namespace

HostKernel::HostKernel(std::shared_ptr<tsl::thread::ThreadPool> thread_pool)
    : thread_pool_(thread_pool) {
  // Kernel and arity will be set separately
}

HostKernel::HostKernel(unsigned arity, SE_HOST_Kernel* kernel,
                       std::shared_ptr<tsl::thread::ThreadPool> thread_pool)
    : function_(std::make_unique<KernelFunctionPtr>(kernel)),
      arity_(arity),
      thread_pool_(thread_pool) {}

absl::Status HostKernel::Launch(
    const ThreadDim& thread_dims,
    absl::Span<const DeviceMemoryBase> buffers) const {
  SE_HOST_KernelThreadDim kernel_thread_dims = {thread_dims.x, thread_dims.y,
                                                thread_dims.z};

  SE_HOST_Kernel* kernel = function_->kernel();
  auto args = ConvertBuffersToKernelArgs(buffers);

  for (uint64_t z = 0; z < thread_dims.z; ++z) {
    for (uint64_t y = 0; y < thread_dims.y; ++y) {
      for (uint64_t x = 0; x < thread_dims.x; ++x) {
        SE_HOST_KernelThread kernel_thread = {x, y, z};

        SE_HOST_KernelCallFrame call_frame = {
            &kernel_thread_dims, &kernel_thread, args.size(), args.data()};

        SE_HOST_KernelError* error = (*kernel)(&call_frame);

        if (error != nullptr) {
          return absl::InternalError("Failed to call host kernel");
        }
      }
    }
  }

  return absl::OkStatus();
}

tsl::AsyncValueRef<CompletionEvent> HostKernel::Launch(
    const ThreadDim& thread_dims, absl::Span<const DeviceMemoryBase> buffers,
    TaskRunner task_runner) const {
  size_t num_tasks = thread_dims.x * thread_dims.y * thread_dims.z;
  DCHECK_GT(num_tasks, 0) << "Number of tasks must be positive";

  // Short-circuit launch with a single task and run it in the caller thread.
  if (ABSL_PREDICT_TRUE(num_tasks == 1)) {
    absl::Status launched = Launch(thread_dims, buffers);
    return ABSL_PREDICT_TRUE(launched.ok())
               ? ReadyCompletionEvent()
               : tsl::MakeErrorAsyncValueRef(std::move(launched));
  }

  // Allocate a control structure that will orchestrate kernel execution.
  auto state = tsl::MakeRef<HostKernelExecuteState>(
      std::move(task_runner), function_.get(), thread_dims, buffers);

  state->CallAsync(/*start_index=*/0, /*end_index=*/num_tasks);

  return state->completion_event();
}

HostKernelExecuteState::HostKernelExecuteState(
    HostKernel::TaskRunner task_runner, HostKernel::KernelFunction* function,
    ThreadDim thread_dims, absl::Span<const DeviceMemoryBase> buffers)
    : task_runner_(std::move(task_runner)),
      num_tasks_(thread_dims.x * thread_dims.y * thread_dims.z),
      kernel_(function->kernel()),
      thread_dims_({thread_dims.x, thread_dims.y, thread_dims.z}),
      args_(ConvertBuffersToKernelArgs(buffers)),
      counter_(num_tasks_),
      event_(tsl::MakeConstructedAsyncValueRef<CompletionEvent>(num_tasks_)) {}

void HostKernelExecuteState::CallSync(uint64_t task_index) {
  DCHECK_LT(task_index, num_tasks_) << "Task index out of range";

  SE_HOST_KernelThread kernel_thread = Delinearize(task_index);
  SE_HOST_KernelCallFrame call_frame = {&thread_dims_, &kernel_thread,
                                        args_.size(), args_.data()};

  SE_HOST_KernelError* error = (*kernel_)(&call_frame);
  CHECK(error == nullptr) << "Failed to call host kernel";

  // Maybe notify event of an execution completion.
  if (counter_.load(std::memory_order_relaxed) == 1 ||
      counter_.fetch_sub(1, std::memory_order_relaxed) == 1) {
    event_.SetStateConcrete();
  }
}

void HostKernelExecuteState::CallAsync(uint64_t start_index,
                                       uint64_t end_index) {
  CHECK_LT(start_index, end_index) << "Invalid task index range";
  while (end_index - start_index > 1) {
    uint64_t mid_index = (start_index + end_index) / 2;
    task_runner_([self = tsl::FormRef(this), mid_index, end_index] {
      self->CallAsync(mid_index, end_index);
    });
    end_index = mid_index;
  }
  CallSync(start_index);
}

SE_HOST_KernelThread HostKernelExecuteState::Delinearize(uint64_t task_index) {
  uint64_t stride_z = thread_dims_.y * thread_dims_.x;
  uint64_t stride_y = thread_dims_.x;

  uint64_t z = task_index / stride_z;
  task_index = task_index % stride_z;

  uint64_t y = task_index / stride_y;
  task_index = task_index % stride_y;

  uint64_t x = task_index;

  return SE_HOST_KernelThread{x, y, z};
}

}  // namespace stream_executor::host
