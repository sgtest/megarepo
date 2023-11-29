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

#ifndef XLA_HLO_EXPERIMENTAL_AUTO_SHARDING_AUTO_SHARDING_WRAPPER_H_
#define XLA_HLO_EXPERIMENTAL_AUTO_SHARDING_AUTO_SHARDING_WRAPPER_H_

#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

#include "xla/hlo/experimental/auto_sharding/auto_sharding_cost_graph.h"
#include "xla/hlo/experimental/auto_sharding/auto_sharding_option.h"
#include "xla/hlo/experimental/auto_sharding/auto_sharding_solver.h"
#include "xla/hlo/experimental/auto_sharding/auto_sharding_strategy.h"
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/hlo/ir/hlo_schedule.h"
#include "xla/hlo/utils/hlo_live_range.h"
#include "xla/service/hlo_cost_analysis.h"

namespace xla {
namespace spmd {

// A wrapper around the solver that converts the given objects into a
// combinatorial optimization problem & solves it.
AutoShardingSolverResult CallSolver(
    const HloModule& hlo_module, const HloLiveRange& hlo_live_range,
    const LivenessNodeSet& liveness_node_set, const StrategyMap& strategy_map,
    const StrategyGroups& strategy_groups, const CostGraph& cost_graph,
    const AliasSet& alias_set, const std::vector<NodeStrategyIdx>& s_hint,
    bool compute_iis, int64_t solver_timeout_in_seconds,
    const AutoShardingOption& option,
    const absl::flat_hash_map<std::string, const HloInstruction*>&
        sharding_propagation_solution = {});

// Computes the penalty to be used for fully replicated sharding strategies for
// dots and convs.
double GetDotConvReplicationPenalty(const HloInstruction* inst,
                                    size_t instruction_id, size_t window,
                                    const HloInstructionSequence& sequence,
                                    const HloCostAnalysis& hlo_cost_analysis);

}  // namespace spmd
}  // namespace xla

#endif  // XLA_HLO_EXPERIMENTAL_AUTO_SHARDING_AUTO_SHARDING_WRAPPER_H_
