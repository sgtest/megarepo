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

#include "xla/hlo/experimental/auto_sharding/auto_sharding_solver.h"

#include <cstdint>
#include <optional>
#include <string>
#include <tuple>
#include <utility>
#include <vector>

#include <gtest/gtest.h>
#include "xla/hlo/experimental/auto_sharding/auto_sharding.pb.h"
#include "xla/hlo/experimental/auto_sharding/auto_sharding_strategy.h"

namespace xla {
namespace spmd {
namespace {

using CostMatrix = std::vector<std::vector<double>>;
using NodeMatrix = std::vector<std::vector<int64_t>>;

void AddCosts(proto2::RepeatedPtrField<AutoShardingSolverRequest_Costs>* costs,
              const CostMatrix& cost_matrix) {
  for (const auto& cost_row : cost_matrix) {
    AutoShardingSolverRequest_Costs cost;
    cost.mutable_costs()->Add(cost_row.begin(), cost_row.end());
    costs->Add(std::move(cost));
  }
}

void AddNodes(proto2::RepeatedPtrField<AutoShardingSolverRequest_Nodes>* nodes,
              const NodeMatrix& node_matrix) {
  for (const auto& node_row : node_matrix) {
    AutoShardingSolverRequest_Nodes node;
    node.mutable_nodes()->Add(node_row.begin(), node_row.end());
    nodes->Add(std::move(node));
  }
}

// clang-format off

AutoShardingSolverRequest DefaultAutoShardingSolverRequest() {
  // The problem below is partially inspired by 'DotLHSTwoNonContractingDims'
  const auto s_len = {4, 3, 4, 4, 3};
  const auto s_follow = {-1, -1, -1, 2, -1};
  AutoShardingSolverRequest_Pair edge1, edge2;
  edge1.set_first(0);
  edge1.set_second(2);
  edge2.set_first(1);
  edge2.set_second(2);
  const auto edges = {edge1, edge2};
  const NodeMatrix live = {{1, 0},
                           {1, 0},
                           {1, 2, 0},
                           {1, 2, 3, 0},
                           {1, 3, 0}};
  const CostMatrix c = {{10, 11, 12, 13},
                        {20, 21, 22},
                        {30, 31, 32, 33},
                        {40, 41, 42, 43},
                        {50, 51, 52, 53}};
  const CostMatrix d = {{100, 110, 120, 130},
                        {200, 210, 220},
                        {300, 310, 320, 330},
                        {400, 410, 420, 430},
                        {500, 510, 520}};
  const CostMatrix m = {{100000, 110000, 990000, 130000},
                        {200000, 210000, 220000},
                        {300000, 310000, 320000, 330000},
                        {400000, 410000, 420000, 430000},
                        {500000, 510000, 520000}};
  const CostMatrix p = {{1.0, 0.0, 1.0, 1.0},
                        {1.0, 0.0, 1.0},
                        {1.0, 0.0, 1.0, 1.0},
                        {1.0, 0.0, 1.0, 1.0},
                        {1.0, 0.0, 1.0}};
  const CostMatrix r = {{1000, 1100, 1200, 1300,
                         2000, 2100, 2200, 2300,
                         3000, 3100, 3200, 3300,
                         4000, 4100, 4200, 4300},
                        {5000, 5100, 5200, 5300,
                         6000, 6100, 6200, 6300,
                         7000, 7100, 7200, 7300}};
  const CostMatrix t = {{73000, 72000, 71000, 70000,
                         63000, 62000, 61000, 60000,
                         53000, 52000, 51000, 50000,
                         43000, 42000, 41000, 40000},
                        {33000, 32000, 31000, 30000,
                         23000, 22000, 21000, 20000,
                         13000, 12000, 11000, 10000}};
  AutoShardingSolverRequest_Pair alias;
  alias.set_first(1);
  alias.set_second(4);
  const auto aliases = {alias};
  const CostMatrix v = {{0, 1, 1,
                         1, 0, 1,
                         1, 1, 0}};
  const std::vector<std::string> instruction_names = {"A", "B", "C", "D", "E"};

  AutoShardingSolverRequest request;
  request.set_num_nodes(5);
  request.set_memory_budget(1500000);
  request.mutable_s_len()->Add(s_len.begin(), s_len.end());
  request.mutable_s_follow()->Add(s_follow.begin(), s_follow.end());
  request.mutable_edges()->Add(edges.begin(), edges.end());
  AddNodes(request.mutable_live(), live);
  AddCosts(request.mutable_computation_costs(), c);
  AddCosts(request.mutable_communication_costs(), d);
  AddCosts(request.mutable_memory_costs(), m);
  AddCosts(request.mutable_departure_costs(), p);
  AddCosts(request.mutable_resharding_costs(), r);
  AddCosts(request.mutable_duration_costs(), t);
  request.mutable_aliases()->Add(aliases.begin(), aliases.end());
  AddCosts(request.mutable_value_costs(), v);
  request.mutable_instruction_names()->Add(instruction_names.begin(),
                                           instruction_names.end());
  AddCosts(request.mutable_computation_costs(), c);
  return request;
}

TEST(CallORToolsSolverTest, SolvesOptimally) {
  const AutoShardingSolverRequest request = DefaultAutoShardingSolverRequest();

  const AutoShardingSolverResult result = CallORToolsSolver(request);

  const std::vector<NodeStrategyIdx> s_val = {0, 0, 0, 0, 0};
  const std::vector<EdgeStrategyIdx> e_val = {0, 0};
  const double objective_value = 7650.0;
  const AutoShardingSolverResult expected_result = {
      std::make_tuple(
          std::move(s_val), std::move(e_val), objective_value), false};
  EXPECT_EQ(result, expected_result);
}

TEST(CallORToolsSolverTest, SolvesOverbudget) {
  AutoShardingSolverRequest request = DefaultAutoShardingSolverRequest();
  request.set_memory_budget(100000);
  request.mutable_overbudget_coeff()->set_coeff(10.0);

  const AutoShardingSolverResult result = CallORToolsSolver(request);

  const std::vector<NodeStrategyIdx> s_val = {0, 0, 0, 0, 0};
  const std::vector<EdgeStrategyIdx> e_val = {0, 0};
  const double objective_value = 9007650.0;
  const AutoShardingSolverResult expected_result = {
      std::make_tuple(
          std::move(s_val), std::move(e_val), objective_value), false};
  EXPECT_EQ(result, expected_result);
}

TEST(CallORToolsSolverTest, SolvesMaxDepartures) {
  AutoShardingSolverRequest request = DefaultAutoShardingSolverRequest();
  request.mutable_max_departures()->set_coeff(3.0);

  const AutoShardingSolverResult result = CallORToolsSolver(request);

  const std::vector<NodeStrategyIdx> s_val = {0, 0, 1, 1, 0};
  const std::vector<EdgeStrategyIdx> e_val = {1, 1};
  const double objective_value = 7872.0;
  const AutoShardingSolverResult expected_result = {
      std::make_tuple(
          std::move(s_val), std::move(e_val), objective_value), false};
  EXPECT_EQ(result, expected_result);
}

TEST(CallORToolsSolverTest, AvoidsInfiniteNodeCosts) {
  AutoShardingSolverRequest request = DefaultAutoShardingSolverRequest();
  request.mutable_computation_costs(0)->set_costs(0, kInfinityCost);
  request.mutable_computation_costs(0)->set_costs(1, kInfinityCost);
  request.mutable_computation_costs(0)->set_costs(2, kInfinityCost);

  const AutoShardingSolverResult result = CallORToolsSolver(request);

  const std::vector<NodeStrategyIdx> s_val = {3, 0, 0, 0, 0};
  const std::vector<EdgeStrategyIdx> e_val = {12, 0};
  const double objective_value = 10683.0;
  const AutoShardingSolverResult expected_result = {
      std::make_tuple(
          std::move(s_val), std::move(e_val), objective_value), false};
  EXPECT_EQ(result, expected_result);
}

TEST(CallORToolsSolverTest, AvoidsInfiniteEdgeCosts) {
  AutoShardingSolverRequest request = DefaultAutoShardingSolverRequest();
  request.mutable_resharding_costs(0)->set_costs(0, kInfinityCost);

  const AutoShardingSolverResult result = CallORToolsSolver(request);

  const std::vector<NodeStrategyIdx> s_val = {0, 0, 1, 1, 0};
  const std::vector<EdgeStrategyIdx> e_val = {1, 1};
  const double objective_value = 7872.0;
  const AutoShardingSolverResult expected_result = {
      std::make_tuple(
          std::move(s_val), std::move(e_val), objective_value), false};
  EXPECT_EQ(result, expected_result);
}

TEST(CallORToolsSolverTest, HandlesFollowedEdges) {
  AutoShardingSolverRequest request = DefaultAutoShardingSolverRequest();
  AutoShardingSolverRequest_Pair edge;
  edge.set_first(1);
  edge.set_second(3);
  // Reduces to {1, 2} since node 3 follows node 2
  *request.mutable_edges()->Add() = edge;
  const CostMatrix r = {{5000, 5100, 5200, 5300,
                         6000, 6100, 6200, 6300,
                         7000, 7100, 7200, 7300}};
  AddCosts(request.mutable_resharding_costs(), r);
  const CostMatrix t = {{50000, 51000, 52000, 53000,
                         60000, 61000, 62000, 63000,
                         70000, 71000, 72000, 73000}};
  AddCosts(request.mutable_duration_costs(), t);

  const AutoShardingSolverResult result = CallORToolsSolver(request);

  const std::vector<NodeStrategyIdx> s_val = {0, 0, 0, 0, 0};
  const std::vector<EdgeStrategyIdx> e_val = {0, 0, 0};
  const double objective_value = 12650.0;
  const AutoShardingSolverResult expected_result = {
      std::make_tuple(
          std::move(s_val), std::move(e_val), objective_value), false};
  EXPECT_EQ(result, expected_result);
}

TEST(CallORToolsSolverTest, UsesHint) {
  AutoShardingSolverRequest request = DefaultAutoShardingSolverRequest();
  const auto s_hint = {1, 0, 0, 0, 0};  // Not optimal, but close.
  request.mutable_s_hint()->Add(s_hint.begin(), s_hint.end());

  const AutoShardingSolverResult result = CallORToolsSolver(request);

  const std::vector<NodeStrategyIdx> s_val = {0, 0, 0, 0, 0};
  const std::vector<EdgeStrategyIdx> e_val = {0, 0};
  const double objective_value = 7650.0;
  const AutoShardingSolverResult expected_result = {
      std::make_tuple(
          std::move(s_val), std::move(e_val), objective_value), false};
  EXPECT_EQ(result, expected_result);
}

TEST(AutoShardingEvaluatorTest, NoViolations) {
  const AutoShardingSolverRequest request = DefaultAutoShardingSolverRequest();
  const std::vector<NodeStrategyIdx> s_val = {3, 1, 2, 2, 1};
  const std::vector<EdgeStrategyIdx> e_val = {14, 6};
  const double objective_value = 12149.0;
  const AutoShardingSolverResult result = {
      std::make_tuple(
          std::move(s_val), std::move(e_val), objective_value), false};

  const AutoShardingEvaluation evaluation = Evaluate(request, result);

  AutoShardingEvaluation expected_evaluation;
  expected_evaluation.total.computation_cost = 159.0;  // 13+21+32+42+51
  expected_evaluation.total.communication_cost = 1590.0;  // 130+210+320+420+510
  expected_evaluation.total.resharding_cost = 10400.0;  // 4200+6200
  expected_evaluation.lower_bound.computation_cost = 150.0;
  expected_evaluation.lower_bound.communication_cost = 1500.0;
  expected_evaluation.lower_bound.resharding_cost = 6000.0;
  expected_evaluation.total_departures = 3.0;
  EXPECT_EQ(evaluation, expected_evaluation);
}

TEST(AutoShardingEvaluatorTest, EvaluatesOverbudget) {
  AutoShardingSolverRequest request = DefaultAutoShardingSolverRequest();
  request.set_memory_budget(100000);
  request.mutable_overbudget_coeff()->set_coeff(10.0);
  const std::vector<NodeStrategyIdx> s_val = {2 /* violates */, 1, 2, 2, 1};
  const std::vector<EdgeStrategyIdx> e_val = {10, 6};
  const double objective_value = 11138.0;
  const AutoShardingSolverResult result = {
      std::make_tuple(
          std::move(s_val), std::move(e_val), objective_value), false};

  const AutoShardingEvaluation evaluation = Evaluate(request, result);

  AutoShardingEvaluation expected_evaluation;
  expected_evaluation.total.computation_cost = 158.0;  // 12+21+32+42+51
  expected_evaluation.total.communication_cost = 1580.0;  // 120+210+320+420+510
  expected_evaluation.total.resharding_cost = 9400.0;  // 3200+6200
  expected_evaluation.total.overbudget_cost = 18400000.0;  // 10*1840000
  expected_evaluation.lower_bound.computation_cost = 150.0;
  expected_evaluation.lower_bound.communication_cost = 1500.0;
  expected_evaluation.lower_bound.resharding_cost = 6000.0;
  expected_evaluation.lower_bound.overbudget_cost = 9000000.0;
  expected_evaluation.total_departures = 3.0;
  EXPECT_EQ(evaluation, expected_evaluation);
}

TEST(AutoShardingEvaluatorTest, ViolatesFollower) {
  const AutoShardingSolverRequest request = DefaultAutoShardingSolverRequest();
  const std::vector<NodeStrategyIdx> s_val = {3, 1, 2, 1 /* violates */, 1};
  const std::vector<EdgeStrategyIdx> e_val = {14, 6};
  const double objective_value = 12138.0;
  const AutoShardingSolverResult result = {
      std::make_tuple(
          std::move(s_val), std::move(e_val), objective_value), false};

  const AutoShardingEvaluation evaluation = Evaluate(request, result);

  AutoShardingEvaluation expected_evaluation;
  expected_evaluation.violation_codes = {kFollowerViolationCode};
  expected_evaluation.total.computation_cost = 158.0;  // 13+21+32+41+51
  expected_evaluation.total.communication_cost = 1580.0;  // 130+210+320+410+510
  expected_evaluation.total.resharding_cost = 10400.0;  // 4200+6200
  expected_evaluation.lower_bound.computation_cost = 150.0;
  expected_evaluation.lower_bound.communication_cost = 1500.0;
  expected_evaluation.lower_bound.resharding_cost = 6000.0;
  expected_evaluation.total_departures = 2.0;
  EXPECT_EQ(evaluation, expected_evaluation);
}

TEST(AutoShardingEvaluatorTest, ViolatesAlias) {
  const AutoShardingSolverRequest request = DefaultAutoShardingSolverRequest();
  const std::vector<NodeStrategyIdx> s_val = {3, 1, 2, 2, 0 /* violates */};
  const std::vector<EdgeStrategyIdx> e_val = {14, 6};
  const double objective_value = 12138.0;
  const AutoShardingSolverResult result = {
      std::make_tuple(
          std::move(s_val), std::move(e_val), objective_value), false};

  const AutoShardingEvaluation evaluation = Evaluate(request, result);

  AutoShardingEvaluation expected_evaluation;
  expected_evaluation.violation_codes = {kAliasViolationCode};
  expected_evaluation.total.computation_cost = 158.0;  // 13+21+32+42+50
  expected_evaluation.total.communication_cost = 1580.0;  // 130+210+320+420+500
  expected_evaluation.total.resharding_cost = 10400.0;  // 4200+6200
  expected_evaluation.lower_bound.computation_cost = 150.0;
  expected_evaluation.lower_bound.communication_cost = 1500.0;
  expected_evaluation.lower_bound.resharding_cost = 6000.0;
  expected_evaluation.total_departures = 4.0;
  EXPECT_EQ(evaluation, expected_evaluation);
}

TEST(AutoShardingEvaluatorTest, ViolatesMemory) {
  const AutoShardingSolverRequest request = DefaultAutoShardingSolverRequest();
  const std::vector<NodeStrategyIdx> s_val = {2 /* violates */, 1, 2, 2, 1};
  const std::vector<EdgeStrategyIdx> e_val = {10, 6};
  const double objective_value = 11138.0;
  const AutoShardingSolverResult result = {
      std::make_tuple(
          std::move(s_val), std::move(e_val), objective_value), false};

  const AutoShardingEvaluation evaluation = Evaluate(request, result);

  AutoShardingEvaluation expected_evaluation;
  expected_evaluation.violation_codes = {kMemoryViolationCode};
  expected_evaluation.total.computation_cost = 158.0;  // 12+21+32+42+51
  expected_evaluation.total.communication_cost = 1580.0;  // 120+210+320+420+510
  expected_evaluation.total.resharding_cost = 9400.0;  // 3200+6200
  expected_evaluation.lower_bound.computation_cost = 150.0;
  expected_evaluation.lower_bound.communication_cost = 1500.0;
  expected_evaluation.lower_bound.resharding_cost = 6000.0;
  expected_evaluation.total_departures = 3.0;
  EXPECT_EQ(evaluation, expected_evaluation);
}

TEST(AutoShardingEvaluatorTest, ViolatesInfiniteCostForNode) {
  AutoShardingSolverRequest request = DefaultAutoShardingSolverRequest();
  request.mutable_computation_costs(0)->set_costs(0, kInfinityCost);
  request.mutable_computation_costs(0)->set_costs(1, kInfinityCost);
  request.mutable_computation_costs(0)->set_costs(2, kInfinityCost);
  const std::vector<NodeStrategyIdx> s_val = {0 /* violates */, 1, 2, 2, 1};
  const std::vector<EdgeStrategyIdx> e_val = {2, 6};
  const double objective_value = 1e+20;
  const AutoShardingSolverResult result = {
      std::make_tuple(
          std::move(s_val), std::move(e_val), objective_value), false};

  const AutoShardingEvaluation evaluation = Evaluate(request, result);

  AutoShardingEvaluation expected_evaluation;
  expected_evaluation.violation_codes = {kInfiniteCostViolationCode};
  expected_evaluation.total.computation_cost = 1e+20;  // infinite cost
  expected_evaluation.total.communication_cost = 1560.0;  // 100+210+320+420+510
  expected_evaluation.total.resharding_cost = 7400.0;  // 1200+6200
  expected_evaluation.lower_bound.computation_cost = 153.0;
  expected_evaluation.lower_bound.communication_cost = 1500.0;
  expected_evaluation.lower_bound.resharding_cost = 6000.0;
  expected_evaluation.total_departures = 3.0;
  EXPECT_EQ(evaluation, expected_evaluation);
}

TEST(AutoShardingEvaluatorTest, ViolatesInfiniteCostForEdge) {
  AutoShardingSolverRequest request = DefaultAutoShardingSolverRequest();
  request.mutable_resharding_costs(0)->set_costs(2, kInfinityCost);
  const std::vector<NodeStrategyIdx> s_val = {0, 1, 2, 2, 1};
  const std::vector<EdgeStrategyIdx> e_val = {2 /* violates */, 6};
  const double objective_value = 1e+20;
  const AutoShardingSolverResult result = {
      std::make_tuple(
          std::move(s_val), std::move(e_val), objective_value), false};

  const AutoShardingEvaluation evaluation = Evaluate(request, result);

  AutoShardingEvaluation expected_evaluation;
  expected_evaluation.violation_codes = {kInfiniteCostViolationCode};
  expected_evaluation.total.computation_cost = 156.0;  // 10+21+32+42+51
  expected_evaluation.total.communication_cost = 1560.0;  // 100+210+320+420+510
  expected_evaluation.total.resharding_cost = 1e+20;  // infinite cost
  expected_evaluation.lower_bound.computation_cost = 150.0;
  expected_evaluation.lower_bound.communication_cost = 1500.0;
  expected_evaluation.lower_bound.resharding_cost = 6000.0;
  expected_evaluation.total_departures = 3.0;
  EXPECT_EQ(evaluation, expected_evaluation);
}

TEST(AutoShardingEvaluatorTest, ViolatesMaxDepartures) {
  AutoShardingSolverRequest request = DefaultAutoShardingSolverRequest();
  request.mutable_max_departures()->set_coeff(2.0);
  const std::vector<NodeStrategyIdx> s_val = {3, 1, 2, 2, 1};
  const std::vector<EdgeStrategyIdx> e_val = {14, 6};
  const double objective_value = 12149.0;
  const AutoShardingSolverResult result = {
      std::make_tuple(
          std::move(s_val), std::move(e_val), objective_value), false};

  const AutoShardingEvaluation evaluation = Evaluate(request, result);

  AutoShardingEvaluation expected_evaluation;
  expected_evaluation.violation_codes = {kMaxDeparturesViolationCode};
  expected_evaluation.total.computation_cost = 159.0;  // 13+21+32+42+51
  expected_evaluation.total.communication_cost = 1590.0;  // 130+210+320+420+510
  expected_evaluation.total.resharding_cost = 10400.0;  // 4200+6200
  expected_evaluation.lower_bound.computation_cost = 150.0;
  expected_evaluation.lower_bound.communication_cost = 1500.0;
  expected_evaluation.lower_bound.resharding_cost = 6000.0;
  expected_evaluation.total_departures = 3.0;
  EXPECT_EQ(evaluation, expected_evaluation);
}

TEST(AutoShardingRationalizerTest, RationalizesProperly) {
  const AutoShardingSolverRequest request = DefaultAutoShardingSolverRequest();
  const std::vector<NodeStrategyIdx> s_val = {0, 1, 2, 2, 1};
  const std::vector<EdgeStrategyIdx> e_val = {2, 6};
  const double objective_value = 9116.0;
  const AutoShardingSolverResult result = {
      std::make_tuple(
          std::move(s_val), std::move(e_val), objective_value), false};
  const std::vector<NodeStrategyIdx> s_subopt = {3, 1, 2, 2, 1};
  const std::vector<EdgeStrategyIdx> e_subopt = {14, 6};
  const double subopt_value = 12149.0;
  const AutoShardingSolverResult subopt = {
      std::make_tuple(
          std::move(s_subopt), std::move(e_subopt), subopt_value), false};

  const std::vector<std::string> rationales =
      Rationalize(request, result, subopt);

  const std::vector<std::string> expected_rationales = {
      "strategy changes for A (0 -> 3)",
      "communication cost increases for A (100 -> 130)",
      "computation cost increases for A (10 -> 13)",
      "resharding cost increases for A and C (1200 -> 4200)"};
  EXPECT_EQ(rationales, expected_rationales);
}

// clang-format on

}  // namespace
}  // namespace spmd
}  // namespace xla
