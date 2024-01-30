/* Copyright 2017 The OpenXLA Authors.

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

#include "xla/service/gpu/thunk.h"

#include <cstdint>
#include <functional>
#include <memory>
#include <ostream>
#include <string>
#include <utility>

#include "absl/algorithm/container.h"
#include "absl/container/inlined_vector.h"
#include "absl/status/status.h"
#include "absl/status/statusor.h"
#include "absl/strings/str_cat.h"
#include "absl/strings/str_format.h"
#include "absl/strings/string_view.h"
#include "absl/types/span.h"
#include "xla/executable_run_options.h"
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/service/global_device_id.h"
#include "xla/service/gpu/buffer_allocations.h"
#include "xla/service/gpu/gpu_executable_run_options.h"
#include "xla/service/gpu/nccl_clique.h"
#include "xla/service/gpu/nccl_clique_key.h"
#include "xla/service/service_executable_run_options.h"
#include "xla/stream_executor/stream.h"
#include "xla/translate/mhlo_to_hlo/location_exporter.h"
#include "tsl/platform/statusor.h"

namespace xla {
namespace gpu {

//===----------------------------------------------------------------------===//
// Thunk::CollectiveCliques
//===----------------------------------------------------------------------===//

Thunk::CollectiveCliques::CollectiveCliques(CliquesMap cliques_map)
    : cliques_map_(std::move(cliques_map)) {}

absl::StatusOr<NcclComm::Lock> Thunk::CollectiveCliques::GetComm(
    const NcclCliqueKey& clique_key, int32_t rank) const {
  // Check that we locked access to a clique for `clique_key`.
  auto clique = cliques_map_.find(clique_key);
  if (clique == cliques_map_.end()) {
    return absl::NotFoundError(absl::StrCat("No clique found for clique key: ",
                                            clique_key.ToString()));
  }

  // Check that clique has a communicator for our rank.
  auto communicator = (*clique->second)->comm(rank);
  if (!communicator.has_value()) {
    return absl::InternalError(absl::StrCat("Communicator for rank ", rank,
                                            " not found in a NCCL clique ",
                                            clique_key.ToString()));
  }

  return (*communicator)->Acquire();
}

//===----------------------------------------------------------------------===//
// Thunk::CollectiveExecuteParams
//===----------------------------------------------------------------------===//

using GlobalDeviceIdMap = Thunk::CollectiveExecuteParams::GlobalDeviceIdMap;

// Returns global device id for a local device ordinal or an error if global
// device id map is misconfigured and missing an entry for a local device.
static absl::StatusOr<GlobalDeviceId> GetGlobalDeviceId(
    const GlobalDeviceIdMap* device_id_map, int64_t local_device_ordinal) {
  // No local -> global mapping was provided; assume the identity mapping.
  if (!device_id_map) return GlobalDeviceId(local_device_ordinal);

  // Find a global device id in a global device id map.
  auto it = device_id_map->find(local_device_ordinal);
  if (it == device_id_map->end())
    return absl::NotFoundError(
        absl::StrCat("No global device id found for local device ordinal: ",
                     local_device_ordinal));

  return it->second;
}

absl::StatusOr<Thunk::CollectiveExecuteParams>
Thunk::CollectiveExecuteParams::Create(
    const ServiceExecutableRunOptions& run_options,
    int64_t local_device_ordinal) {
  const GpuExecutableRunOptions* gpu_options =
      run_options.run_options().gpu_executable_run_options();

  auto* device_id_map = gpu_options && gpu_options->gpu_global_device_ids()
                            ? &*gpu_options->gpu_global_device_ids()
                            : nullptr;

  auto* nccl_callback = gpu_options && gpu_options->nccl_clique_id_callback()
                            ? &gpu_options->nccl_clique_id_callback()
                            : nullptr;

  TF_ASSIGN_OR_RETURN(GlobalDeviceId global_device_id,
                      GetGlobalDeviceId(device_id_map, local_device_ordinal));

  return CollectiveExecuteParams(run_options.run_options().run_id(),
                                 local_device_ordinal, global_device_id,
                                 run_options.run_options().device_assignment(),
                                 device_id_map, nccl_callback);
}

Thunk::CollectiveExecuteParams::CollectiveExecuteParams(
    RunId run_id, int64_t local_device_ordinal, GlobalDeviceId global_device_id,
    const DeviceAssignment* device_assn,
    const GlobalDeviceIdMap* global_device_id_map,
    const NcclCliqueIdCallback* nccl_clique_id_callback)
    : run_id(run_id),
      local_device_ordinal(local_device_ordinal),
      global_device_id(global_device_id),
      device_assn(device_assn),
      global_device_id_map(global_device_id_map),
      nccl_clique_id_callback(nccl_clique_id_callback) {}

//===----------------------------------------------------------------------===//
// Thunk::ExecuteParams
//===----------------------------------------------------------------------===//

Thunk::ExecuteParams Thunk::ExecuteParams::Create(
    const ServiceExecutableRunOptions& run_options,
    const BufferAllocations& buffer_allocations, se::Stream* stream,
    se::Stream* command_buffer_trace_stream,
    absl::Span<se::Stream* const> async_streams,
    CollectiveExecuteParams* collective_params,
    CollectiveCliques* collective_cliques) {
  return ExecuteParams(&buffer_allocations, stream, command_buffer_trace_stream,
                       {async_streams.begin(), async_streams.end()},
                       collective_params, collective_cliques,
                       run_options.run_options().device_to_host_stream(),
                       run_options.run_options().host_to_device_stream(),
                       run_options.run_options().send_device_memory_function(),
                       run_options.run_options().recv_device_memory_function());
}

Thunk::ExecuteParams::ExecuteParams(
    const BufferAllocations* buffer_allocations, se::Stream* stream,
    se::Stream* command_buffer_trace_stream,
    absl::InlinedVector<se::Stream*, 4> async_comms_streams,
    CollectiveExecuteParams* collective_params,
    CollectiveCliques* collective_cliques, se::Stream* device_to_host_stream,
    se::Stream* host_to_device_stream,
    SendDeviceMemoryFunction* send_device_memory_function,
    RecvDeviceMemoryFunction* recv_device_memory_function)
    : buffer_allocations(buffer_allocations),
      stream(stream),
      command_buffer_trace_stream(command_buffer_trace_stream),
      async_comms_streams(async_comms_streams),
      collective_params(collective_params),
      collective_cliques(collective_cliques),
      device_to_host_stream(device_to_host_stream),
      host_to_device_stream(host_to_device_stream),
      send_device_memory_function(send_device_memory_function),
      recv_device_memory_function(recv_device_memory_function) {}

//===----------------------------------------------------------------------===//

/*static*/ absl::string_view Thunk::KindToString(Thunk::Kind kind) {
#define CASE(x)  \
  case Thunk::x: \
    return #x
  switch (kind) {
    CASE(kCholesky);
    CASE(kCommandBuffer);
    CASE(kConditional);
    CASE(kConvolution);
    CASE(kConvolutionReorder);
    CASE(kCopy);
    CASE(kCubSort);
    CASE(kCublasLtMatmul);
    CASE(kCustomCall);
    CASE(kCustomKernel);
    CASE(kNcclAllGather);
    CASE(kNcclAllGatherStart);
    CASE(kNcclAllGatherDone);
    CASE(kNcclAllReduce);
    CASE(kNcclAllReduceStart);
    CASE(kNcclAllReduceDone);
    CASE(kNcclCollectivePermute);
    CASE(kNcclCollectivePermuteStart);
    CASE(kNcclCollectivePermuteDone);
    CASE(kNcclReduceScatter);
    CASE(kNcclReduceScatterStart);
    CASE(kNcclReduceScatterDone);
    CASE(kNcclAllToAll);
    CASE(kNcclAllToAllStart);
    CASE(kNcclAllToAllDone);
    CASE(kNcclSend);
    CASE(kNcclSendDone);
    CASE(kNcclRecv);
    CASE(kNcclRecvDone);
    CASE(kFft);
    CASE(kGemm);
    CASE(kInfeed);
    CASE(kKernel);
    CASE(kMemset32BitValue);
    CASE(kMemzero);
    CASE(kNorm);
    CASE(kOutfeed);
    CASE(kSend);
    CASE(kSendDone);
    CASE(kPartitionId);
    CASE(kReplicaId);
    CASE(kRecv);
    CASE(kRecvDone);
    CASE(kSequential);
    CASE(kTriangularSolve);
    CASE(kWhile);
    CASE(kFusedMHA);
  }
}

std::ostream& operator<<(std::ostream& os, Thunk::Kind kind) {
  return os << Thunk::KindToString(kind);
}

std::string ThunkSequence::ToString(
    int indent,
    std::function<std::string(const Thunk*)> get_thunk_annotation) const {
  const std::string indent_str(indent * 2, ' ');
  if (empty()) return indent_str + "No thunks.";

  auto thunk_with_longest_kind = absl::c_max_element(
      *this,
      [](const std::unique_ptr<Thunk>& a, const std::unique_ptr<Thunk>& b) {
        return Thunk::KindToString(a->kind()).length() <
               Thunk::KindToString(b->kind()).length();
      });
  int64_t max_thunk_kind_len =
      Thunk::KindToString(thunk_with_longest_kind->get()->kind()).length();
  std::string result;
  for (const std::unique_ptr<Thunk>& thunk : *this) {
    // Write out the thunk kind, padded out to max_thunk_kind_len.
    absl::string_view kind_str = Thunk::KindToString(thunk->kind());
    absl::StrAppend(&result, indent_str, kind_str,
                    std::string(max_thunk_kind_len - kind_str.length(), ' '),
                    "\t");
    if (get_thunk_annotation) {
      absl::StrAppend(&result, get_thunk_annotation(thunk.get()));
    }
    absl::StrAppend(&result, thunk->ToStringExtra(indent));
    absl::StrAppend(&result, "\n");
  }
  return result;
}

bool IsReductionCollective(Thunk::Kind kind) {
  return kind == Thunk::kNcclAllReduce || kind == Thunk::kNcclAllReduceStart ||
         kind == Thunk::kNcclReduceScatter ||
         kind == Thunk::kNcclReduceScatterStart;
}

Thunk::ThunkInfo Thunk::ThunkInfo::WithProfileAnnotation(mlir::Operation* op) {
  ThunkInfo thunk_info(op);
  thunk_info.profile_annotation = absl::StrFormat(
      "Thunk:#hlo_op=%s#", mlir::mhlo::GetDebugNameFromLocation(op->getLoc()));
  return thunk_info;
}

Thunk::ThunkInfo Thunk::ThunkInfo::WithProfileAnnotation(
    const HloInstruction* instr) {
  ThunkInfo thunk_info(nullptr);
  thunk_info.profile_annotation =
      absl::StrFormat("Thunk:#hlo_op=%s#", instr->name());
  return thunk_info;
}

}  // namespace gpu
}  // namespace xla
