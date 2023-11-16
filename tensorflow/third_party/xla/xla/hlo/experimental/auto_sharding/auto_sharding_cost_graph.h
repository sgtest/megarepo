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

#ifndef XLA_HLO_EXPERIMENTAL_AUTO_SHARDING_AUTO_SHARDING_COST_GRAPH_H_
#define XLA_HLO_EXPERIMENTAL_AUTO_SHARDING_AUTO_SHARDING_COST_GRAPH_H_

#include <algorithm>
#include <cstddef>
#include <numeric>
#include <string>
#include <utility>
#include <vector>

#include "absl/log/check.h"
#include "absl/strings/str_cat.h"
#include "absl/types/span.h"
#include "xla/hlo/experimental/auto_sharding/auto_sharding_strategy.h"
#include "xla/hlo/experimental/auto_sharding/matrix.h"
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/shape_util.h"
namespace xla {
namespace spmd {

// A graph data structrue to simplify the edge cost graph.
// It merges nodes and does path compression.
class CostGraph {
 public:
  CostGraph(const StrategyGroups& strategy_groups,
            const AssociativeDotPairs& associative_dot_pairs) {
    node_lens_.reserve(strategy_groups.size());
    extra_node_costs_.reserve(strategy_groups.size());
    adjacency_.assign(strategy_groups.size(), StableHashSet<int>());

    // Build the cost graph
    for (const auto& strategies : strategy_groups) {
      node_lens_.push_back(strategies->strategies.size());
      extra_node_costs_.push_back(
          std::vector<double>(strategies->strategies.size(), 0.0));

      for (size_t i = 0; i < strategies->in_nodes.size(); ++i) {
        if (!strategies->in_nodes[i]->is_tuple) {
          NodeIdx src_idx = strategies->in_nodes[i]->node_idx;
          NodeIdx dst_idx = strategies->node_idx;
          Matrix edge_cost = CreateEdgeCost(src_idx, dst_idx, i, strategies);
          AddEdgeCost(src_idx, dst_idx, edge_cost);
        } else if (strategies->in_nodes[i]->is_tuple &&
                   strategies->in_nodes.size() > 1) {
          for (size_t l = 0; l < strategies->in_nodes[i]->childs.size(); l++) {
            NodeIdx src_idx = strategies->in_nodes[i]->childs.at(l)->node_idx;
            NodeIdx dst_idx = strategies->node_idx;
            Matrix edge_cost =
                CreateEdgeCost(src_idx, dst_idx, i, strategies, true);
            AddEdgeCost(src_idx, dst_idx, edge_cost);
          }

        } else {
          CHECK_EQ(strategies->in_nodes.size(), 1)
              << "Do not support instructions with more than one tuple "
                 "operand. If this CHECK fails, we will need to fix "
                 "b/233412625.";
          for (size_t l = 0; l < strategies->in_nodes[i]->childs.size(); l++) {
            NodeIdx src_idx = strategies->in_nodes[i]->childs.at(l)->node_idx;
            NodeIdx dst_idx = strategies->node_idx;
            // TODO(b/233412625) Support more general case, e.g., multiple tuple
            // operands. If there is only one operand and it's a tuple, the
            // first index of resharding_costs is for the tuple element.
            Matrix edge_cost =
                CreateEdgeCost(src_idx, dst_idx, /*in_node_idx=*/l, strategies);
            AddEdgeCost(src_idx, dst_idx, edge_cost);
          }
        }
      }

      if (strategies->following) {
        to_merge_pairs_.push_back(
            {strategies->node_idx, strategies->following->node_idx});
      }
    }

    // Adjust the edge costs for dot pairs that can be optimized by
    // AllReduceReassociate
    for (const auto& pair : associative_dot_pairs) {
      NodeIdx src_idx = pair.first->node_idx;
      NodeIdx dst_idx = pair.second->node_idx;

      if (node_lens_[src_idx] != node_lens_[dst_idx]) {
        continue;
      }

      Matrix edge_cost(node_lens_[src_idx], node_lens_[dst_idx]);
      for (NodeStrategyIdx i = 0; i < node_lens_[src_idx]; ++i) {
        if (strategy_groups[src_idx]->strategies[i].communication_cost > 0) {
          CHECK_LE(
              std::abs(
                  strategy_groups[src_idx]->strategies[i].communication_cost -
                  strategy_groups[dst_idx]->strategies[i].communication_cost),
              1e-6);
          edge_cost(i, i) =
              -strategy_groups[src_idx]->strategies[i].communication_cost;
        }
      }
      AddEdgeCost(src_idx, dst_idx, edge_cost);
    }
  }

  Matrix CreateEdgeCost(NodeIdx src_idx, NodeIdx dst_idx, size_t in_node_idx,
                        StrategyGroup* strategy_group, bool zero_cost = false) {
    CHECK_LT(src_idx, node_lens_.size());
    CHECK_LT(dst_idx, node_lens_.size());
    Matrix edge_cost(node_lens_[src_idx], node_lens_[dst_idx]);
    for (NodeStrategyIdx k = 0; k < strategy_group->strategies.size(); ++k) {
      const ShardingStrategy& strategy = strategy_group->strategies[k];
      size_t start_idx = 0;
      if (strategy.resharding_costs[in_node_idx].size() > node_lens_[src_idx]) {
        start_idx =
            strategy.resharding_costs[in_node_idx].size() - node_lens_[src_idx];
      }
      for (size_t j = start_idx;
           j < strategy.resharding_costs[in_node_idx].size(); ++j) {
        edge_cost(j - start_idx, k) =
            zero_cost ? 0 : strategy.resharding_costs[in_node_idx][j];
      }
    }

    return edge_cost;
  }

  Matrix GetEdgeCost(NodeIdx i, NodeIdx j) {
    if (i <= j) {
      return edge_costs_[{i, j}];
    }
    return edge_costs_[{j, i}].Transpose();
  }

  void AddEdgeCost(NodeIdx i, NodeIdx j, Matrix& cost) {
    if (i > j) {
      std::swap(i, j);
      cost = cost.Transpose();
    }

    if (edge_costs_.contains({i, j})) {
      CHECK(adjacency_[i].contains(j));
      CHECK(adjacency_[j].contains(i));
      edge_costs_[{i, j}] = edge_costs_[{i, j}] + cost;
    } else {
      adjacency_[i].insert(j);
      adjacency_[j].insert(i);
      edge_costs_[{i, j}] = cost;
    }
  }

  void RemoveEdge(NodeIdx i, NodeIdx j) {
    if (i > j) {
      std::swap(i, j);
    }

    CHECK(adjacency_[i].contains(j));
    CHECK(adjacency_[j].contains(i));
    CHECK(edge_costs_.contains({i, j}));

    adjacency_[i].erase(j);
    adjacency_[j].erase(i);
    edge_costs_.erase({i, j});
  }

  void MergeNode(NodeIdx src, NodeIdx dst) {
    // Merge node src into node dst. This is used when we set one operator to
    // follow another operator's sharding spec. For the following computation
    // graph:
    //   dst -- src -- adj1
    //           |
    //          adj2
    //
    // It will be transformed into the following graph:
    //   (src)
    //    dst -- adj1
    //     |
    //    adj2
    // Where all the edges costs between src and adjs will be added into
    // the edge costs between dst and adjs. The edge cost between src and
    // dst will be added to the extra node cost of dst. Other node costs of
    // src will be added into dst's node cost in the ILP.

    CHECK(adjacency_[src].contains(dst));
    CHECK(adjacency_[dst].contains(src));
    CHECK(!merged_to_.contains(src));
    CHECK(!merged_to_.contains(dst));
    CHECK_NE(src, dst);

    Matrix edge_cost = GetEdgeCost(dst, src);

    std::vector<NodeStrategyIdx> reindexing(node_lens_[dst]);
    if (node_lens_[dst] == node_lens_[src]) {
      // Assume the orders of strategies in src and dst match
      // (i.e. i-th strategy in src follows i-th strategy in dst).
      // This is true in most cases because of how we create the
      // following strategies.
      std::iota(reindexing.begin(), reindexing.end(), 0);
    } else {
      // Otherwise, find the strategy to follow greedily.
      // For every strategy in dst, find the strategy in src with
      // the lowest resharding cost.
      std::vector<int> arange(node_lens_[src]);
      std::iota(arange.begin(), arange.end(), 0);
      for (NodeStrategyIdx i = 0; i < node_lens_[dst]; ++i) {
        std::vector<std::pair<double, int>> keys;

        // If there are multiple strategies with the same lowest costs,
        // prefer to follow "replicated", which has the largest index.
        // Node: We assume the strategy "Repilcated" is always appended
        // as the last strategy in BuildStrategyAndCost.
        keys.reserve(node_lens_[src]);
        for (NodeStrategyIdx j = 0; j < node_lens_[src]; ++j) {
          keys.push_back({edge_cost(i, j), -j});
        }

        std::sort(arange.begin(), arange.end(), [&keys](int l, int r) {
          return (keys[l].first < keys[r].first) ||
                 (keys[l].first == keys[r].first &&
                  keys[l].second < keys[r].second);
        });

        reindexing[i] = arange.front();
      }
    }
    merged_to_[src] = dst;
    reindexing_vector_[src] = reindexing;

    // Merge edge cost matrix
    std::vector<NodeIdx> adj_list(adjacency_[src].begin(),
                                  adjacency_[src].end());
    for (NodeIdx adj : adj_list) {
      if (adj == dst) {
        for (NodeStrategyIdx i = 0; i < node_lens_[dst]; ++i) {
          extra_node_costs_[dst][i] += edge_cost(i, reindexing[i]);
        }
      } else {
        Matrix added_edge_cost(node_lens_[dst], node_lens_[adj]);
        Matrix edge_cost_src_adj = GetEdgeCost(src, adj);

        for (NodeStrategyIdx i = 0; i < node_lens_[dst]; ++i) {
          for (NodeStrategyIdx k = 0; k < node_lens_[adj]; ++k) {
            added_edge_cost(i, k) = edge_cost_src_adj(reindexing[i], k);
          }
        }

        AddEdgeCost(dst, adj, added_edge_cost);
      }
    }

    // Remove edges
    for (NodeIdx adj : adj_list) {
      RemoveEdge(src, adj);
    }
  }

  NodeIdx QueryDestination(NodeIdx node_idx) {
    if (merged_to_.contains(node_idx)) {
      NodeIdx old_dst = merged_to_[node_idx];
      NodeIdx new_dst = QueryDestination(old_dst);
      if (old_dst != new_dst) {
        // Compresss path
        absl::Span<const NodeStrategyIdx> old_reindexing_vector =
            reindexing_vector_[node_idx];
        std::vector<NodeStrategyIdx> new_reindexing_vector;
        new_reindexing_vector.reserve(node_lens_.size());
        for (NodeStrategyIdx i = 0; i < node_lens_[new_dst]; ++i) {
          new_reindexing_vector.push_back(
              old_reindexing_vector[reindexing_vector_[old_dst][i]]);
        }
        reindexing_vector_[node_idx] = new_reindexing_vector;
        merged_to_[node_idx] = new_dst;
      }
      return new_dst;
    }
    return node_idx;
  }

  void Simplify(bool enable) {
    // Merge nodes
    for (const auto& pair : to_merge_pairs_) {
      NodeIdx src = pair.first;
      NodeIdx dst = pair.second;
      dst = QueryDestination(dst);
      if (enable) {
        MergeNode(src, dst);
      }
    }

    // Build follow map
    follow_idx_.reserve(node_lens_.size());
    for (NodeIdx i = 0; i < node_lens_.size(); ++i) {
      if (merged_to_.contains(i)) {
        follow_idx_.push_back(QueryDestination(i));
      } else {
        follow_idx_.push_back(-1);
      }
    }
  }

  NodeStrategyIdx RemapIndex(NodeIdx node_id, NodeStrategyIdx value) const {
    if (follow_idx_[node_id] < 0) {
      return value;
    }
    return reindexing_vector_.at(node_id)[value];
  }

  std::string ToString() {
    std::string str;
    absl::StrAppend(&str, "Cost Graph:\n");

    for (NodeIdx i = 0; i < node_lens_.size(); ++i) {
      absl::StrAppend(&str, "Node", i, ": ", node_lens_[i], "\n");
    }
    absl::StrAppend(&str, "\n");

    for (const auto& iter : edge_costs_) {
      absl::StrAppend(&str, "Edge (", iter.first.first, ", ", iter.first.second,
                      "):\n");
      absl::StrAppend(&str, iter.second.ToString(), "\n");
    }

    return str;
  }

  // The number of strategies of each node.
  std::vector<int> node_lens_;
  // The adjacency list of each node.
  std::vector<StableHashSet<int>> adjacency_;
  // The cost matrix between two nodes.

  StableHashMap<std::pair<NodeIdx, NodeIdx>, Matrix> edge_costs_;
  // The extra node costs introduced by merging nodes.
  std::vector<std::vector<double>> extra_node_costs_;
  // The reindexing vector of the node.
  // A reindexing vector maps a strategy index from the node being followed
  // to a strategy index of the current node.
  StableHashMap<int, std::vector<NodeStrategyIdx>> reindexing_vector_;
  // Maps a node id to the node id that is being followed by this node.
  // The value is -1 if the current node does not follow any node.
  std::vector<NodeIdx> follow_idx_;

  // Save the destination of merged nodes.
  StableHashMap<NodeIdx, NodeIdx> merged_to_;
  // Save pairs that need to be merged.
  std::vector<std::pair<NodeIdx, NodeIdx>> to_merge_pairs_;
};

// Get the final sharding strategy according to the ilp solution.
inline const ShardingStrategy& GetShardingStrategy(
    const HloInstruction* inst, const StrategyMap& strategy_map,
    const CostGraph& cost_graph, absl::Span<const NodeStrategyIdx> s_val) {
  const StrategyGroup* strategy_group = strategy_map.at(inst).get();
  CHECK(!strategy_group->is_tuple);
  NodeIdx node_idx = strategy_group->node_idx;
  NodeStrategyIdx stra_idx = cost_graph.RemapIndex(node_idx, s_val[node_idx]);
  return strategy_group->strategies[stra_idx];
}

// Get the final sharding strategy according to the ilp solution.
inline const ShardingStrategy& GetShardingStrategyForTuple(
    const HloInstruction* inst, ShapeIndex index,
    const StrategyMap& strategy_map, const CostGraph& cost_graph,
    absl::Span<const NodeStrategyIdx> s_val) {
  const StrategyGroup* strategy_group = strategy_map.at(inst).get();
  CHECK(strategy_group->is_tuple);
  for (auto index_element : index) {
    CHECK_LT(index_element, strategy_group->childs.size());
    const auto& strategies = strategy_group->childs[index_element];
    strategy_group = strategies.get();
  }
  NodeIdx node_idx = strategy_group->node_idx;
  NodeStrategyIdx stra_idx = cost_graph.RemapIndex(node_idx, s_val[node_idx]);
  return strategy_group->strategies[stra_idx];
}

}  // namespace spmd
}  // namespace xla
#endif  // XLA_HLO_EXPERIMENTAL_AUTO_SHARDING_AUTO_SHARDING_COST_GRAPH_H_
