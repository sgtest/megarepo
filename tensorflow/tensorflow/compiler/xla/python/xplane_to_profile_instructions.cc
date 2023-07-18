/* Copyright 2017 The TensorFlow Authors All Rights Reserved.

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
#include "tensorflow/compiler/xla/python/xplane_to_profile_instructions.h"

#include <cstdint>
#include <memory>
#include <numeric>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include "absl/container/flat_hash_map.h"
#include "absl/status/status.h"
#include "absl/strings/match.h"
#include "absl/strings/str_cat.h"
#include "absl/types/optional.h"
#include "tensorflow/compiler/xla/hlo/ir/hlo_module.h"
#include "tensorflow/compiler/xla/service/hlo.pb.h"
#include "tensorflow/compiler/xla/status.h"
#include "tensorflow/compiler/xla/xla.pb.h"
#include "tensorflow/tsl/platform/env.h"
#include "tensorflow/tsl/platform/types.h"
#include "tensorflow/tsl/profiler/protobuf/xplane.pb.h"
#include "tensorflow/tsl/profiler/utils/file_system_utils.h"
#include "tensorflow/tsl/profiler/utils/tf_xplane_visitor.h"
#include "tensorflow/tsl/profiler/utils/xplane_schema.h"
#include "tensorflow/tsl/profiler/utils/xplane_utils.h"
#include "tensorflow/tsl/profiler/utils/xplane_visitor.h"

namespace xla {
namespace {

constexpr char kXPlanePb[] = "xplane.pb";
constexpr char kCostNameSep[] = "::";

using tensorflow::profiler::XPlane;
using tensorflow::profiler::XSpace;
using tsl::profiler::CreateTfXPlaneVisitor;
using tsl::profiler::FindPlanesWithPrefix;
using tsl::profiler::FindPlaneWithName;
using tsl::profiler::GetStatTypeStr;
using tsl::profiler::HostEventType;
using tsl::profiler::IsInternalEvent;
using tsl::profiler::IsInternalStat;
using tsl::profiler::ProfilerJoinPath;
using tsl::profiler::StatType;
using tsl::profiler::XEventMetadataVisitor;
using tsl::profiler::XEventVisitor;
using tsl::profiler::XLineVisitor;
using tsl::profiler::XPlaneVisitor;
using tsl::profiler::XStatVisitor;

void GetXPlaneLatencyInfo(
    const XPlaneVisitor& xplane,
    const absl::flat_hash_map<std::string, std::string>& hlo_module_info,
    absl::flat_hash_map<std::string, HloLatencyInfo>* hlo_latency_info) {
  // Iterate events.
  xplane.ForEachLine([hlo_latency_info,
                      hlo_module_info](const XLineVisitor& xline) {
    if (xline.DisplayName() == tsl::profiler::kXlaAsyncOpLineName) {
      return;
    }
    xline.ForEachEvent([hlo_latency_info,
                        hlo_module_info](const XEventVisitor& xevent) {
      int64_t event_type =
          xevent.Type().value_or(HostEventType::kUnknownHostEventType);
      if (IsInternalEvent(event_type)) return;
      std::optional<std::string> hlo_name = std::nullopt;
      std::optional<std::string> hlo_module_name = std::nullopt;
      std::optional<std::string> fingerprint = std::nullopt;

      auto for_each_stat = [&](const XStatVisitor& stat) {
        if (stat.ValueCase() == tsl::profiler::XStat::VALUE_NOT_SET) return;
        if (IsInternalStat(stat.Type())) return;
        // Store latency information for HLOs.
        if (stat.Name() == GetStatTypeStr(StatType::kHloOp)) {
          hlo_name = stat.ToString();
        }
        if (stat.Name() == GetStatTypeStr(StatType::kHloModule)) {
          hlo_module_name = stat.ToString();
          if (hlo_module_info.contains(hlo_module_name.value())) {
            fingerprint = hlo_module_info.at(hlo_module_name.value());
          }
        }
      };
      xevent.Metadata().ForEachStat(for_each_stat);
      xevent.ForEachStat(for_each_stat);
      if (!hlo_name.has_value() || !hlo_module_name.has_value()) {
        return;
      }
      double latency = static_cast<double>(xevent.DurationNs()) / 1e3;
      std::string key = hlo_name.value();
      if (fingerprint.has_value()) {
        key = absl::StrCat(fingerprint.value(), kCostNameSep, hlo_name.value());
      }
      (*hlo_latency_info)[key].durations.emplace_back(latency);
    });
  });
}

std::unique_ptr<xla::HloModule> CreateModuleFromProto(
    const xla::HloModuleProto& proto) {
  auto config = xla::HloModule::CreateModuleConfigFromProto(proto, {});
  if (config.ok()) {
    auto module = xla::HloModule::CreateFromProto(proto, config.value());
    if (module.ok()) {
      return std::move(*module);
    }
  }
  return nullptr;
}

std::optional<std::string> GetHloModuleFingerprint(
    const xla::HloModuleProto& hlo_module_proto) {
  std::unique_ptr<xla::HloModule> hlo_module =
      CreateModuleFromProto(hlo_module_proto);
  if (hlo_module == nullptr) {
    return std::nullopt;
  }
  const auto& map = hlo_module->entry_computation()
                        ->root_instruction()
                        ->frontend_attributes()
                        .map();
  auto it = map.find("fingerprint_before_lhs");
  if (it != map.end()) {
    return it->second;
  }
  return std::nullopt;
}

void GetXPlaneHloModuleInfo(
    const XPlaneVisitor& xplane,
    absl::flat_hash_map<std::string, std::string>* hlo_module_info) {
  // Iterate events.
  xplane.ForEachEventMetadata([&](const XEventMetadataVisitor& event_metadata) {
    event_metadata.ForEachStat([&](const XStatVisitor& stat) {
      xla::HloProto hlo_proto;
      if (tsl::ParseProtoUnlimited(&hlo_proto, stat.BytesValue().data(),
                                   stat.BytesValue().size())) {
        const xla::HloModuleProto& hlo_module_proto = hlo_proto.hlo_module();

        std::optional<std::string> fingerprint =
            GetHloModuleFingerprint(hlo_module_proto);
        if (fingerprint.has_value()) {
          (*hlo_module_info)[hlo_module_proto.name()] = fingerprint.value();
        }
      }
    });
  });
}

}  // namespace

Status ConvertXplaneToProfiledInstructionsProto(
    const std::string& logdir, tensorflow::profiler::ProfiledInstructionsProto*
                                   profiled_instructions_proto) {
  // Find the xplane files for each host under logdir.
  std::vector<std::string> children_path;
  TF_RETURN_IF_ERROR(tsl::Env::Default()->GetChildren(logdir, &children_path));
  if (children_path.empty()) {
    return absl::NotFoundError(
        absl::StrCat("Could not find file under: ", logdir));
  }
  std::vector<tensorflow::profiler::XSpace> xspaces;
  for (const std::string& child_path : children_path) {
    if (absl::StrContains(child_path, kXPlanePb)) {
      std::string xspace_path = ProfilerJoinPath(logdir, child_path);
      tensorflow::profiler::XSpace xspace;
      TF_RETURN_IF_ERROR(
          ReadBinaryProto(tsl::Env::Default(), xspace_path, &xspace));
      xspaces.emplace_back(xspace);
    }
  }

  // Gets the duration information for each hlo.
  absl::flat_hash_map<std::string, HloLatencyInfo> hlo_latency_info;
  absl::flat_hash_map<std::string, std::string> hlo_module_info;
  // Iterate through each host.
  for (const XSpace& xspace : xspaces) {
    const XPlane* metadata_plane =
        FindPlaneWithName(xspace, tsl::profiler::kMetadataPlaneName);
    if (metadata_plane != nullptr) {
      XPlaneVisitor xplane = CreateTfXPlaneVisitor(metadata_plane);
      GetXPlaneHloModuleInfo(xplane, &hlo_module_info);
    }
    std::vector<const XPlane*> device_planes =
        FindPlanesWithPrefix(xspace, tsl::profiler::kGpuPlanePrefix);
    // We don't expect GPU and TPU planes and custom devices to be present in
    // the same XSpace.
    if (device_planes.empty()) {
      device_planes =
          FindPlanesWithPrefix(xspace, tsl::profiler::kTpuPlanePrefix);
    }
    if (device_planes.empty()) {
      device_planes =
          FindPlanesWithPrefix(xspace, tsl::profiler::kCustomPlanePrefix);
    }
    // Go over each device plane.
    for (const XPlane* device_plane : device_planes) {
      XPlaneVisitor xplane = CreateTfXPlaneVisitor(device_plane);
      GetXPlaneLatencyInfo(xplane, hlo_module_info, &hlo_latency_info);
    }
  }

  // Get the mean duration for each hlo and store into the proto.
  for (const auto& iter : hlo_latency_info) {
    auto* cost = profiled_instructions_proto->add_costs();
    std::vector<double> durations = iter.second.durations;
    double sum = std::accumulate(durations.begin(), durations.end(), 0.0);
    cost->set_cost_us(sum / durations.size());
    cost->set_name(iter.first);
  }

  return OkStatus();
}

}  // namespace xla
