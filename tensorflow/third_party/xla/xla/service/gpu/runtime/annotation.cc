/* Copyright 2023 The OpenXLA Authors.

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

#include "xla/service/gpu/runtime/annotation.h"

#include <cstddef>
#include <cstring>
#include <optional>
#include <string>
#include <string_view>
#include <utility>

#include "absl/status/status.h"
#include "absl/strings/str_format.h"
#include "absl/strings/str_split.h"
#include "xla/hlo/ir/dfs_hlo_visitor_with_default.h"
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/hlo/ir/hlo_opcode.h"
#include "tsl/platform/errors.h"
#include "tsl/profiler/lib/nvtx_utils.h"

namespace xla::gpu {
namespace {

nvtxStringHandle_t RegisterString(const std::string& str) {
#if GOOGLE_CUDA
  auto domain = tsl::profiler::GetNVTXDomain();
  if (!domain) {
    return {};  // NVTX not enabled, so don't registering strings.
  }
  constexpr auto max_length = 65330;
  if (str.size() <= max_length) {
    return nvtxDomainRegisterStringA(*domain, str.c_str());
  }
  // nvbugs 4340868
  std::string_view suffix{"\n[truncated]\n"};
  std::string buffer(str.data(), max_length - suffix.size());
  buffer.append(suffix);
  return nvtxDomainRegisterStringA(*domain, buffer.c_str());
#else
  return {};
#endif
}

template <typename Visitor>
absl::Status VisitInstAndCalledButNotOperands(Visitor& visitor,
                                              const HloInstruction& inst) {
  // Visit the given instruction, and the things it calls, but not its operands.
  TF_RETURN_IF_ERROR(visitor.DefaultAction(&inst));
  for (const HloComputation* called : inst.called_computations()) {
    const HloInstruction* const root = called->root_instruction();
    TF_RETURN_IF_ERROR(root->Accept(&visitor, false /* call_finish_visit */,
                                    true /* ignore_control_predecessors */,
                                    true /* cross_computation */));
  }
  return absl::OkStatus();
}

// Split `a` and `b` by `delim` into two lists of possibly-empty tokens, then
// rejoin the first N of those lists that match by `delim`. Note: it is
// unspecified which argument the return value points into.
std::string_view LongestPrefix(std::string_view a, std::string_view b,
                               char delim = '/') {
  auto split_a = absl::StrSplit(a, delim);
  auto split_b = absl::StrSplit(b, delim);

  size_t common_prefix_len = 0;

  for (auto a_it = split_a.begin(), b_it = split_b.begin();
       a_it != split_a.end() && b_it != split_b.end(); ++a_it, ++b_it) {
    if (*a_it != *b_it) break;

    if (common_prefix_len) ++common_prefix_len;  // account for delimiter
    common_prefix_len += a_it->size();           // length of a matching token
  }

  return std::string_view(a.data(), common_prefix_len);
}

// Find the longest prefix among instructions' op_name metadata
// Chunk this by delimiting slashes, i.e. given a/b/cat and a/b/cabbage, the
// longest prefix is a/b not a/b/ca
class OpNamePrefixVisitor : public ConstDfsHloVisitorWithDefault {
 public:
  absl::Status DefaultAction(const HloInstruction* inst) final {
    auto const& op_name = inst->metadata().op_name();
    if (!op_name.empty()) {
      prefix_ = prefix_ ? LongestPrefix(*prefix_, op_name) : op_name;
    }
    return absl::OkStatus();
  }

  std::string_view longest_op_name_prefix() const {
    return prefix_.value_or("");
  }

 private:
  std::optional<std::string_view> prefix_;
};

std::string_view GetLongestOpNamePrefix(const HloModule& mod) {
  // In the presence of (at least) debug callbacks, calling Accept on the root
  // instruction of the module may not reach all instructions in the module.
  OpNamePrefixVisitor visitor{};
  for (const HloComputation* computation : mod.computations()) {
    for (const HloInstruction* inst : computation->instructions()) {
      if (!visitor.DefaultAction(inst).ok()) {
        return {};
      }
    }
  }
  return visitor.longest_op_name_prefix();
}

std::string_view GetLongestOpNamePrefix(const HloInstruction& inst) {
  OpNamePrefixVisitor visitor{};
  if (!VisitInstAndCalledButNotOperands(visitor, inst).ok()) {
    return {};
  }
  return visitor.longest_op_name_prefix();
}

std::string MakeTitle(const HloModule& mod, std::string_view longest_prefix) {
  if (longest_prefix.empty()) {
    return absl::StrFormat("XlaModule:#hlo_module=%s,program_id=%d#",
                           mod.name(), mod.unique_id());
  }
  return absl::StrFormat("XlaModule:#prefix=%s,hlo_module=%s,program_id=%d#",
                         longest_prefix, mod.name(), mod.unique_id());
}
}  // namespace

ModuleAnnotation::ModuleAnnotation(std::string_view module_name)
    : title_str(absl::StrFormat("XlaModule:#hlo_module=%s", module_name)),
      title(RegisterString(title_str)) {}

ModuleAnnotation::ModuleAnnotation(const HloModule& mod)
    : longest_prefix(GetLongestOpNamePrefix(mod)),
      title_str(MakeTitle(mod, longest_prefix)),
      title(RegisterString(title_str)) {}

namespace {
std::string MakeKernelName(std::string_view prefix,
                           const HloInstruction& inst) {
  // Sometimes an instruction doesn't have metadata, but the computations that
  // it calls do have metadata. Consider all of those metadata op_name entries
  // and attach the longest prefix to this launch.
  std::string_view op_name = GetLongestOpNamePrefix(inst);
  if (op_name.empty()) {
    return absl::StrFormat("Thunk:#hlo_op=%s#", inst.name());
  } else if (op_name.substr(0, prefix.size()) != prefix) {
    // the op_name we got for this instruction does not start with the prefix
    // that we thought was common to all instructions in the module
    return absl::StrFormat("Thunk:#name=%s,hlo_op=%s#", op_name, inst.name());
  } else {
    // remove the prefix that's in the parent module annotation
    auto short_name = op_name.substr(prefix.size());
    // remove the leading / if there is one (prefix might be an empty string)
    if (!short_name.empty() && short_name.front() == '/') {
      short_name = short_name.substr(1);
    }
    return absl::StrFormat("Thunk:#name=%s,hlo_op=%s#", short_name,
                           inst.name());
  }
}
}  // namespace

KernelAnnotation::KernelAnnotation(const ModuleAnnotation& module_annotation,
                                   const HloInstruction& inst)
    : title_str(
          MakeKernelName(module_annotation.longest_op_name_prefix(), inst)),
      title(RegisterString(title_str)) {}

ModuleAnnotations::ModuleAnnotations(std::string_view module_name)
    : top_level(module_name) {}

ModuleAnnotations::ModuleAnnotations(const HloModule& mod) : top_level{mod} {
  // loop through `mod` and populate `kernels` (string -> KernelAnnotation map)
  // with the information we want to attach to individual kernels.
  for (const HloComputation* computation :
       mod.computations()) {  // top-level blocks in the module
    for (const HloInstruction* inst :
         computation->instructions()) {  // statements within block
      // working assumption: only custom calls and fusions end up with NVTX
      // ranges named after them. bad assumption [at least partially]: cuda
      // graph launches are not handled correctly
      switch (inst->opcode()) {
        case HloOpcode::kCustomCall:
        case HloOpcode::kFusion: {
          // e.g. inst.name is "fusion.6", inst.opcode is "kFusion" and called
          // is ["fused_computation.5"], in which case the content of
          // "fused_computation.5" ends up under an NVTX range called
          // "fusion.6". We want to construct a useful annotation for that NVTX
          // range based on the content of `inst`, including `called` etc.
          // FIXME: using try_emplace here was sensitive to
          // https://github.com/abseil/abseil-cpp/issues/388.
          kernels.insert({inst->name(), {top_level, *inst}});
        } break;
        default:
          break;
      }
    }
  }
}

//===----------------------------------------------------------------------===//
// Scoped RAII helper to set and restore thread local module annotations
//===----------------------------------------------------------------------===//

namespace {
thread_local const ModuleAnnotations* current_annotations = nullptr;
}  // namespace

ScopedModuleAnnotations::ScopedModuleAnnotations(
    const ModuleAnnotations* annotations)
    : restore_(std::exchange(current_annotations, annotations)) {}

ScopedModuleAnnotations::~ScopedModuleAnnotations() {
  std::exchange(current_annotations, restore_);
}

const ModuleAnnotations* GetCurrentModuleAnnotations() {
  return current_annotations;
}

}  // namespace xla::gpu
