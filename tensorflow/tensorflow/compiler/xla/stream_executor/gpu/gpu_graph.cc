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

#include "tensorflow/compiler/xla/stream_executor/gpu/gpu_graph.h"

#include <atomic>
#include <cstddef>
#include <cstdlib>
#include <string>

#include "tensorflow/compiler/xla/stream_executor/gpu/gpu_driver.h"
#include "tensorflow/compiler/xla/stream_executor/gpu/gpu_stream.h"
#include "tensorflow/tsl/platform/env.h"
#include "tensorflow/tsl/platform/errors.h"
#include "tensorflow/tsl/platform/path.h"

namespace stream_executor {
namespace gpu {

//===----------------------------------------------------------------------===//
// RAII helpers for gpu graph types.
//===----------------------------------------------------------------------===//

std::atomic<size_t> GpuGraphSupport::allocated_gpu_graph_execs_;
std::atomic<size_t> GpuGraphSupport::alive_gpu_graph_execs_;

/*static*/ size_t GpuGraphSupport::NotifyGraphExecCreated() {
  alive_gpu_graph_execs_.fetch_add(1, std::memory_order_relaxed);
  return allocated_gpu_graph_execs_.fetch_add(1, std::memory_order_relaxed);
}

/*static*/ size_t GpuGraphSupport::NotifyGraphExecDestroyed() {
  return alive_gpu_graph_execs_.fetch_sub(1, std::memory_order_relaxed) - 1;
}

/*static*/ size_t GpuGraphSupport::allocated_gpu_graph_execs() {
  return allocated_gpu_graph_execs_.load(std::memory_order_relaxed);
}

/*static*/ size_t GpuGraphSupport::alive_gpu_graph_execs() {
  return alive_gpu_graph_execs_.load(std::memory_order_relaxed);
}

void GpuGraphSupport::DestroyGraph::operator()(GpuGraphHandle graph) {
  auto st = GpuDriver::DestroyGraph(graph);
  CHECK(st.ok()) << "Failed to destroy gpu graph: " << st.message();
}

void GpuGraphSupport::DestroyGraphExec::operator()(GpuGraphExecHandle exec) {
  auto st = GpuDriver::DestroyGraphExec(exec);
  CHECK(st.ok()) << "Failed to destroy executable gpu graph: " << st.message();
}

tsl::Status OwnedGpuGraphExec::Update(OwnedGpuGraph graph) {
  VLOG(3) << "Update gpu graph exec with a new graph after " << num_launches_
          << " launches since last update"
          << " #" << num_updates_++;

  num_launches_ = 0;

  GpuDriver::GraphExecUpdateResultInfo result;
  auto st = GpuDriver::GraphExecUpdate(get(), graph.get(), &result);

  if (!st.ok() || result.result != GpuDriver::GraphExecUpdateResult::kSuccess) {
    return tsl::errors::Internal("Failed to update gpu graph: ", st.message());
  }

  return tsl::OkStatus();
}

tsl::Status OwnedGpuGraphExec::Launch(stream_executor::Stream* stream) {
  VLOG(3) << "Launch gpu graph " << get()
          << " on a stream: " << stream->DebugStreamPointers() << " #"
          << ++num_launches_;

  return GpuDriver::GraphLaunch(get(), AsGpuStreamValue(stream));
}

OwnedGpuGraphExec::~OwnedGpuGraphExec() {
  if (*this)  // do not log for moved-from instances
    VLOG(5) << "Destroy GPU graph exec #" << id_
            << " (remaining alive instances: "
            << GpuGraphSupport::NotifyGraphExecDestroyed() << ")";
}

//===----------------------------------------------------------------------===//
// GPU Graph Helpers.
//===----------------------------------------------------------------------===//

tsl::StatusOr<OwnedGpuGraph> CaptureGpuGraph(
    stream_executor::Stream* stream,
    absl::AnyInvocable<tsl::Status()> capture) {
  VLOG(3) << "Capture gpu graph on a stream: " << stream->DebugStreamPointers();

  GpuGraphHandle graph;

  // Get the underlying stream for passing to GPU runtime APIs.
  auto gpu_stream = AsGpuStreamValue(stream);

  // Capture graph constructed by the exported graph capture function.
  TF_RETURN_IF_ERROR(GpuDriver::StreamBeginCapture(
      gpu_stream, GpuDriver::StreamCaptureMode::kThreadLocal));

  // Call into graph capture function.
  auto captured = capture();

  // Always stop capturing the stream before checking `captured` result.
  TF_RETURN_IF_ERROR(GpuDriver::StreamEndCapture(gpu_stream, &graph));

  if (!captured.ok())
    return tsl::errors::Internal("failed to capture gpu graph: ",
                                 captured.message());

  VLOG(5) << "Captured XLA:GPU operations into the graph " << graph;

  if (const char* path = getenv("XLA_GPU_GRAPH_DEBUG_DIRECTORY"); path) {
    std::string file = tsl::io::JoinPath(std::string(path), "/gpu-graph-");

    if (tsl::Env::Default()->CreateUniqueFileName(&file, ".dot")) {
      VLOG(100) << "Print gpu graph " << graph
                << " debug dot file to: " << file;
      auto printed = GpuDriver::GraphDebugDotPrint(graph, file.c_str());
      printed.IgnoreError();  // warning will be printed by GpuDriver
    } else {
      LOG(WARNING) << "Cannot create unique filename, won't enable gpu "
                      "graph debugging";
    }
  }

  return OwnedGpuGraph(graph);
}

tsl::StatusOr<OwnedGpuGraphExec> InstantiateGpuGraph(OwnedGpuGraph graph) {
  GpuGraphExecHandle exec;

  GpuDriver::GraphInstantiateFlags flags;
  TF_RETURN_IF_ERROR(GpuDriver::GraphInstantiate(&exec, graph.get(), flags));

  size_t id = GpuGraphSupport::NotifyGraphExecCreated();
  VLOG(5) << "Instantiated gpu graph exec instance #" << id
          << " (alive instances: " << GpuGraphSupport::alive_gpu_graph_execs()
          << ")";
  return OwnedGpuGraphExec(id, exec);
}

tsl::StatusOr<bool> IsStreamCapturing(stream_executor::Stream* stream) {
  return GpuDriver::StreamIsCapturing(AsGpuStreamValue(stream));
}

}  // namespace gpu
}  // namespace stream_executor
