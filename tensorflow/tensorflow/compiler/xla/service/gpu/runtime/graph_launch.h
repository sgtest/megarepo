/* Copyright 2022 The TensorFlow Authors. All Rights Reserved.

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

#ifndef TENSORFLOW_COMPILER_XLA_SERVICE_GPU_RUNTIME_GRAPH_LAUNCH_H_
#define TENSORFLOW_COMPILER_XLA_SERVICE_GPU_RUNTIME_GRAPH_LAUNCH_H_

#include <atomic>
#include <memory>
#include <utility>

#include "absl/container/flat_hash_set.h"
#include "absl/container/node_hash_map.h"
#include "tensorflow/compiler/xla/runtime/custom_call_registry.h"
#include "tensorflow/compiler/xla/runtime/executable.h"
#include "tensorflow/compiler/xla/service/service_executable_run_options.h"
#include "tensorflow/compiler/xla/stream_executor/stream_executor.h"
#include "tensorflow/compiler/xla/stream_executor/stream_executor_pimpl.h"

#if GOOGLE_CUDA
#include "tensorflow/compiler/xla/stream_executor/cuda/cuda_graph.h"
#endif  // #if GOOGLE_CUDA

namespace xla {
namespace gpu {

// Registers XLA Gpu runtime graph launch custom calls.
void RegisterGraphLaunchCustomCalls(
    runtime::DirectCustomCallRegistry& registry);

struct GraphInstance;                // Forward declare
class StreamExecutorGraphInstances;  // Forward declare

// A state vector that keeps track of the number of times a capture function
// gets executed. Graph capture function ordinal is the key in this container.
class CapturedFunctionExecutionCount
    : public runtime::StateVector<std::unique_ptr<std::atomic<uint64_t>>> {};

#if GOOGLE_CUDA

// A state vector that owns all instantiated CUDA graphs. Graph capture function
// ordinal is the key in this container.
class StreamExecutorGraphInstances
    : public runtime::StateVector<GraphInstance> {};

// Instantiated CUDA graph instance guarded with a mutex for exclusive access.
struct GraphInstance {
  GraphInstance(size_t ptr_hash, se::gpu::OwnedCudaGraphExec exec)
      : ptr_hash(ptr_hash), exec(std::move(exec)), mutex(new absl::Mutex) {}

  // Graph instance is fully identified by the hash of its pointer arguments
  // because currently it's guaranteed that all shapes and launch dimensions
  // will be constant from run to run.
  size_t ptr_hash ABSL_GUARDED_BY(*mutex);
  se::gpu::OwnedCudaGraphExec exec ABSL_GUARDED_BY(*mutex);

  // Access to a graph instance must be synchronized, because we potentially can
  // run concurrent graph instance updates.
  std::unique_ptr<absl::Mutex> mutex;
};

#else  // #if !GOOGLE_CUDA

// Define empty struct and empty state when CUDA is not enabled.
struct GraphInstance {};
class StreamExecutorGraphInstances
    : public runtime::StateVector<GraphInstance> {};

#endif  // #if GOOGLE_CUDA

// Xla executable keeps a mapping from stream executors to graph instances.
class GraphInstances {
 public:
  StreamExecutorGraphInstances* operator()(se::StreamExecutor* executor);

  // Instantiates all Gpu graphs defined by the given executable using user
  // provided run options. This guarantees that once we start execution, all Gpu
  // graphs are ready, and will only require cheap update operation and will not
  // require allocating new resources (we avoid non deterministic OOM errors).
  Status InstantiateAllGraphs(const ServiceExecutableRunOptions* run_options,
                              const runtime::Executable& executable,
                              const runtime::CustomCall::UserData& user_data,
                              void* ptr);

 private:
  mutable absl::Mutex mutex_;
  absl::node_hash_map<se::StreamExecutor*, StreamExecutorGraphInstances> graphs_
      ABSL_GUARDED_BY(mutex_);
  absl::flat_hash_set<se::StreamExecutor*> instantiated_
      ABSL_GUARDED_BY(mutex_);
};

// Xla executable keeps a mapping from stream executors to execution counts.
class CapturedFunctionExecutionCounts {
 public:
  CapturedFunctionExecutionCount* operator()(se::StreamExecutor* executor);

 private:
  mutable absl::Mutex mutex_;
  absl::node_hash_map<se::StreamExecutor*, CapturedFunctionExecutionCount>
      counts_ ABSL_GUARDED_BY(mutex_);
};

}  // namespace gpu
}  // namespace xla

#endif  // TENSORFLOW_COMPILER_XLA_SERVICE_GPU_RUNTIME_GRAPH_LAUNCH_H_
