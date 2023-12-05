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

#ifndef XLA_HLO_EXPERIMENTAL_AUTO_SHARDING_AUTO_SHARDING_SOLVER_H_
#define XLA_HLO_EXPERIMENTAL_AUTO_SHARDING_AUTO_SHARDING_SOLVER_H_

#include <cstdint>
#include <string>
#include <tuple>
#include <vector>

#include "absl/container/flat_hash_set.h"
#include "xla/hlo/experimental/auto_sharding/auto_sharding.pb.h"
#include "xla/hlo/experimental/auto_sharding/auto_sharding_strategy.h"
#include "xla/statusor.h"
#include "ortools/linear_solver/linear_solver.h"

namespace xla {
namespace spmd {

struct AutoShardingSolverResult {
 public:
  AutoShardingSolverResult(
      StatusOr<std::tuple<std::vector<NodeStrategyIdx>,
                          std::vector<EdgeStrategyIdx>, double>>
          status,
      bool skip_auto_sharding)
      : status(status), skip_auto_sharding(skip_auto_sharding) {}
  bool operator==(const AutoShardingSolverResult& other) const;
  StatusOr<std::tuple<std::vector<int64_t>, std::vector<int64_t>, double>>
      status;
  bool skip_auto_sharding;
};

AutoShardingSolverResult CallORToolsSolver(
    const AutoShardingSolverRequest& request);

enum AutoShardingViolationCode {
  kAliasViolationCode,     // Some node's strategy does not match its alias
  kFollowerViolationCode,  // Some node's strategy does not match its follower
  kInfiniteCostViolationCode,   // Some node or edge incurs infinite cost
  kMemoryViolationCode,         // The solution eclipses the memory budget
  kMaxDeparturesViolationCode,  // The solution has too many sharding departures
};

struct CostComponents {
  double communication_cost = 0.0;
  double computation_cost = 0.0;
  double resharding_cost = 0.0;
  double overbudget_cost = 0.0;
  double makespan_cost = 0.0;

  double cost() const;

  bool operator==(const CostComponents& other) const;
};

// Captures the metrics, lower bounds, and constraint violations for the
// sharding result.
struct AutoShardingEvaluation {
  // A set of constraint violations; should be empty for any viable solution.
  absl::flat_hash_set<AutoShardingViolationCode> violation_codes;

  // A breakdown and lower bound for each individual cost component.
  CostComponents total;
  CostComponents lower_bound;

  // How many instructions departed from the "default" sharding strategy.
  double total_departures = 0.0;

  // The (raw) total makespan, i.e., not scaled by the makespan coefficient.
  double total_makespan = 0.0;

  bool operator==(const AutoShardingEvaluation& other) const;
};

// Evaluates the given solver result w.r.t. the input request, computing various
// solution quality metrics and validating the consistency of hard constraints.
AutoShardingEvaluation Evaluate(const AutoShardingSolverRequest& request,
                                const AutoShardingSolverResult& result);

// Produces a list of rationales for why an alternate result may be suboptimal.
std::vector<std::string> Rationalize(const AutoShardingSolverRequest& request,
                                     const AutoShardingSolverResult& result,
                                     const AutoShardingSolverResult& subopt);

// Creates and returns a variable for makespan.
operations_research::MPVariable* CreateMakespanVar(
    const AutoShardingSolverRequest& request,
    const std::vector<std::vector<operations_research::MPVariable*>>& e,
    operations_research::MPSolver& solver);

double EvaluateMakespan(const AutoShardingSolverRequest& request,
                        const AutoShardingSolverResult& result,
                        AutoShardingEvaluation& evaluation);

}  // namespace spmd
}  // namespace xla

#endif  // XLA_HLO_EXPERIMENTAL_AUTO_SHARDING_AUTO_SHARDING_SOLVER_H_
