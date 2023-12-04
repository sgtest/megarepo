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

#include "xla/service/gpu/custom_fusion_rewriter.h"

#include <cstdint>
#include <optional>
#include <utility>
#include <vector>

#include "absl/algorithm/container.h"
#include "absl/container/flat_hash_map.h"
#include "absl/container/flat_hash_set.h"
#include "absl/container/inlined_vector.h"
#include "absl/strings/str_cat.h"
#include "absl/strings/string_view.h"
#include "absl/types/span.h"
#include "xla/hlo/ir/hlo_casting_utils.h"
#include "xla/hlo/ir/hlo_computation.h"
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/hlo/ir/hlo_instructions.h"
#include "xla/hlo/ir/hlo_module.h"
#include "xla/service/gpu/kernels/custom_fusion_pattern.h"
#include "xla/statusor.h"
#include "xla/stream_executor/device_description.h"
#include "tsl/platform/errors.h"
#include "tsl/platform/logging.h"
#include "tsl/platform/statusor.h"

namespace xla::gpu {

CustomFusionRewriter::CustomFusionRewriter(
    const se::DeviceDescription* device,
    const CustomFusionPatternRegistry* patterns)
    : device_(device), patterns_(patterns) {}

// Returns a set of instruction that have users outside of a matched pattern
// and have a replacement that must be applied after building a new custom
// fusion instruction. Only root instruction can have external users and does
// not require a replacement, as the fusion itself is a replacement. If
// instruction has external users and does not have a replacement returns empty
// optional.
static std::optional<absl::flat_hash_set<HloInstruction*>>
GetPatternReplacements(const CustomFusionPattern::Match& match) {
  absl::flat_hash_set<HloInstruction*> requires_replacement;
  absl::flat_hash_set<HloInstruction*> instructions_set(
      match.instructions().begin(), match.instructions().end());

  for (HloInstruction* instr : match.instructions()) {
    for (HloInstruction* user : instr->users()) {
      if (instr == match.root() || instructions_set.contains(user)) continue;

      if (match.HasReplacement(instr)) {
        requires_replacement.insert(instr);
        continue;
      }

      VLOG(3) << "Custom fusion intermediate result " << instr->name()
              << " has users outside of a matched pattern: " << user->name();
      return std::nullopt;
    }
  }

  return requires_replacement;
}

// Returns instructions that have to become custom fusion parameters. Returns an
// error if matched pattern can't be outlined as a fusion.
static absl::InlinedVector<HloInstruction*, 4> GetPatternCaptures(
    const CustomFusionPattern::Match& match) {
  absl::InlinedVector<HloInstruction*, 4> captures;

  absl::flat_hash_set<HloInstruction*> instructions_set(
      match.instructions().begin(), match.instructions().end());

  for (HloInstruction* instr : match.instructions()) {
    for (HloInstruction* operand : instr->operands()) {
      if (!instructions_set.contains(operand) &&
          absl::c_find(captures, operand) == captures.end()) {
        captures.emplace_back(operand);
      }
    }
  }

  return captures;
}

// Creates custom fusion computation and moves all matched instructions into it.
static StatusOr<HloComputation*> CreateFusionBody(
    HloModule* module, const CustomFusionPattern::Match& match,
    absl::Span<HloInstruction* const> captures) {
  HloComputation::Builder builder(match.config().name());

  // A mapping from original instructions to instructions in the fusion body.
  absl::flat_hash_map<const HloInstruction*, HloInstruction*> instr_mapping;

  auto mapped_operands = [&](HloInstruction* instr) {
    absl::InlinedVector<HloInstruction*, 4> operands;
    for (HloInstruction* operand : instr->operands()) {
      operands.push_back(instr_mapping.at(operand));
    }
    return operands;
  };

  // For every parameter create a parameter instruction in the computation body
  // and set up instruction mapping.
  for (const HloInstruction* capture : captures) {
    int64_t index = instr_mapping.size();
    instr_mapping[capture] =
        builder.AddInstruction(HloInstruction::CreateParameter(
            index, capture->shape(), absl::StrCat("p", index)));
  }

  // TODO(ezhulenev): Instructions in the pattern must be topologically sorted,
  // otherwise we'll get a crash! Figure out how to do it!
  for (HloInstruction* instr : match.instructions()) {
    instr_mapping[instr] = builder.AddInstruction(
        instr->CloneWithNewOperands(instr->shape(), mapped_operands(instr)));
  }

  return module->AddComputationAndUnifyNamesAndIds(builder.Build(), false);
}

static StatusOr<HloInstruction*> CreateFusionInstruction(
    HloModule* module, const CustomFusionPattern::Match& match,
    absl::Span<HloInstruction* const> captures, HloComputation* body) {
  // We'll be replacing the root operation of a custom fusion with a fusion
  // instruction calling fusion computation.
  HloInstruction* root = match.root();
  HloComputation* parent = root->parent();

  // Add a fusion operation calling outlined fusion computation.
  HloInstruction* fusion = parent->AddInstruction(HloInstruction::CreateFusion(
      root->shape(), HloInstruction::FusionKind::kCustom, captures, body));
  module->SetAndUniquifyInstrName(fusion, match.config().name());

  // Set backends config to a matched custom fusion config.
  FusionBackendConfig backend_config;
  backend_config.set_kind("__custom_fusion");
  *backend_config.mutable_custom_fusion_config() = match.config();
  TF_RETURN_IF_ERROR(fusion->set_backend_config(std::move(backend_config)));

  return fusion;
}

StatusOr<bool> CustomFusionRewriter::Run(
    HloModule* module,
    const absl::flat_hash_set<absl::string_view>& execution_threads) {
  std::vector<CustomFusionPattern::Match> matches;

  // Collect all potential custom fusion matches in the module.
  for (HloComputation* computation : module->computations()) {
    for (HloInstruction* instr : computation->instructions()) {
      auto matched = patterns_->Match(*device_, instr);
      matches.insert(matches.end(), matched.begin(), matched.end());
    }
  }

  if (matches.empty()) return false;

  for (const CustomFusionPattern::Match& match : matches) {
    VLOG(2) << "Matched custom fusion " << match.config().name()
            << "; root instruction: " << match.instructions().back()->name();

    auto replacememts = GetPatternReplacements(match);
    if (!replacememts.has_value()) continue;

    auto captures = GetPatternCaptures(match);

    TF_ASSIGN_OR_RETURN(HloComputation * fusion_body,
                        CreateFusionBody(module, match, captures));
    TF_ASSIGN_OR_RETURN(
        HloInstruction * fusion,
        CreateFusionInstruction(module, match, captures, fusion_body));

    VLOG(2) << "Added a fusion instruction: " << fusion->name()
            << " for custom fusion " << match.config().name()
            << " (instruction count = " << match.instructions().size() << ")";

    for (HloInstruction* instr : *replacememts) {
      VLOG(2) << "Replace matched instruction: " << instr->name()
              << " with a pattern replacement";

      TF_ASSIGN_OR_RETURN(
          HloInstruction * replacement,
          match.BuildReplacement(instr, Cast<HloFusionInstruction>(fusion)));

      TF_RETURN_IF_ERROR(
          instr->ReplaceAllUsesWith(replacement, match.config().name()));

      VLOG(2) << "Replaced instruction: " << instr->name()
              << " with: " << replacement->name();
    }

    VLOG(2) << "Replace custom fusion root instruction " << match.root()->name()
            << "with " << fusion->name();
    HloComputation* parent = match.root()->parent();
    TF_RETURN_IF_ERROR(parent->ReplaceInstruction(match.root(), fusion));
  }

  return true;
}

}  // namespace xla::gpu
