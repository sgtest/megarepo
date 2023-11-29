/*Copyright 2022 The TensorFlow Authors.All Rights Reserved.

Licensed under the Apache License, Version 2.0(the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
==============================================================================*/

#include <cstddef>
#include <cstdint>
#include <functional>
#include <memory>
#include <optional>
#include <string>
#include <tuple>
#include <vector>

#include "absl/algorithm/container.h"
#include "absl/log/check.h"
#include "absl/log/log.h"
#include "absl/strings/str_format.h"
#include "absl/strings/str_join.h"
#include "absl/types/span.h"
#include "xla/array.h"
#include "xla/hlo/experimental/auto_sharding/auto_sharding.h"
#include "xla/hlo/experimental/auto_sharding/auto_sharding_option.h"
#include "xla/hlo/experimental/auto_sharding/auto_sharding_strategy.h"
#include "xla/hlo/experimental/auto_sharding/auto_sharding_util.h"
#include "xla/hlo/experimental/auto_sharding/cluster_environment.h"
#include "xla/hlo/ir/hlo_casting_utils.h"
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/hlo/ir/hlo_instructions.h"
#include "xla/hlo/ir/hlo_opcode.h"
#include "xla/hlo/ir/hlo_sharding.h"
#include "xla/service/call_graph.h"
#include "xla/service/dot_as_convolution_util.h"
#include "xla/service/sharding_propagation.h"
#include "xla/status.h"
#include "tsl/platform/errors.h"

namespace xla {
namespace spmd {
namespace {

using DimMap = StableHashMap</*tensor dim*/ int, /* mesh dim*/ int>;
using MeshDims = absl::Span<const int64_t>;

struct Enumeration {
  MeshDims mesh_dims;
  int64_t i;
  int64_t j;
};

// Contains base functionality common to both DotHandler and ConvHandler.
class HandlerBase {
 protected:
  HandlerBase(std::unique_ptr<StrategyGroup>& strategy_group,
              StrategyMap& strategy_map, const HloInstruction* ins,
              const ClusterEnvironment& cluster_env,
              const InstructionBatchDimMap& batch_map,
              const AutoShardingOption& option, const CallGraph& call_graph)
      : strategy_group_(strategy_group),
        strategy_map_(strategy_map),
        ins_(ins),
        cluster_env_(cluster_env),
        batch_map_(batch_map),
        option_(option),
        call_graph_(call_graph),
        device_mesh_(cluster_env.device_mesh_),
        device_mesh_1d_(cluster_env.device_mesh_1d_),
        lhs_(ins->operand(0)),
        rhs_(ins->operand(1)) {}

  void AppendNewStrategy(const std::string& name,
                         const HloSharding& output_spec,
                         absl::Span<const HloSharding> input_specs,
                         double compute_cost, double communication_cost);

  bool CheckDims(const HloInstruction* ins, const DimMap& dim_map) const {
    for (const auto& [tensor_dim, mesh_dim] : dim_map) {
      auto shape_dim = ins->shape().dimensions().at(tensor_dim);
      auto device_mesh_dim = device_mesh_.dim(mesh_dim);
      if (shape_dim < device_mesh_dim) return false;
      if (option_.only_allow_divisible_intermediate &&
          !IsDivisible(shape_dim, device_mesh_dim))
        return false;
    }
    return true;
  }

  HloSharding CreateInputSpec(const HloInstruction* ins, const DimMap& dim_map,
                              const Array<int64_t>& device_mesh) const {
    if (dim_map.empty()) return HloSharding::Replicate();
    std::vector<int64_t> tensor_dims, mesh_dims;
    for (const auto& [tensor_dim, mesh_dim] : dim_map) {
      tensor_dims.push_back(tensor_dim);
      mesh_dims.push_back(mesh_dim);
    }
    return Tile(ins->shape(), tensor_dims, mesh_dims, device_mesh);
  }

  // Given lhs and rhs dim maps, infers a sharding for the output by relying on
  // the sharding_propagation pass.
  void MaybeAppend(
      const std::string& name, const DimMap& lhs_dim_map,
      const DimMap& rhs_dim_map,
      const std::optional<DimMap>& expected_output_dim_map,
      const Array<int64_t>& device_mesh, double compute_cost = 0,
      const std::optional<std::function<double(const HloSharding&)>>&
          communication_cost_fn = std::nullopt);

  std::optional<HloSharding> GetShardingFromUser(const HloSharding& lhs_spec,
                                                 const HloSharding& rhs_spec);

  // Enumerates combinations of the given mesh + tensor dimensions.
  void Enumerate(std::function<void(const Enumeration&)> split_func,
                 size_t num_outer_dims = 2, size_t num_inner_dims = 2,
                 bool half = false) {
    absl::Span<const int64_t> mesh_shape = device_mesh_.dimensions();
    for (int64_t dim0 = 0; dim0 < mesh_shape.size(); ++dim0) {
      for (int64_t dim1 = 0; dim1 < mesh_shape.size(); ++dim1) {
        if (dim0 == dim1) continue;
        for (int64_t i = 0; i < num_outer_dims; ++i) {
          for (int64_t j = half ? i + 1 : 0; j < num_inner_dims; ++j) {
            split_func({{dim0, dim1}, i, j});
          }
        }
      }
    }
  }

  // Enumerates *half* of the combinations (if inner & outer dims are the same).
  void EnumerateHalf(std::function<void(const Enumeration&)> split_func,
                     size_t num_outer_dims = 2, size_t num_inner_dims = 2) {
    Enumerate(split_func, num_outer_dims, num_inner_dims, true);
  }

  std::unique_ptr<StrategyGroup>& strategy_group_;
  StrategyMap& strategy_map_;
  const HloInstruction* ins_;
  const ClusterEnvironment& cluster_env_;
  const InstructionBatchDimMap& batch_map_;
  const AutoShardingOption& option_;
  const CallGraph& call_graph_;

  const Array<int64_t>& device_mesh_;
  const Array<int64_t>& device_mesh_1d_;
  const HloInstruction* lhs_;
  const HloInstruction* rhs_;
};

class DotHandler : public HandlerBase {
 public:
  DotHandler(std::unique_ptr<StrategyGroup>& strategy_group,
             StrategyMap& strategy_map, const HloDotInstruction* ins,
             const ClusterEnvironment& cluster_env,
             const InstructionBatchDimMap& batch_map,
             const AutoShardingOption& option, const CallGraph& call_graph);

  DotHandler(
      std::unique_ptr<StrategyGroup>& strategy_group, StrategyMap& strategy_map,
      const HloConvolutionInstruction* ins,
      const dot_as_convolution_util::DotConvolutionDimsInfo& conv_as_dot_dims,
      const ClusterEnvironment& cluster_env,
      const InstructionBatchDimMap& batch_map, const AutoShardingOption& option,
      const CallGraph& call_graph);

  void SplitLhsSpaceRhsSpace();

  void SplitLhsSpaceOnly();

  void SplitRhsSpaceOnly();

  void SplitLhsSpaceBothContract();

  void SplitRhsSpaceBothContract();

  void SplitOneBatchDim();

  void SplitTwoBatchDims();

  void SplitBatchDimLhsSpace();

  void SplitBatchDimRhsSpace();

  void SplitBatchDimBothContract();

  void SplitBothContractTwoDims();

  void RecomputeSplitBothContract();

  void Add1DDataParallel();

  void Add1DBatchSplit();

  Status RegisterStrategies();

  // Dimension information
  bool is_dot_;
  int64_t space_base_dim_;
  tsl::protobuf::RepeatedField<int64_t> lhs_space_dims_, rhs_space_dims_;
  tsl::protobuf::RepeatedField<int64_t> lhs_con_dims_;
  tsl::protobuf::RepeatedField<int64_t> rhs_con_dims_;
  tsl::protobuf::RepeatedField<int64_t> lhs_batch_dims_;
  tsl::protobuf::RepeatedField<int64_t> rhs_batch_dims_;
};

class ConvHandler : public HandlerBase {
 public:
  ConvHandler(std::unique_ptr<StrategyGroup>& strategy_group,
              StrategyMap& strategy_map, const HloInstruction* ins,
              const ClusterEnvironment& cluster_env,
              const InstructionBatchDimMap& batch_map,
              const AutoShardingOption& option, const CallGraph& call_graph);

  void SplitLhsBatchRhsOutchannel();

  void SplitLhsBatchBothInchannel();

  void SplitRhsOutchannelBothInchannel();

  void Add1DDataParallel();

  void SplitDepthwise(bool forward);

  Status RegisterStrategies();

  // Dimension information
  const ConvolutionDimensionNumbers& conv_dnums_;
  int64_t lhs_batch_dim_, lhs_in_channel_dim_;
  int64_t rhs_in_channel_dim_, rhs_out_channel_dim_;
  int64_t out_batch_dim_, out_out_channel_dim_;
};

/************** HandlerBase function definitions **************/

void HandlerBase::AppendNewStrategy(const std::string& name,
                                    const HloSharding& output_spec,
                                    absl::Span<const HloSharding> input_specs,
                                    double compute_cost,
                                    double communication_cost) {
  std::vector<std::vector<double>> resharding_costs;

  for (int i = 0; i < ins_->operand_count(); ++i) {
    const HloInstruction* operand = ins_->operand(i);
    resharding_costs.push_back(
        ReshardingCostVector(strategy_map_.at(operand).get(), operand->shape(),
                             input_specs[i], cluster_env_));
  }

  strategy_group_->strategies.push_back(ShardingStrategy({
      name,
      output_spec,
      compute_cost,
      communication_cost,
      GetBytes(ins_->shape()) / output_spec.NumTiles(),
      resharding_costs,
      {input_specs.begin(), input_specs.end()},
  }));
}

// Given lhs and rhs dim maps, infers a sharding for the output by relying on
// the sharding_propagation pass. Given that this is a relatively new change
// (as of 11/2023), we also take an optional expected output dim map as an
// argument, to verify that sharding propagation in fact infers the sharding
// we expect (and to crash if it doesn't).
// TODO(b/309638633) As we build more confidence in this, we should remove
// this expected_output_dim_map argument and fully rely on sharding
// propagation.
void HandlerBase::MaybeAppend(
    const std::string& name, const DimMap& lhs_dim_map,
    const DimMap& rhs_dim_map,
    const std::optional<DimMap>& expected_output_dim_map,
    const Array<int64_t>& device_mesh, double compute_cost,
    const std::optional<std::function<double(const HloSharding&)>>&
        communication_cost_fn) {
  HloSharding lhs_spec = CreateInputSpec(lhs_, lhs_dim_map, device_mesh);
  HloSharding rhs_spec = CreateInputSpec(rhs_, rhs_dim_map, device_mesh);
  if (std::optional<HloSharding> output_spec =
          GetShardingFromUser(lhs_spec, rhs_spec);
      output_spec.has_value()) {
    if (expected_output_dim_map.has_value()) {
      HloSharding expected_output_spec =
          CreateInputSpec(ins_, *expected_output_dim_map, device_mesh);
      // TODO(b/308687597) Once the bug is resolved, we ideally either want
      // have a CHECK statement verifying that the sharding inferred by
      // sharding propagation is in fact what we expect, or we trust sharding
      // propagation's results without the check. b/308687597 currently
      // prevents us from doing so. AutoShardingTest.LargeSize in
      // //third_party/tensorflow/compiler/xla/hlo/experimental/auto_sharding:auto_sharding_test
      // currently fails due to the issue.
      if (ins_->opcode() == HloOpcode::kDot &&
          *output_spec != expected_output_spec) {
        output_spec = expected_output_spec;
        LOG(ERROR)
            << "The sharding inferred by sharding propagation in this case "
               "does not match the expected sharding for the dot "
               "instruction. This may be related to b/308687597. Given this "
               "mismatch, we continue with the expected sharding";
      }
    }
    double communication_cost = 0;
    if (communication_cost_fn.has_value()) {
      communication_cost = communication_cost_fn.value()(*output_spec);
    }
    AppendNewStrategy(name, *output_spec, {lhs_spec, rhs_spec}, compute_cost,
                      communication_cost);
  } else {
    LOG(FATAL) << "Sharding propagation could not infer output sharding";
  }
}

std::optional<HloSharding> HandlerBase::GetShardingFromUser(
    const HloSharding& lhs_spec, const HloSharding& rhs_spec) {
  std::unique_ptr<HloInstruction> ins_clone = ins_->Clone();
  std::unique_ptr<HloInstruction> lhs_clone = lhs_->Clone();
  std::unique_ptr<HloInstruction> rhs_clone = rhs_->Clone();
  ins_clone->clear_sharding();
  lhs_clone->set_sharding(lhs_spec);
  rhs_clone->set_sharding(rhs_spec);
  CHECK_OK(ins_clone->ReplaceOperandWith(0, lhs_clone.get()));
  CHECK_OK(ins_clone->ReplaceOperandWith(1, rhs_clone.get()));
  if (ins_->opcode() == HloOpcode::kConvolution) {
    xla::InferConvolutionShardingFromOperands(
        ins_clone.get(), call_graph_, 10,
        /* may_combine_partial_sharding */ true, /* is_spmd */ true);
  } else {
    xla::InferDotShardingFromOperands(
        ins_clone.get(), call_graph_,
        dot_as_convolution_util::ParseDotGeneralFromDot(ins_clone.get()),
        /* may_combine_partial_sharding/ */ true, /* is_spmd */ true);
  }
  if (!ins_clone->has_sharding()) {
    return std::nullopt;
  }
  return ins_clone->sharding();
}

/************** DotHandler function definitions **************/

DotHandler::DotHandler(std::unique_ptr<StrategyGroup>& strategy_group,
                       StrategyMap& strategy_map, const HloDotInstruction* ins,
                       const ClusterEnvironment& cluster_env,
                       const InstructionBatchDimMap& batch_map,
                       const AutoShardingOption& option,
                       const CallGraph& call_graph)
    : HandlerBase(strategy_group, strategy_map, ins, cluster_env, batch_map,
                  option, call_graph),
      is_dot_(true),
      space_base_dim_(ins->dot_dimension_numbers().lhs_batch_dimensions_size()),
      lhs_con_dims_(ins->dot_dimension_numbers().lhs_contracting_dimensions()),
      rhs_con_dims_(ins->dot_dimension_numbers().rhs_contracting_dimensions()),
      lhs_batch_dims_(ins->dot_dimension_numbers().lhs_batch_dimensions()),
      rhs_batch_dims_(ins->dot_dimension_numbers().rhs_batch_dimensions()) {
  std::tie(lhs_space_dims_, rhs_space_dims_) =
      GetSpaceDims(lhs_->shape(), rhs_->shape(), ins->dot_dimension_numbers());
  CHECK_EQ(lhs_con_dims_.size(), rhs_con_dims_.size());
  CHECK_EQ(lhs_batch_dims_.size(), rhs_batch_dims_.size());
}

DotHandler::DotHandler(
    std::unique_ptr<StrategyGroup>& strategy_group, StrategyMap& strategy_map,
    const HloConvolutionInstruction* ins,
    const dot_as_convolution_util::DotConvolutionDimsInfo& conv_as_dot_dims,
    const ClusterEnvironment& cluster_env,
    const InstructionBatchDimMap& batch_map, const AutoShardingOption& option,
    const CallGraph& call_graph)
    : HandlerBase(strategy_group, strategy_map, ins, cluster_env, batch_map,
                  option, call_graph),
      is_dot_(false),
      space_base_dim_(-1) {
  CHECK(conv_as_dot_dims.conv_spatial_dims.empty());

  for (auto dim_idx : conv_as_dot_dims.batch_dims) {
    if (dim_idx.lhs >= 0) lhs_batch_dims_.Add(dim_idx.lhs);
    if (dim_idx.rhs >= 0) rhs_batch_dims_.Add(dim_idx.rhs);
  }

  for (auto dim_idx : conv_as_dot_dims.contracting_dims) {
    if (dim_idx.lhs >= 0) lhs_con_dims_.Add(dim_idx.lhs);
    if (dim_idx.rhs >= 0) rhs_con_dims_.Add(dim_idx.rhs);
  }

  for (auto dim_idx : conv_as_dot_dims.lhs_non_contracting_dims) {
    if (dim_idx.lhs >= 0) lhs_space_dims_.Add(dim_idx.lhs);
  }

  for (auto dim_idx : conv_as_dot_dims.rhs_non_contracting_dims) {
    if (dim_idx.rhs >= 0) rhs_space_dims_.Add(dim_idx.rhs);
  }
}

void DotHandler::SplitLhsSpaceRhsSpace() {
  auto func = [this](const Enumeration& e) {
    const DimMap lhs_dim_map = {{lhs_space_dims_[e.i], e.mesh_dims[0]}};
    const DimMap rhs_dim_map = {{rhs_space_dims_[e.j], e.mesh_dims[1]}};
    std::string name =
        absl::StrFormat("SS = SR x RS @ {%s}", absl::StrJoin(e.mesh_dims, ","));

    std::optional<DimMap> out_dim_map = std::nullopt;
    if (is_dot_) {
      out_dim_map = DimMap{
          {space_base_dim_ + e.i, e.mesh_dims[0]},
          {space_base_dim_ + static_cast<int64_t>(lhs_space_dims_.size()) + e.j,
           e.mesh_dims[1]}};
    }
    MaybeAppend(name, lhs_dim_map, rhs_dim_map, out_dim_map, device_mesh_);
  };
  Enumerate(func, lhs_space_dims_.size(), rhs_space_dims_.size());
}

void DotHandler::SplitLhsSpaceOnly() {
  auto func = [this](const Enumeration& e) {
    const DimMap lhs_dim_map = {{lhs_space_dims_[e.i], e.mesh_dims[0]},
                                {lhs_space_dims_[e.j], e.mesh_dims[1]}};
    std::string name = absl::StrFormat("SSR = SSR x RR @ {%s}",
                                       absl::StrJoin(e.mesh_dims, ","));
    std::optional<DimMap> out_dim_map = std::nullopt;
    if (is_dot_) {
      out_dim_map = DimMap{{space_base_dim_ + e.i, e.mesh_dims[0]},
                           {space_base_dim_ + e.j, e.mesh_dims[1]}};
    }
    MaybeAppend(name, lhs_dim_map, {}, out_dim_map, device_mesh_);
  };
  EnumerateHalf(func, lhs_space_dims_.size(), lhs_space_dims_.size());
}

void DotHandler::SplitRhsSpaceOnly() {
  auto func = [this](const Enumeration& e) {
    const DimMap rhs_dim_map = {{rhs_space_dims_[e.i], e.mesh_dims[0]},
                                {rhs_space_dims_[e.j], e.mesh_dims[1]}};
    std::string name = absl::StrFormat("RSS = RR x RSS @ {%s}",
                                       absl::StrJoin(e.mesh_dims, ","));
    std::optional<DimMap> out_dim_map = std::nullopt;
    if (is_dot_) {
      out_dim_map = DimMap{
          {space_base_dim_ + static_cast<int64_t>(lhs_space_dims_.size()) + e.i,
           e.mesh_dims[0]},
          {space_base_dim_ + static_cast<int64_t>(lhs_space_dims_.size()) + e.j,
           e.mesh_dims[1]}};
    }
    MaybeAppend(name, {}, rhs_dim_map, out_dim_map, device_mesh_);
  };
  EnumerateHalf(func, rhs_space_dims_.size(), rhs_space_dims_.size());
}

void DotHandler::SplitLhsSpaceBothContract() {
  auto func = [this](const Enumeration& e) {
    if (device_mesh_.dim(e.mesh_dims[0]) <= 1 ||
        device_mesh_.dim(e.mesh_dims[1]) <= 1)
      return;
    std::string name =
        absl::StrFormat("SR = SS x SR @ {%s} (allreduce @ %d)",
                        absl::StrJoin(e.mesh_dims, ","), e.mesh_dims[1]);
    const DimMap lhs_dim_map = {{lhs_space_dims_[e.i], e.mesh_dims[0]},
                                {lhs_con_dims_[e.j], e.mesh_dims[1]}};
    const DimMap rhs_dim_map = {{rhs_con_dims_[e.j], e.mesh_dims[1]}};
    std::optional<DimMap> out_dim_map = std::nullopt;
    if (is_dot_) {
      out_dim_map = DimMap{{space_base_dim_ + e.i, e.mesh_dims[0]}};
    }

    auto communication_cost_fn = [this, &e](const HloSharding& output_spec) {
      double memory_cost = GetBytes(ins_->shape()) / output_spec.NumTiles();
      return cluster_env_.AllReduceCost(memory_cost, e.mesh_dims[1]);
    };
    MaybeAppend(name, lhs_dim_map, rhs_dim_map, out_dim_map, device_mesh_, 0,
                communication_cost_fn);
  };
  Enumerate(func, lhs_space_dims_.size(), lhs_con_dims_.size());
}

void DotHandler::SplitRhsSpaceBothContract() {
  auto func = [this](const Enumeration& e) {
    if (device_mesh_.dim(e.mesh_dims[0]) <= 1) return;
    std::string name =
        absl::StrFormat("RS = RS x SS @ {%s} (allreduce @ %d)",
                        absl::StrJoin(e.mesh_dims, ","), e.mesh_dims[0]);
    const DimMap rhs_dim_map = {{rhs_space_dims_[e.i], e.mesh_dims[1]},
                                {rhs_con_dims_[e.j], e.mesh_dims[0]}};
    const DimMap lhs_dim_map = {{lhs_con_dims_[e.j], e.mesh_dims[0]}};
    std::optional<DimMap> out_dim_map = std::nullopt;
    if (is_dot_) {
      out_dim_map = DimMap{
          {space_base_dim_ + static_cast<int64_t>(lhs_space_dims_.size()) + e.i,
           e.mesh_dims[1]}};
    }
    auto communication_cost_fn = [this, &e](const HloSharding& output_spec) {
      double memory_cost = GetBytes(ins_->shape()) / output_spec.NumTiles();
      return cluster_env_.AllReduceCost(memory_cost, e.mesh_dims[0]);
    };
    MaybeAppend(name, lhs_dim_map, rhs_dim_map, out_dim_map, device_mesh_, 0,
                communication_cost_fn);
  };
  Enumerate(func, rhs_space_dims_.size(), lhs_con_dims_.size());
}

void DotHandler::SplitOneBatchDim() {
  if (absl::c_count_if(device_mesh_.dimensions(),
                       [](int64_t size) { return size > 1; }) != 1) {
    return;
  }
  auto func = [this](const Enumeration& e) {
    const DimMap lhs_dim_map = {{lhs_batch_dims_[e.i], e.j}};
    const DimMap rhs_dim_map = {{rhs_batch_dims_[e.i], e.j}};
    std::string name = absl::StrFormat("Sb_%d = Sb x Sb @ {%d}", e.i, e.j);
    std::optional<DimMap> out_dim_map = std::nullopt;
    if (is_dot_) {
      out_dim_map = DimMap{{e.i, e.j}};
    }
    MaybeAppend(name, lhs_dim_map, rhs_dim_map, out_dim_map, device_mesh_);
  };
  Enumerate(func, lhs_batch_dims_.size(), device_mesh_.num_dimensions());
}

void DotHandler::SplitTwoBatchDims() {
  if (lhs_batch_dims_.size() != 2) return;
  auto func = [this](const Enumeration& e) {
    if (device_mesh_.dim(e.mesh_dims[0]) <= 1 ||
        device_mesh_.dim(e.mesh_dims[1]) <= 1)
      return;
    const DimMap lhs_dim_map = {{lhs_batch_dims_[0], e.mesh_dims[0]},
                                {lhs_batch_dims_[1], e.mesh_dims[1]}};
    const DimMap rhs_dim_map = {{rhs_batch_dims_[0], e.mesh_dims[0]},
                                {rhs_batch_dims_[1], e.mesh_dims[1]}};
    std::string name =
        absl::StrFormat("Sb = Sb x Sb @ {%s}", absl::StrJoin(e.mesh_dims, ","));
    std::optional<DimMap> out_dim_map = std::nullopt;
    if (is_dot_) {
      out_dim_map = DimMap{{0, e.mesh_dims[0]}, {1, e.mesh_dims[1]}};
    }
    MaybeAppend(name, lhs_dim_map, rhs_dim_map, out_dim_map, device_mesh_);
  };
  EnumerateHalf(func, lhs_batch_dims_.size(), lhs_batch_dims_.size());
}

void DotHandler::SplitBatchDimLhsSpace() {
  if (lhs_batch_dims_.empty()) return;
  auto func = [this](const Enumeration& e) {
    if (device_mesh_.dim(e.mesh_dims[0]) <= 1 ||
        device_mesh_.dim(e.mesh_dims[1]) <= 1)
      return;
    std::string name = absl::StrFormat("SbSi = SbSi x SbR @ {%s}",
                                       absl::StrJoin(e.mesh_dims, ","));
    const DimMap lhs_dim_map = {{lhs_space_dims_[e.i], e.mesh_dims[1]},
                                {lhs_batch_dims_[e.j], e.mesh_dims[0]}};
    const DimMap rhs_dim_map = {{rhs_batch_dims_[e.j], e.mesh_dims[0]}};
    std::optional<DimMap> out_dim_map = std::nullopt;
    if (is_dot_) {
      out_dim_map = DimMap{{e.j, e.mesh_dims[0]},
                           {space_base_dim_ + e.i, e.mesh_dims[1]}};
    }
    MaybeAppend(name, lhs_dim_map, rhs_dim_map, out_dim_map, device_mesh_);
  };
  Enumerate(func, lhs_space_dims_.size(), lhs_batch_dims_.size());
}

void DotHandler::SplitBatchDimRhsSpace() {
  if (lhs_batch_dims_.empty()) return;
  auto func = [this](const Enumeration& e) {
    if (device_mesh_.dim(e.mesh_dims[0]) <= 1 ||
        device_mesh_.dim(e.mesh_dims[1]) <= 1)
      return;
    std::string name = absl::StrFormat("SbSj = SbR x SbSj @ {%s}",
                                       absl::StrJoin(e.mesh_dims, ","));
    const DimMap rhs_dim_map = {{rhs_space_dims_[e.i], e.mesh_dims[1]},
                                {rhs_batch_dims_[e.j], e.mesh_dims[0]}};
    const DimMap lhs_dim_map = {{lhs_batch_dims_[e.j], e.mesh_dims[0]}};
    std::optional<DimMap> out_dim_map = std::nullopt;
    if (is_dot_) {
      out_dim_map = DimMap{
          {e.j, e.mesh_dims[0]},
          {space_base_dim_ + static_cast<int64_t>(lhs_space_dims_.size()) + e.i,
           e.mesh_dims[1]}};
    }
    MaybeAppend(name, lhs_dim_map, rhs_dim_map, out_dim_map, device_mesh_);
  };
  Enumerate(func, rhs_space_dims_.size(), lhs_batch_dims_.size());
}

void DotHandler::SplitBatchDimBothContract() {
  if (lhs_batch_dims_.empty()) return;
  auto func = [this](const Enumeration& e) {
    if (device_mesh_.dim(e.mesh_dims[0]) <= 1 ||
        device_mesh_.dim(e.mesh_dims[1]) <= 1)
      return;
    std::string name =
        absl::StrFormat("SbR = SbSk x SbSk @ {%s} (allreduce @ %d}",
                        absl::StrJoin(e.mesh_dims, ","), e.mesh_dims[1]);
    const DimMap lhs_dim_map = {{lhs_con_dims_[e.i], e.mesh_dims[1]},
                                {lhs_batch_dims_[e.j], e.mesh_dims[0]}};
    const DimMap rhs_dim_map = {{rhs_batch_dims_[e.j], e.mesh_dims[0]}};
    std::optional<DimMap> out_dim_map = std::nullopt;
    if (is_dot_) {
      out_dim_map = DimMap{{e.j, e.mesh_dims[0]}};
    }
    auto communication_cost_fn = [this, &e](const HloSharding& output_spec) {
      double memory_cost = GetBytes(ins_->shape()) / output_spec.NumTiles();
      return cluster_env_.AllReduceCost(memory_cost, e.mesh_dims[1]);
    };
    MaybeAppend(name, lhs_dim_map, rhs_dim_map, out_dim_map, device_mesh_, 0,
                communication_cost_fn);
  };
  Enumerate(func, lhs_con_dims_.size(), lhs_batch_dims_.size());
}

void DotHandler::SplitBothContractTwoDims() {
  if (lhs_con_dims_.size() < 2 || rhs_con_dims_.size() < 2) return;
  auto func = [this](const Enumeration& e) {
    // Applies when there are more than one contracting dimension.
    if (device_mesh_.dim(e.mesh_dims[0]) <= 1 ||
        device_mesh_.dim(e.mesh_dims[1]) <= 1)
      return;
    std::string name = absl::StrFormat("RR = SS x SS @ {%s} (allreduce @ {%s}}",
                                       absl::StrJoin(e.mesh_dims, ","),
                                       absl::StrJoin(e.mesh_dims, ", "));
    const DimMap lhs_dim_map = {{lhs_con_dims_[e.i], e.mesh_dims[0]},
                                {lhs_con_dims_[e.j], e.mesh_dims[1]}};
    const DimMap rhs_dim_map = {{rhs_con_dims_[e.i], e.mesh_dims[0]},
                                {rhs_con_dims_[e.j], e.mesh_dims[1]}};
    std::optional<DimMap> out_dim_map = std::nullopt;
    if (is_dot_) {
      out_dim_map = DimMap{};
    }
    auto communication_cost_fn = [this, &e](const HloSharding& output_spec) {
      double memory_cost = GetBytes(ins_->shape()) / output_spec.NumTiles();
      return cluster_env_.AllReduceCost(memory_cost, e.mesh_dims[0],
                                        e.mesh_dims[1]);
    };
    MaybeAppend(name, lhs_dim_map, rhs_dim_map, out_dim_map, device_mesh_, 0,
                communication_cost_fn);
  };
  EnumerateHalf(func, lhs_con_dims_.size(), lhs_con_dims_.size());
}

void DotHandler::RecomputeSplitBothContract() {
  auto func = [this](const Enumeration& e) {
    if (device_mesh_.dim(e.mesh_dims[0]) <= 1 ||
        device_mesh_.dim(e.mesh_dims[1]) <= 1)
      return;
    std::string name = absl::StrFormat("RR = RS x SR @ {%d} (allreduce @ %d)",
                                       e.mesh_dims[0], e.mesh_dims[0]);
    const DimMap lhs_dim_map = {{lhs_con_dims_[e.i], e.mesh_dims[0]}};
    const DimMap rhs_dim_map = {{rhs_con_dims_[e.i], e.mesh_dims[0]}};
    std::optional<DimMap> out_dim_map = std::nullopt;
    if (is_dot_) {
      out_dim_map = DimMap{};
    }
    double compute_cost = cluster_env_.DotCost(lhs_->shape(), rhs_->shape());
    auto communication_cost_fn = [this, &e](const HloSharding& output_spec) {
      double memory_cost = GetBytes(ins_->shape()) / output_spec.NumTiles();
      return cluster_env_.AllReduceCost(memory_cost, e.mesh_dims[0]);
    };
    MaybeAppend(name, lhs_dim_map, rhs_dim_map, out_dim_map, device_mesh_,
                compute_cost, communication_cost_fn);
  };
  Enumerate(func, lhs_con_dims_.size(), 1);
}

void DotHandler::Add1DDataParallel() {
  if (device_mesh_.dim(0) > 1 &&
      absl::c_count_if(device_mesh_.dimensions(),
                       [](int64_t size) { return size > 1; }) > 1) {
    int mesh_dim = 0;
    int64_t num_devices = device_mesh_1d_.dim(mesh_dim);

    // Si = Si x R @ 0
    for (int64_t i = 0; i < lhs_space_dims_.size(); ++i) {
      const DimMap lhs_dim_map = {{lhs_space_dims_[i], mesh_dim}};
      if (lhs_->shape().dimensions(lhs_space_dims_[i]) < num_devices) {
        continue;
      }
      if (option_.only_allow_divisible_intermediate &&
          !IsDivisible(lhs_->shape().dimensions(lhs_space_dims_[i]),
                       num_devices)) {
        continue;
      }
      std::string name = absl::StrFormat("Si = Si x R @ %d", mesh_dim);
      std::optional<DimMap> out_dim_map = std::nullopt;
      if (is_dot_) {
        out_dim_map = DimMap{{space_base_dim_ + i, mesh_dim}};
      }
      MaybeAppend(name, lhs_dim_map, {}, out_dim_map, device_mesh_1d_);
    }

    // R = Sk x Sk @ (allreduce @ 0)
    for (int64_t i = 0; i < lhs_con_dims_.size(); ++i) {
      const DimMap lhs_dim_map = {{lhs_con_dims_[i], mesh_dim}};
      const DimMap rhs_dim_map = {{rhs_con_dims_[i], mesh_dim}};
      if (lhs_->shape().dimensions(lhs_con_dims_[i]) < num_devices) {
        continue;
      }
      if (option_.only_allow_divisible_intermediate &&
          !IsDivisible(lhs_->shape().dimensions(lhs_con_dims_[i]),
                       num_devices)) {
        continue;
      }
      std::string name = absl::StrFormat("R = Sk x Sk @ %d (allreduce @ %d)",
                                         mesh_dim, mesh_dim);
      std::optional<DimMap> out_dim_map = std::nullopt;
      if (is_dot_) {
        out_dim_map = DimMap{};
      }
      auto communication_cost_fn = [this,
                                    mesh_dim](const HloSharding& output_spec) {
        double memory_cost = GetBytes(ins_->shape()) / output_spec.NumTiles();
        return cluster_env_.AllReduceCost(memory_cost, mesh_dim);
      };
      MaybeAppend(name, lhs_dim_map, rhs_dim_map, out_dim_map, device_mesh_1d_,
                  0, communication_cost_fn);
    }
  }
}

void DotHandler::Add1DBatchSplit() {
  if (device_mesh_.dim(0) > 1 &&
      absl::c_count_if(device_mesh_.dimensions(),
                       [](int64_t size) { return size > 1; }) > 1) {
    int mesh_dim = 0;
    for (int64_t i = 0; i < lhs_batch_dims_.size(); ++i) {
      const DimMap lhs_dim_map = {{lhs_batch_dims_[i], mesh_dim}};
      const DimMap rhs_dim_map = {{rhs_batch_dims_[i], mesh_dim}};
      std::string name =
          absl::StrFormat("Sb_%d = Sb x Sb @ {%d} 1d", i, mesh_dim);
      std::optional<DimMap> out_dim_map = std::nullopt;
      if (is_dot_) {
        out_dim_map = DimMap{{i, mesh_dim}};
      }
      MaybeAppend(name, lhs_dim_map, rhs_dim_map, out_dim_map, device_mesh_1d_);
    }
  }
}

Status DotHandler::RegisterStrategies() {
  // SS = SR x RS
  // Split lhs space dim and rhs space dim.
  SplitLhsSpaceRhsSpace();

  // SSR = SSR x RR
  // Split lhs space dims only if it has more than 1 space dims.
  if (lhs_space_dims_.size() > 1) {
    SplitLhsSpaceOnly();
  }
  // RSS = RR x RSS
  // Split rhs space dims only if it has more than 1 space dims.
  if (rhs_space_dims_.size() > 1) {
    SplitRhsSpaceOnly();
  }

  // SR = SS x SR
  // Split lhs space dim and both contracting dims.
  SplitLhsSpaceBothContract();

  // RS = RS x SS
  // Split rhs space dim and both contracting dims.
  SplitRhsSpaceBothContract();

  // RR = SS x SS
  // Split two contracting dims on lhs and rhs.
  SplitBothContractTwoDims();

  // RR = RS x SR
  // This is a special case where we allow splitting only one dim in the
  // multi-dimensional mesh case. This allows some recomputation
  // (e.g., the dense layer in the LM_head of BERT).
  RecomputeSplitBothContract();

  // Add 1d data parallel in multi-dimensional mesh
  if (option_.allow_mixed_mesh_shape) {
    Add1DDataParallel();
  }

  if (option_.batch_matmul_always_split_batch && !lhs_batch_dims_.empty() &&
      cluster_env_.non_zero_mesh_dims_.size() > 1) {
    // If there is a batch dim and the device mesh is multi-dimensional,
    // always split on batch dim. Clear all old strategies.
    strategy_group_->strategies.clear();
  }

  // Sb = Sb x Sb
  // Split one batch dim. Only used for 1d mesh
  SplitOneBatchDim();

  // SbSi = SbSi x SbR
  // Split batch dim and lhs space dim
  SplitBatchDimLhsSpace();

  // SbSj = SbR x SbSj
  // Split batch dim and rhs space dim
  SplitBatchDimRhsSpace();

  // SbSj = SbR x SbSj
  // Split batch dim and contracting dim
  SplitBatchDimBothContract();

  if (option_.batch_matmul_always_split_batch && lhs_batch_dims_.size() == 2 &&
      absl::c_count_if(device_mesh_.dimensions(),
                       [](int64_t size) { return size > 1; }) > 1) {
    // If there are two batch dims, always split on these two dims.
    // Clear all old strategies.
    strategy_group_->strategies.clear();
  }

  // Sb = Sb x Sb
  // Split batch dims.
  SplitTwoBatchDims();

  if (option_.allow_mixed_mesh_shape) {
    Add1DBatchSplit();
  }

  // If force_batch_dim_to_mesh_dim is set, filter out invalid strategies
  // and only keep the data parallel strategies.
  if (option_.force_batch_dim_to_mesh_dim >= 0 &&
      batch_map_.contains(GetBatchDimMapKey(ins_))) {
    TF_RETURN_IF_ERROR(FilterStrategy(ins_, ins_->shape(), strategy_group_,
                                      cluster_env_, batch_map_, option_));
  }

  return OkStatus();
}

/************** ConvHandler function definitions **************/

ConvHandler::ConvHandler(std::unique_ptr<StrategyGroup>& strategy_group,
                         StrategyMap& strategy_map, const HloInstruction* ins,
                         const ClusterEnvironment& cluster_env,
                         const InstructionBatchDimMap& batch_map,
                         const AutoShardingOption& option,
                         const CallGraph& call_graph)
    : HandlerBase(strategy_group, strategy_map, ins, cluster_env, batch_map,
                  option, call_graph),
      conv_dnums_(ins->convolution_dimension_numbers()) {
  lhs_batch_dim_ = conv_dnums_.input_batch_dimension();
  lhs_in_channel_dim_ = conv_dnums_.input_feature_dimension();
  rhs_in_channel_dim_ = conv_dnums_.kernel_input_feature_dimension();
  rhs_out_channel_dim_ = conv_dnums_.kernel_output_feature_dimension();
  out_batch_dim_ = conv_dnums_.output_batch_dimension();
  out_out_channel_dim_ = conv_dnums_.output_feature_dimension();
}

Status ConvHandler::RegisterStrategies() {
  // For 1D sharding
  if ((ins_->feature_group_count() ==
           lhs_->shape().dimensions(lhs_in_channel_dim_) &&
       ins_->feature_group_count() ==
           rhs_->shape().dimensions(rhs_out_channel_dim_))) {
    // for depthwise conv
    // SS = SS x S
    // Split batch dim and channel dim
    SplitDepthwise(true);
  } else if ((ins_->batch_group_count() ==
                  lhs_->shape().dimensions(lhs_batch_dim_) &&
              ins_->batch_group_count() ==
                  rhs_->shape().dimensions(rhs_out_channel_dim_))) {
    // for depthwise conv filter_backward
    // SS = SS x S
    // Split batch dim and channel dim
    SplitDepthwise(false);
  }

  // SS = SR x RS
  // Split lhs batch dim and rhs out_channel dim.
  SplitLhsBatchRhsOutchannel();

  // SR = SS x SR
  // Split lhs batch dim and both in_channel dims.
  SplitLhsBatchBothInchannel();

  // RS = RS x SS
  // Split rhs out_channel dim and both in_channel dims.
  SplitRhsOutchannelBothInchannel();

  // Add 1d data parallel in multi-dimensional mesh
  if (option_.allow_mixed_mesh_shape) {
    Add1DDataParallel();
  }

  // If force_batch_dim_to_mesh_dim is set, filter out invalid strategies
  // and only keep the data parallel strategies.
  if (option_.force_batch_dim_to_mesh_dim >= 0 &&
      batch_map_.contains(GetBatchDimMapKey(ins_))) {
    TF_RETURN_IF_ERROR(FilterStrategy(ins_, ins_->shape(), strategy_group_,
                                      cluster_env_, batch_map_, option_));
  }

  return OkStatus();
}

void ConvHandler::SplitLhsBatchRhsOutchannel() {
  auto func = [this](const Enumeration& e) {
    const DimMap lhs_dim_map = {{lhs_batch_dim_, e.mesh_dims[0]}};
    const DimMap rhs_dim_map = {{rhs_out_channel_dim_, e.mesh_dims[1]}};
    std::string name =
        absl::StrFormat("SS = SR x RS @ {%s}", absl::StrJoin(e.mesh_dims, ","));
    const DimMap out_dim_map = {{out_batch_dim_, e.mesh_dims[0]},
                                {out_out_channel_dim_, e.mesh_dims[1]}};
    MaybeAppend(name, lhs_dim_map, rhs_dim_map, out_dim_map, device_mesh_);
  };
  EnumerateHalf(func);
}

void ConvHandler::SplitLhsBatchBothInchannel() {
  auto func = [this](const Enumeration& e) {
    if (device_mesh_.dim(e.mesh_dims[0]) <= 1 ||
        device_mesh_.dim(e.mesh_dims[1]) <= 1)
      return;
    const DimMap lhs_dim_map = {{lhs_batch_dim_, e.mesh_dims[0]},
                                {lhs_in_channel_dim_, e.mesh_dims[1]}};
    const DimMap rhs_dim_map = {{rhs_in_channel_dim_, e.mesh_dims[1]}};
    std::string name =
        absl::StrFormat("SR = SS x SR @ {%s} (allreduce @ %d)",
                        absl::StrJoin(e.mesh_dims, ","), e.mesh_dims[1]);
    const DimMap out_dim_map = {{out_batch_dim_, e.mesh_dims[0]}};
    auto communication_cost_fn = [this, &e](const HloSharding& output_spec) {
      double memory_cost = GetBytes(ins_->shape()) / output_spec.NumTiles();
      return cluster_env_.AllReduceCost(memory_cost, e.mesh_dims[1]);
    };
    MaybeAppend(name, lhs_dim_map, rhs_dim_map, out_dim_map, device_mesh_, 0,
                communication_cost_fn);
  };
  EnumerateHalf(func);
}

void ConvHandler::SplitRhsOutchannelBothInchannel() {
  auto func = [this](const Enumeration& e) {
    if (device_mesh_.dim(e.mesh_dims[0]) <= 1) return;
    const DimMap lhs_dim_map = {{lhs_in_channel_dim_, e.mesh_dims[0]}};
    const DimMap rhs_dim_map = {{rhs_in_channel_dim_, e.mesh_dims[0]},
                                {rhs_out_channel_dim_, e.mesh_dims[1]}};
    std::string name =
        absl::StrFormat("RS = RS x SS @ {%s} (allreduce @ %d)",
                        absl::StrJoin(e.mesh_dims, ","), e.mesh_dims[0]);
    const DimMap out_dim_map = {{out_out_channel_dim_, e.mesh_dims[1]}};
    auto communication_cost_fn = [this, &e](const HloSharding& output_spec) {
      double memory_cost = GetBytes(ins_->shape()) / output_spec.NumTiles();
      return cluster_env_.AllReduceCost(memory_cost, e.mesh_dims[0]);
    };
    MaybeAppend(name, lhs_dim_map, rhs_dim_map, out_dim_map, device_mesh_, 0,
                communication_cost_fn);
  };
  EnumerateHalf(func);
}

void ConvHandler::Add1DDataParallel() {
  if (device_mesh_.dim(0) > 1 &&
      absl::c_count_if(device_mesh_.dimensions(),
                       [](int64_t size) { return size > 1; }) > 1) {
    int mesh_dim = 0;
    int64_t num_devices = device_mesh_1d_.dim(mesh_dim);

    // Si = Si x R @ 0
    if (lhs_->shape().dimensions(lhs_batch_dim_) % num_devices == 0) {
      const DimMap lhs_dim_map = {{lhs_batch_dim_, mesh_dim}};
      std::string name = absl::StrFormat("Si = Si x R @ 0");
      const DimMap out_dim_map = {{out_batch_dim_, mesh_dim}};
      MaybeAppend(name, lhs_dim_map, {}, out_dim_map, device_mesh_1d_);
    }

    // R = Sk x Sk @ (allreduce @ 0)
    if (lhs_->shape().dimensions(lhs_in_channel_dim_) % num_devices == 0 &&
        rhs_->shape().dimensions(rhs_in_channel_dim_) % num_devices == 0) {
      const DimMap lhs_dim_map = {{lhs_in_channel_dim_, mesh_dim}};
      const DimMap rhs_dim_map = {{rhs_in_channel_dim_, mesh_dim}};
      std::string name = absl::StrFormat("R = Sk x Sk @ %d (allreduce @ %d)",
                                         mesh_dim, mesh_dim);
      const DimMap out_dim_map = {};
      auto communication_cost_fn = [this](const HloSharding& output_spec) {
        double memory_cost = GetBytes(ins_->shape()) / output_spec.NumTiles();
        return cluster_env_.AllReduceCost(memory_cost, 0) +
               cluster_env_.AllReduceCost(memory_cost, 1);
      };
      MaybeAppend(name, lhs_dim_map, rhs_dim_map, out_dim_map, device_mesh_1d_,
                  0, communication_cost_fn);
    }
  }
}

void ConvHandler::SplitDepthwise(bool forward) {
  auto func = [this, forward](const Enumeration& e) {
    const DimMap lhs_dim_map = {
        {lhs_batch_dim_, e.mesh_dims[forward ? 0 : 1]},
        {lhs_in_channel_dim_, e.mesh_dims[forward ? 1 : 0]}};
    const DimMap rhs_dim_map = {{rhs_out_channel_dim_, e.mesh_dims[1]}};
    std::string name =
        absl::StrFormat("SS = SS x RS @ {%s}", absl::StrJoin(e.mesh_dims, ","));
    const DimMap out_dim_map = {{out_batch_dim_, e.mesh_dims[0]},
                                {out_out_channel_dim_, e.mesh_dims[1]}};
    MaybeAppend(name, lhs_dim_map, rhs_dim_map, out_dim_map, device_mesh_);
  };
  EnumerateHalf(func);
}

}  // namespace

// Register strategies for dot instructions.
Status HandleDot(std::unique_ptr<StrategyGroup>& strategy_group,
                 StrategyGroups& strategy_groups, StrategyMap& strategy_map,
                 const HloInstruction* ins, size_t instruction_id,
                 const ClusterEnvironment& cluster_env,
                 const InstructionBatchDimMap& batch_map,
                 const AutoShardingOption& option,
                 const CallGraph& call_graph) {
  strategy_group = CreateLeafStrategyGroup(instruction_id, ins, strategy_map,
                                           strategy_groups);

  DotHandler handler(strategy_group, strategy_map, Cast<HloDotInstruction>(ins),
                     cluster_env, batch_map, option, call_graph);
  TF_RETURN_IF_ERROR(handler.RegisterStrategies());
  return OkStatus();
}

// Register strategies for convolution instructions.
Status HandleConv(std::unique_ptr<StrategyGroup>& strategy_group,
                  StrategyGroups& strategy_groups, StrategyMap& strategy_map,
                  const HloInstruction* ins, size_t instruction_id,
                  const ClusterEnvironment& cluster_env,
                  const InstructionBatchDimMap& batch_map,
                  const AutoShardingOption& option,
                  const CallGraph& call_graph) {
  strategy_group = CreateLeafStrategyGroup(instruction_id, ins, strategy_map,
                                           strategy_groups);

  auto conv_as_dot_dims =
      dot_as_convolution_util::ParseConvolutionDimsInfo(ins);
  if (conv_as_dot_dims.conv_spatial_dims.empty()) {
    DotHandler handler(strategy_group, strategy_map,
                       Cast<HloConvolutionInstruction>(ins), conv_as_dot_dims,
                       cluster_env, batch_map, option, call_graph);
    TF_RETURN_IF_ERROR(handler.RegisterStrategies());

  } else {
    ConvHandler handler(strategy_group, strategy_map, ins, cluster_env,
                        batch_map, option, call_graph);
    TF_RETURN_IF_ERROR(handler.RegisterStrategies());
  }

  return OkStatus();
}

}  // namespace spmd
}  // namespace xla
