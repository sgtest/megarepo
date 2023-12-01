/* Copyright 2019 The TensorFlow Authors. All Rights Reserved.


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

#include "xla/service/hlo_value_semantics_analysis.h"

#include <algorithm>
#include <cstdint>
#include <iterator>
#include <memory>
#include <numeric>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include "absl/algorithm/container.h"
#include "absl/container/flat_hash_set.h"
#include "absl/log/check.h"
#include "absl/log/log.h"
#include "absl/memory/memory.h"
#include "absl/strings/str_cat.h"
#include "absl/strings/str_join.h"
#include "absl/types/span.h"
#include "xla/hlo/ir/dfs_hlo_visitor.h"
#include "xla/hlo/ir/hlo_computation.h"
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/hlo/ir/hlo_module.h"
#include "xla/hlo/ir/hlo_opcode.h"
#include "xla/service/hlo_value.h"
#include "xla/shape.h"
#include "xla/shape_tree.h"
#include "xla/shape_util.h"
#include "xla/status.h"
#include "xla/statusor.h"
#include "xla/util.h"
#include "tsl/platform/errors.h"
#include "tsl/platform/statusor.h"

namespace xla {

extern const char kXlaHostTransferRendezvousNameAttr[];

SendRecvGroupMap CreateSendRecvGroupMap(const HloModule& hlo_module) {
  SendRecvGroupMap send_recv_group_map;
  for (HloComputation* computation : hlo_module.computations()) {
    for (HloInstruction* instruction : computation->instructions()) {
      if (instruction->opcode() != HloOpcode::kSend &&
          instruction->opcode() != HloOpcode::kRecv) {
        continue;
      }
      std::string rendezvous = instruction->frontend_attributes().map().at(
          kXlaHostTransferRendezvousNameAttr);
      auto send_recv_iter = send_recv_group_map.find(rendezvous);
      if (send_recv_iter == send_recv_group_map.end()) {
        auto insert_success = send_recv_group_map.insert(
            {rendezvous, SendRecvGroup{nullptr, nullptr}});
        send_recv_iter = insert_success.first;
      }
      if (instruction->opcode() == HloOpcode::kSend) {
        send_recv_iter->second.send = instruction;
      } else {
        send_recv_iter->second.recv = instruction;
      }
    }
  }
  return send_recv_group_map;
}

bool HloPreOrderDFS::IsReady(const HloInstruction* instruction) const {
  for (HloInstruction* user : instruction->users()) {
    if (!visited_.contains(user)) {
      return false;
    }
  }
  return true;
}

namespace {

std::vector<HloInstruction*> GetAllInstructionsWithZeroUsers(
    const HloComputation& computation) {
  std::vector<HloInstruction*> results;
  for (HloInstruction* instruction : computation.instructions()) {
    if (instruction->users().empty()) {
      results.push_back(instruction);
    }
  }
  return results;
}

StatusOr<HloInstruction*> GetMatchingSendOrRecvFromMap(
    HloInstruction* send_or_recv, const SendRecvGroupMap& send_recv_group_map) {
  if (send_or_recv->opcode() != HloOpcode::kSend &&
      send_or_recv->opcode() != HloOpcode::kRecv) {
    return InvalidArgument("Expecting only send or recv");
  }
  std::string rendezvous = send_or_recv->frontend_attributes().map().at(
      kXlaHostTransferRendezvousNameAttr);
  auto send_recv_iter = send_recv_group_map.find(rendezvous);
  if (send_recv_iter == send_recv_group_map.end()) {
    return InternalError("Missing send or recv from send recv group.");
  }
  if (send_or_recv->opcode() == HloOpcode::kSend) {
    return send_recv_iter->second.recv;
  }
  return send_recv_iter->second.send;
}

}  // namespace

Status HloPreOrderDFS::Run(const HloComputation& computation,
                           DfsHloVisitorBase<HloInstruction*>* visitor) {
  stack_.clear();
  visited_.clear();
  std::vector<HloInstruction*> roots =
      GetAllInstructionsWithZeroUsers(computation);
  for (HloInstruction* root : roots) {
    stack_.push_back(root);
  }
  while (!stack_.empty()) {
    HloInstruction* to_visit = stack_.back();
    stack_.pop_back();
    if (visited_.contains(to_visit)) {
      continue;
    }
    visited_.insert(to_visit);
    for (HloInstruction* operand : to_visit->mutable_operands()) {
      if (IsReady(operand)) {
        stack_.push_back(operand);
      }
    }
    TF_RETURN_IF_ERROR(visitor->Preprocess(to_visit));
    TF_RETURN_IF_ERROR(to_visit->Visit(visitor));
    TF_RETURN_IF_ERROR(visitor->Postprocess(to_visit));
  }
  return OkStatus();
}

Status EinsumDepthAnalysis::RunInternal(
    const HloComputation& computation,
    const std::optional<ShapeTree<int>>& root_depth) {
  std::vector<HloInstruction*> roots =
      GetAllInstructionsWithZeroUsers(computation);
  for (HloInstruction* root : roots) {
    if (root == computation.root_instruction()) {
      if (root_depth.has_value()) {
        TF_RETURN_IF_ERROR(SetInstructionDepth(root, *root_depth));
      } else {
        TF_RETURN_IF_ERROR(SetInstructionDepth(root, 0));
      }
    } else {
      GetOrCreateDepthTree(root);
    }
  }
  HloPreOrderDFS dfs;
  return dfs.Run(computation, this);
}

StatusOr<std::unique_ptr<EinsumDepthAnalysis>> EinsumDepthAnalysis::Run(
    const HloComputation& computation,
    const SendRecvGroupMap& send_recv_group_map) {
  EinsumDepthAnalysis* analysis_ptr =
      new EinsumDepthAnalysis(send_recv_group_map);
  std::unique_ptr<EinsumDepthAnalysis> analysis(analysis_ptr);
  TF_RETURN_IF_ERROR(analysis->RunInternal(computation, std::nullopt));
  return analysis;
}

namespace {

int MergeDepth(int original_depth, int new_depth) {
  // If the instruction has users that are dependent upon by the root, its depth
  // is set by the max of all its users that are dependence of the root.
  if (new_depth >= 0) {
    return std::max(original_depth, new_depth);
  }
  // If the instruction's user is not dependent upon by the root, it affects
  // the depth of the instruction only if all users of the instruction are not
  // ancestors of the root.
  if (new_depth < 0 && original_depth < 0) {
    return std::min(original_depth, new_depth);
  }
  return original_depth;
}

void SetDepth(ShapeTree<int>& depth_tree, int depth) {
  depth_tree.ForEachMutableElement(
      [depth, &depth_tree](const ShapeIndex& shape_index, int* depth_ptr) {
        if (depth_tree.IsLeaf(shape_index)) {
          *depth_ptr = MergeDepth(*depth_ptr, depth);
        }
      });
}

void SetDepth(ShapeTree<int>& depth_tree, const ShapeTree<int>& source) {
  depth_tree.ForEachMutableElement(
      [&depth_tree, &source](const ShapeIndex& shape_index, int* depth_ptr) {
        if (depth_tree.IsLeaf(shape_index)) {
          *depth_ptr = MergeDepth(*depth_ptr, source.element(shape_index));
        }
      });
}

int GetMaxDepth(const ShapeTree<int>& depth_tree) {
  int max_depth = -1;
  depth_tree.ForEachElement(
      [&max_depth](const ShapeIndex& shape_index, int depth) {
        max_depth = std::max(max_depth, depth);
        return OkStatus();
      });
  if (max_depth >= 0) {
    return max_depth;
  }
  depth_tree.ForEachElement(
      [&max_depth](const ShapeIndex& shape_index, int depth) {
        max_depth = std::min(max_depth, depth);
        return OkStatus();
      });
  return max_depth;
}

void SetDepthFromTupleDepth(ShapeTree<int>& depth_tree,
                            const ShapeTree<int>& tuple_depth_tree,
                            int tuple_index) {
  depth_tree.ForEachMutableElement([&depth_tree, &tuple_depth_tree,
                                    tuple_index](const ShapeIndex& shape_index,
                                                 int* depth_ptr) {
    if (depth_tree.IsLeaf(shape_index)) {
      ShapeIndex output_index = shape_index;
      output_index.push_front(tuple_index);
      *depth_ptr = std::max(*depth_ptr, tuple_depth_tree.element(output_index));
    }
  });
}

}  // namespace

EinsumDepthMap::iterator EinsumDepthAnalysis::GetOrCreateDepthTree(
    HloInstruction* instruction) {
  auto depth_iter = einsum_depth_map_.find(instruction);
  if (depth_iter == einsum_depth_map_.end()) {
    ShapeTree<int> depth_tree(instruction->shape(), -1);
    auto inserted = einsum_depth_map_.insert(
        std::make_pair(instruction, std::move(depth_tree)));
    depth_iter = inserted.first;
  }
  return depth_iter;
}

Status EinsumDepthAnalysis::SetInstructionDepth(HloInstruction* instruction,
                                                int depth) {
  auto depth_iter = GetOrCreateDepthTree(instruction);
  ShapeTree<int>& depth_tree = depth_iter->second;
  SetDepth(depth_tree, depth);
  return OkStatus();
}

Status EinsumDepthAnalysis::SetInstructionDepth(HloInstruction* instruction,
                                                const ShapeTree<int>& depth) {
  auto depth_iter = GetOrCreateDepthTree(instruction);
  ShapeTree<int>& depth_tree = depth_iter->second;
  SetDepth(depth_tree, depth);
  return OkStatus();
}

Status EinsumDepthAnalysis::DefaultAction(HloInstruction* instruction) {
  if (!instruction->shape().IsToken() && !instruction->shape().IsArray() &&
      !instruction->shape().IsTuple()) {
    return InvalidArgument("Unexpected shape for default action.");
  }
  auto depth_iter = einsum_depth_map_.find(instruction);
  CHECK(depth_iter != einsum_depth_map_.end());
  const ShapeTree<int> depth_tree = depth_iter->second;
  int max_depth = GetMaxDepth(depth_tree);
  if (instruction->shape().IsToken()) {
    for (HloInstruction* operand : instruction->mutable_operands()) {
      TF_RETURN_IF_ERROR(SetInstructionDepth(operand, max_depth));
    }
  }
  if (instruction->operand_count() == 1) {
    HloInstruction* operand = instruction->mutable_operand(0);
    if (Shape::Equal().IgnoreLayout()(instruction->shape(), operand->shape())) {
      TF_RETURN_IF_ERROR(SetInstructionDepth(operand, depth_tree));
      return OkStatus();
    }
  }
  // If the instruction is an array, the output depends on all operands.
  if (instruction->shape().IsArray()) {
    int instruction_depth = depth_tree.element({});
    for (HloInstruction* operand : instruction->mutable_operands()) {
      TF_RETURN_IF_ERROR(SetInstructionDepth(operand, instruction_depth));
    }
    return OkStatus();
  }
  // If the instruction is a tuple and the output size is larger than the
  // operand count, each tuple element depends on all operands.
  int tuple_shape_size = instruction->shape().tuple_shapes_size();
  if (instruction->operand_count() < tuple_shape_size) {
    for (HloInstruction* operand : instruction->mutable_operands()) {
      TF_RETURN_IF_ERROR(SetInstructionDepth(operand, max_depth));
    }
    return OkStatus();
  }
  // Each tuple element depends on a specific operand.
  for (int operand_index = 0; operand_index < instruction->operand_count();
       ++operand_index) {
    HloInstruction* operand = instruction->mutable_operand(operand_index);
    if (operand_index < tuple_shape_size) {
      auto operand_depth_iter = GetOrCreateDepthTree(operand);
      ShapeTree<int>& operand_depth = operand_depth_iter->second;
      SetDepthFromTupleDepth(operand_depth, depth_tree, operand_index);
    } else {
      TF_RETURN_IF_ERROR(SetInstructionDepth(operand, max_depth));
    }
  }
  return OkStatus();
}

Status EinsumDepthAnalysis::HandleTuple(HloInstruction* tuple) {
  auto depth_iter = einsum_depth_map_.find(tuple);
  CHECK(depth_iter != einsum_depth_map_.end());
  const ShapeTree<int> depth_tree = depth_iter->second;
  for (int operand_index = 0; operand_index < tuple->operand_count();
       ++operand_index) {
    HloInstruction* operand = tuple->mutable_operand(operand_index);
    auto operand_depth_iter = GetOrCreateDepthTree(operand);
    ShapeTree<int>& operand_depth = operand_depth_iter->second;
    SetDepthFromTupleDepth(operand_depth, depth_tree, operand_index);
  }
  return OkStatus();
}

Status EinsumDepthAnalysis::HandleGetTupleElement(
    HloInstruction* get_tuple_element) {
  auto depth_iter = einsum_depth_map_.find(get_tuple_element);
  CHECK(depth_iter != einsum_depth_map_.end());
  const ShapeTree<int> depth_tree = depth_iter->second;

  HloInstruction* operand = get_tuple_element->mutable_operand(0);
  int tuple_index = get_tuple_element->tuple_index();
  auto operand_depth_iter = GetOrCreateDepthTree(operand);
  ShapeTree<int>& operand_depth = operand_depth_iter->second;
  operand_depth.ForEachMutableElement(
      [&operand_depth, &depth_tree, tuple_index](const ShapeIndex& shape_index,
                                                 int* depth_ptr) {
        if (shape_index.empty() || shape_index.front() != tuple_index) {
          return;
        }
        if (operand_depth.IsLeaf(shape_index)) {
          ShapeIndex output_index = shape_index;
          output_index.pop_front();
          *depth_ptr = MergeDepth(*depth_ptr, depth_tree.element(output_index));
        }
      });
  return OkStatus();
}

Status EinsumDepthAnalysis::HandleDepthIncrementInstruction(
    HloInstruction* instruction) {
  auto depth_iter = einsum_depth_map_.find(instruction);
  CHECK(depth_iter != einsum_depth_map_.end());
  int instruction_depth = depth_iter->second.element({});
  for (HloInstruction* operand : instruction->mutable_operands()) {
    TF_RETURN_IF_ERROR(SetInstructionDepth(
        operand, instruction_depth >= 0 ? instruction_depth + 1
                                        : instruction_depth - 1));
  }
  return OkStatus();
}

Status EinsumDepthAnalysis::HandleDot(HloInstruction* dot) {
  auto depth_iter = einsum_depth_map_.find(dot);
  CHECK(depth_iter != einsum_depth_map_.end());
  return HandleDepthIncrementInstruction(dot);
}

Status EinsumDepthAnalysis::HandleConvolution(HloInstruction* convolution) {
  return HandleDepthIncrementInstruction(convolution);
}

Status EinsumDepthAnalysis::HandleCall(HloInstruction* call) {
  auto depth_iter = einsum_depth_map_.find(call);
  CHECK(depth_iter != einsum_depth_map_.end());
  const ShapeTree<int> depth_tree = depth_iter->second;
  return HandleCalledComputation(*call->called_computations()[0], depth_tree,
                                 call->operands());
}

Status EinsumDepthAnalysis::HandleFusion(HloInstruction* fusion) {
  auto depth_iter = einsum_depth_map_.find(fusion);
  CHECK(depth_iter != einsum_depth_map_.end());
  const ShapeTree<int> depth_tree = depth_iter->second;
  return HandleCalledComputation(*fusion->called_computations()[0], depth_tree,
                                 fusion->operands());
}

Status EinsumDepthAnalysis::HandleCustomCall(HloInstruction* custom_call) {
  if (custom_call->shape().IsToken() || custom_call->shape().IsArray() ||
      custom_call->shape().IsTuple()) {
    return DefaultAction(custom_call);
  }
  return Unimplemented("Unimplemented custom-call: %s",
                       custom_call->custom_call_target());
}

Status EinsumDepthAnalysis::HandleWhile(HloInstruction* xla_while) {
  auto depth_iter = einsum_depth_map_.find(xla_while);
  CHECK(depth_iter != einsum_depth_map_.end());
  const ShapeTree<int>& depth_tree = depth_iter->second;
  int max_depth = GetMaxDepth(depth_tree);
  HloComputation* condition_computation = xla_while->while_condition();
  HloInstruction* condition_root = condition_computation->root_instruction();
  ShapeTree<int> condition_depth(condition_root->shape(), max_depth);
  TF_RETURN_IF_ERROR(HandleCalledComputation(
      *condition_computation, condition_depth, xla_while->operands()));
  HloComputation* body_computation = xla_while->while_body();
  bool run_depth_propagation_on_body = true;
  const ShapeTree<int>* root_depth_ptr = &depth_tree;
  auto root_depth_iter =
      GetOrCreateDepthTree(body_computation->root_instruction());
  ShapeTree<int>& root_depth = root_depth_iter->second;
  while (run_depth_propagation_on_body) {
    run_depth_propagation_on_body = false;
    TF_RETURN_IF_ERROR(HandleCalledComputation(
        *body_computation, *root_depth_ptr, xla_while->operands()));
    // Elements of while loop outputs may only be used within the while loop.
    // If such elements exist, we set its root depth to it operand depth. Then
    // recompute while loop instruction depths.
    HloInstruction* operand = body_computation->parameter_instruction(0);
    const ShapeTree<int>& operand_depth = GetOrCreateDepthTree(operand)->second;

    root_depth.ForEachMutableElement(
        [&run_depth_propagation_on_body, &root_depth, &operand_depth](
            const ShapeIndex& shape_index, int* depth_ptr) {
          if (!root_depth.IsLeaf(shape_index)) {
            return;
          }
          if (root_depth.element(shape_index) < 0 &&
              operand_depth.element(shape_index) >= 0) {
            *depth_ptr = operand_depth.element(shape_index);
            run_depth_propagation_on_body = true;
          }
        });
    root_depth_ptr = &root_depth;
  }
  return OkStatus();
}

Status EinsumDepthAnalysis::HandleConditional(HloInstruction* conditional) {
  auto depth_iter = einsum_depth_map_.find(conditional);
  CHECK(depth_iter != einsum_depth_map_.end());
  const ShapeTree<int> depth_tree = depth_iter->second;
  // Conditionals have one more operand than the number of branches. The first
  // operand is the pred.
  TF_RETURN_IF_ERROR(
      SetInstructionDepth(conditional->operands()[0], depth_tree));
  for (int i = 0; i < conditional->branch_count(); ++i) {
    TF_RETURN_IF_ERROR(
        HandleCalledComputation(*conditional->called_computations()[i],
                                depth_tree, {conditional->operands()[i + 1]}));
  }
  return OkStatus();
}

Status EinsumDepthAnalysis::HandleCalledComputation(
    const HloComputation& called_computation, const ShapeTree<int>& root_depth,
    absl::Span<HloInstruction* const> operands) {
  TF_RETURN_IF_ERROR(RunInternal(called_computation,
                                 std::optional<ShapeTree<int>>(root_depth)));
  for (int i = 0; i < operands.size(); ++i) {
    HloInstruction* operand = operands[i];
    HloInstruction* parameter = called_computation.parameter_instruction(i);
    const ShapeTree<int> parameter_depth =
        GetOrCreateDepthTree(parameter)->second;
    TF_RETURN_IF_ERROR(SetInstructionDepth(operand, parameter_depth));
  }
  return OkStatus();
}

Status EinsumDepthAnalysis::HandleAfterAll(HloInstruction* after_all) {
  return OkStatus();
}

Status EinsumDepthAnalysis::HandleOutfeed(HloInstruction* outfeed) {
  auto depth_iter = einsum_depth_map_.find(outfeed);
  CHECK(depth_iter != einsum_depth_map_.end());
  const ShapeTree<int> depth_tree = depth_iter->second;
  int max_depth = GetMaxDepth(depth_tree);
  for (HloInstruction* operand : outfeed->mutable_operands()) {
    TF_RETURN_IF_ERROR(SetInstructionDepth(operand, max_depth));
  }
  return OkStatus();
}

Status EinsumDepthAnalysis::HandleCollectivePermuteStart(
    HloInstruction* collective_permute_start) {
  auto depth_iter = einsum_depth_map_.find(collective_permute_start);
  CHECK(depth_iter != einsum_depth_map_.end());
  const ShapeTree<int>& depth_tree = depth_iter->second;
  for (int operand_index = 0;
       operand_index < collective_permute_start->operand_count();
       ++operand_index) {
    HloInstruction* operand =
        collective_permute_start->mutable_operand(operand_index);
    if (operand_index >= 2) {
      TF_RETURN_IF_ERROR(SetInstructionDepth(operand, GetMaxDepth(depth_tree)));
      continue;
    }
    auto operand_depth_iter = GetOrCreateDepthTree(operand);
    ShapeTree<int>& operand_depth = operand_depth_iter->second;
    SetDepthFromTupleDepth(operand_depth, depth_tree, 1);
  }
  return OkStatus();
}

Status EinsumDepthAnalysis::HandleCollectivePermuteDone(
    HloInstruction* collective_permute_done) {
  auto depth_iter = einsum_depth_map_.find(collective_permute_done);
  CHECK(depth_iter != einsum_depth_map_.end());
  const ShapeTree<int>& depth_tree = depth_iter->second;
  auto operand_depth_iter =
      GetOrCreateDepthTree(collective_permute_done->mutable_operand(0));
  ShapeTree<int>& operand_depth = operand_depth_iter->second;
  int max_depth = GetMaxDepth(depth_tree);
  operand_depth.ForEachMutableElement([&operand_depth, &depth_tree, max_depth](
                                          const ShapeIndex& index, int* depth) {
    if (!operand_depth.IsLeaf(index)) {
      return;
    }
    if (index.front() == 0 || index.front() == 1) {
      ShapeIndex output_index = index;
      output_index.pop_front();
      *depth = depth_tree.element(output_index);
    }
    *depth = max_depth;
  });
  return OkStatus();
}

Status EinsumDepthAnalysis::HandleSend(HloInstruction* send) {
  auto depth_iter = GetOrCreateDepthTree(send);
  const ShapeTree<int>& depth_tree = depth_iter->second;
  HloInstruction* send_buffer = send->mutable_operand(0);
  auto send_buffer_depth_iter = GetOrCreateDepthTree(send_buffer);
  ShapeTree<int>& send_buffer_depth = send_buffer_depth_iter->second;
  SetDepthFromTupleDepth(send_buffer_depth, depth_tree, 0);
  int max_depth = GetMaxDepth(depth_tree);
  HloInstruction* token = send->mutable_operand(1);
  return SetInstructionDepth(token, max_depth);
}

Status EinsumDepthAnalysis::HandleRecv(HloInstruction* recv) {
  auto depth_iter = GetOrCreateDepthTree(recv);
  const ShapeTree<int>& depth_tree = depth_iter->second;
  TF_ASSIGN_OR_RETURN(HloInstruction * send,
                      GetMatchingSendOrRecvFromMap(recv, send_recv_group_map_));
  auto send_depth_iter = GetOrCreateDepthTree(send);
  ShapeTree<int>& send_depth = send_depth_iter->second;
  int max_depth = GetMaxDepth(depth_tree);
  send_depth.ForEachMutableElement([&depth_tree, &send_depth, max_depth](
                                       const ShapeIndex& index, int* depth) {
    if (!send_depth.IsLeaf(index)) {
      return;
    }
    if (index.front() == 0) {
      *depth = MergeDepth(*depth, depth_tree.element(index));
      return;
    }
    *depth = MergeDepth(*depth, max_depth);
  });
  return OkStatus();
}

Status EinsumDepthAnalysis::HandleSendDone(HloInstruction* send_done) {
  HloInstruction* send = send_done->mutable_operand(0);
  auto depth_iter = GetOrCreateDepthTree(send_done);
  const ShapeTree<int>& depth_tree = depth_iter->second;
  int max_depth = GetMaxDepth(depth_tree);
  return SetInstructionDepth(send, max_depth);
}

Status EinsumDepthAnalysis::HandleRecvDone(HloInstruction* recv_done) {
  auto depth_iter = GetOrCreateDepthTree(recv_done);
  const ShapeTree<int>& depth_tree = depth_iter->second;
  int max_depth = GetMaxDepth(depth_tree);
  HloInstruction* recv = recv_done->mutable_operand(0);
  auto recv_depth_iter = GetOrCreateDepthTree(recv);
  ShapeTree<int>& recv_depth = recv_depth_iter->second;
  recv_depth.ForEachMutableElement([&depth_tree, &recv_depth, max_depth](
                                       const ShapeIndex& index, int* depth) {
    if (!recv_depth.IsLeaf(index)) {
      return;
    }
    if (index.front() == 0) {
      *depth = MergeDepth(*depth, depth_tree.element(index));
      return;
    }
    *depth = MergeDepth(*depth, max_depth);
  });
  return OkStatus();
}

std::string HloValueSemanticLabelToString(HloValueSemanticLabel label) {
  switch (label) {
    case HloValueSemanticLabel::kStatic:
      return "Static";
    case HloValueSemanticLabel::kRandom:
      return "Random";
    case HloValueSemanticLabel::kWeight:
      return "Weight";
    case HloValueSemanticLabel::kActivation:
      return "Activation";
    case HloValueSemanticLabel::kActivationGradient:
      return "ActivationGradient";
    case HloValueSemanticLabel::kWeightGradient:
      return "WeightGradient";
    case HloValueSemanticLabel::kTupleOrToken:
      return "TupleOrToken";
  }
}

std::string HloValueSemantics::ToString() const {
  std::string content = absl::StrJoin(
      {absl::StrCat("label: ", HloValueSemanticLabelToString(label_)),
       absl::StrCat("origin: ", origin_.ToString())},
      ", ");
  return absl::StrCat("{", content, "}");
}

HloValueSemantics::HloValueSemantics(HloValueSemanticLabel label,
                                     const HloPosition& origin)
    : HloValueSemantics(0, label, origin) {}

HloValueSemantics::HloValueSemantics(Id id, HloValueSemanticLabel label,
                                     const HloPosition& origin)
    : id_(id), label_(label), origin_(origin) {}

HloValueSemanticsAnalysis::HloValueSemanticsAnalysis(const HloModule& module)
    : module_(module), next_id_(0) {}

const HloValueSemantics* HloValueSemanticsAnalysis::GetSemantics(
    const HloInstruction* instruction, const ShapeIndex& index) const {
  return GetInstructionSemantics(instruction).element(index);
}

StatusOr<std::unique_ptr<HloValueSemanticsAnalysis>>
HloValueSemanticsAnalysis::Run(const HloModule& module) {
  std::unique_ptr<HloValueSemanticsAnalysis> value_semantics_analysis =
      absl::WrapUnique(new HloValueSemanticsAnalysis(module));
  value_semantics_analysis->InitializeSendRecvGroups();
  TF_RETURN_IF_ERROR(value_semantics_analysis->InitializeEinsumDepth());
  value_semantics_analysis->AnnotateWeights();
  TF_RETURN_IF_ERROR(
      value_semantics_analysis->RunOnComputation(*module.entry_computation()));
  return value_semantics_analysis;
}

Status HloValueSemanticsAnalysis::InitializeEinsumDepth() {
  TF_ASSIGN_OR_RETURN(
      std::unique_ptr<EinsumDepthAnalysis> einsum_depth_analysis,
      EinsumDepthAnalysis::Run(*module_.entry_computation(),
                               send_recv_group_map_));
  einsum_depth_map_ = einsum_depth_analysis->GetEinsumDepthMap();
  return OkStatus();
}

void HloValueSemanticsAnalysis::InitializeSendRecvGroups() {
  send_recv_group_map_ = CreateSendRecvGroupMap(module_);
}

bool HloValueSemanticsAnalysis::HasSemanticsFor(
    const HloInstruction* instruction) const {
  return value_semantics_.contains(instruction);
}

StatusOr<HloInstruction*> HloValueSemanticsAnalysis::GetMatchingSendOrRecv(
    HloInstruction* send_or_recv) const {
  return GetMatchingSendOrRecvFromMap(send_or_recv, send_recv_group_map_);
}

HloValueSemantics::Id HloValueSemanticsAnalysis::NextId() { return next_id_++; }

const HloValueSemantics* HloValueSemanticsAnalysis::NewHloValueSemantics(
    HloValueSemanticLabel label, const HloPosition& origin) {
  HloValueSemantics::Id id = NextId();
  auto inserted = value_semantics_map_.insert(std::make_pair(
      id, std::make_unique<HloValueSemantics>(id, label, origin)));
  return inserted.first->second.get();
}

const ShapeTree<const HloValueSemantics*>&
HloValueSemanticsAnalysis::GetInstructionSemantics(
    const HloInstruction* instruction) const {
  auto semantics_iter = value_semantics_.find(instruction);
  CHECK(semantics_iter != value_semantics_.end())
      << "instruction: " << instruction->ToString();
  return semantics_iter->second;
}

void HloValueSemanticsAnalysis::DeepCopyHloValueSemantics(
    ShapeTree<const HloValueSemantics*>& copy_to,
    const ShapeTree<const HloValueSemantics*>& copy_from,
    const ShapeIndex& source_index, const ShapeIndex& destination_index) {
  copy_to.ForEachMutableElement(
      [this, &copy_from, &source_index, &destination_index](
          const ShapeIndex& index, const HloValueSemantics** semantics) {
        if (index.size() < destination_index.size()) {
          return;
        }
        bool in_subtree_to_copy = true;
        for (int i = 0; i < destination_index.size(); ++i) {
          if (index[i] != destination_index[i]) {
            in_subtree_to_copy = false;
            break;
          }
        }
        if (!in_subtree_to_copy) {
          return;
        }
        ShapeIndex full_source_index = source_index;
        for (int i = destination_index.size(); i < index.size(); ++i) {
          full_source_index.push_back(index[i]);
        }
        const HloValueSemantics* source_semantics =
            copy_from.element(full_source_index);
        *semantics = NewHloValueSemantics(source_semantics->label(),
                                          source_semantics->origin());
      });
}

void HloValueSemanticsAnalysis::DeepCopyHloValueSemantics(
    const HloInstruction* target,
    const ShapeTree<const HloValueSemantics*>& copy_from,
    const ShapeIndex& source_index) {
  auto semantics_iter = value_semantics_.find(target);
  if (semantics_iter != value_semantics_.end()) {
    DeleteHloValueSemantics(semantics_iter->second);
    DeepCopyHloValueSemantics(semantics_iter->second, copy_from, source_index,
                              {});
    return;
  }
  ShapeTree<const HloValueSemantics*> semantics_shape_tree(target->shape(),
                                                           nullptr);
  DeepCopyHloValueSemantics(semantics_shape_tree, copy_from, source_index, {});
  value_semantics_[target] = std::move(semantics_shape_tree);
}

void HloValueSemanticsAnalysis::SetHloValueSemantics(
    const HloInstruction* target,
    const ShapeTree<const HloValueSemantics*>& semantics) {
  auto semantics_iter = value_semantics_.find(target);
  if (semantics_iter != value_semantics_.end()) {
    DeleteHloValueSemantics(semantics_iter->second);
  }
  value_semantics_[target] = semantics;
}

void HloValueSemanticsAnalysis::DeleteHloValueSemantics(
    const HloValueSemantics* to_delete) {
  value_semantics_map_.erase(to_delete->id());
}

void HloValueSemanticsAnalysis::DeleteHloValueSemantics(
    const ShapeTree<const HloValueSemantics*>& to_delete) {
  to_delete.ForEachElement(
      [this](const ShapeIndex& index, const HloValueSemantics* semantics) {
        DeleteHloValueSemantics(semantics);
      });
}

void HloValueSemanticsAnalysis::AnnotateWeights() {
  const HloComputation* entry_computation = module_.entry_computation();
  for (HloInstruction* parameter :
       entry_computation->parameter_instructions()) {
    ShapeTree<const HloValueSemantics*> semantics_shape_tree(parameter->shape(),
                                                             nullptr);
    semantics_shape_tree.ForEachMutableElement(
        [this, &semantics_shape_tree, parameter](
            const ShapeIndex& index, const HloValueSemantics** semantics) {
          if (!semantics_shape_tree.IsLeaf(index)) {
            *semantics = NewHloValueSemantics(
                HloValueSemanticLabel::kTupleOrToken, {parameter, index});
          }
          *semantics = NewHloValueSemantics(HloValueSemanticLabel::kWeight,
                                            {parameter, index});
        });
    value_semantics_[parameter] = std::move(semantics_shape_tree);
  }
}

Status HloValueSemanticsAnalysis::RunOnComputation(
    const HloComputation& computation,
    absl::Span<const HloInstruction* const> operands) {
  CHECK_EQ(computation.num_parameters(), operands.size());
  for (int i = 0; i < computation.num_parameters(); ++i) {
    auto semantics_iter = value_semantics_.find(operands[i]);
    CHECK(semantics_iter != value_semantics_.end());
    DeepCopyHloValueSemantics(computation.parameter_instructions()[i],
                              semantics_iter->second);
  }
  return RunOnComputation(computation);
}

Status HloValueSemanticsAnalysis::RunOnComputation(
    const HloComputation& computation) {
  HloValueSemanticsPropagation propagation(this);
  return propagation.Run(computation);
}

HloValueSemanticsPropagation::HloValueSemanticsPropagation(
    HloValueSemanticsAnalysis* analysis)
    : analysis_(analysis) {}

Status HloValueSemanticsPropagation::Run(const HloComputation& computation) {
  return computation.root_instruction()->Accept(this);
}

HloValueSemantics HloValueSemanticsPropagation::CopySemantics(
    const HloValueSemantics& semantics) const {
  return HloValueSemantics(semantics.label(), semantics.origin());
}

HloValueSemantics HloValueSemanticsPropagation::CopySemanticsWithNewOrigin(
    const HloValueSemantics& semantics, HloInstruction* new_origin,
    const ShapeIndex& index) const {
  return HloValueSemantics(semantics.label(), {new_origin, index});
}

const HloValueSemantics* HloValueSemanticsPropagation::AddSemantics(
    const HloValueSemantics& semantics) {
  return analysis_->NewHloValueSemantics(semantics.label(), semantics.origin());
}

std::vector<HloValueSemanticsPropagation::EinsumAndOperandIndex>
HloValueSemanticsPropagation::FindEinsumsWhereOriginDependsOnOther(
    const HloValueSemantics& semantics, const HloPosition& origin_dependence,
    bool recursive) const {
  std::vector<HloPosition> stack;
  absl::flat_hash_set<HloPosition> visited;
  std::vector<HloValueSemanticsPropagation::EinsumAndOperandIndex>
      dependent_einsums;
  stack.push_back(semantics.origin());
  while (!stack.empty()) {
    HloPosition origin = stack.back();
    stack.pop_back();
    if (visited.contains(origin)) {
      continue;
    }
    visited.insert(origin);
    absl::Span<const HloInstruction* const> operands =
        origin.instruction->operands();
    // Do not check slice indices.
    if (origin.instruction->opcode() == HloOpcode::kDynamicUpdateSlice) {
      operands = operands.subspan(0, 2);
    }
    if (origin.instruction->opcode() == HloOpcode::kDynamicSlice) {
      operands = operands.subspan(0, 1);
    }
    bool is_einsum = origin.instruction->opcode() == HloOpcode::kDot ||
                     origin.instruction->opcode() == HloOpcode::kConvolution;
    bool found_einsum = false;
    if (is_einsum) {
      for (int64_t operand_index = 0; operand_index < operands.size();
           ++operand_index) {
        const HloInstruction* origin_operand = operands[operand_index];
        const HloValueSemantics* origin_operand_semantics =
            analysis_->GetSemantics(origin_operand);
        if (origin_operand_semantics->origin() == origin_dependence) {
          dependent_einsums.push_back({origin.instruction, operand_index});
          found_einsum = true;
        }
      }
    }
    if (!found_einsum && recursive) {
      for (int64_t operand_index = 0; operand_index < operands.size();
           ++operand_index) {
        const HloInstruction* origin_operand = operands[operand_index];
        const HloValueSemantics* origin_operand_semantics =
            analysis_->GetSemantics(origin_operand);
        stack.push_back(origin_operand_semantics->origin());
      }
    }
  }
  return dependent_einsums;
}

bool HloValueSemanticsPropagation::OriginDependsOn(
    const HloValueSemantics& semantics, const HloPosition& origin_dependence,
    bool recursive) const {
  auto dependent_einsums = FindEinsumsWhereOriginDependsOnOther(
      semantics, origin_dependence, recursive);
  return !dependent_einsums.empty();
}

StatusOr<HloValueSemantics>
HloValueSemanticsPropagation::ComputeSemanticsFromStaticAndOther(
    const HloValueSemantics& static_semantics,
    const HloValueSemantics& other_semantics,
    HloInstruction* instruction) const {
  CHECK(static_semantics.label() == HloValueSemanticLabel::kStatic)
      << __func__ << ", : " << static_semantics.ToString();
  if (other_semantics.label() == HloValueSemanticLabel::kStatic) {
    return CopySemanticsWithNewOrigin(other_semantics, instruction);
  }

  bool is_dot_or_convolution = instruction->opcode() == HloOpcode::kDot ||
                               instruction->opcode() == HloOpcode::kConvolution;
  if (is_dot_or_convolution &&
      other_semantics.label() == HloValueSemanticLabel::kActivationGradient) {
    return MaybeCreateGradientSemantics(
        instruction, HloValueSemanticLabel::kActivationGradient);
  }
  return CopySemantics(other_semantics);
}

StatusOr<HloValueSemantics>
HloValueSemanticsPropagation::ComputeSemanticsFromRandomAndOther(
    const HloValueSemantics& random_semantics,
    const HloValueSemantics& other_semantics,
    HloInstruction* instruction) const {
  CHECK(random_semantics.label() == HloValueSemanticLabel::kRandom);
  CHECK(other_semantics.label() != HloValueSemanticLabel::kStatic);
  if (other_semantics.label() == HloValueSemanticLabel::kRandom) {
    return CopySemanticsWithNewOrigin(other_semantics, instruction);
  }
  return CopySemantics(other_semantics);
}

StatusOr<HloValueSemantics>
HloValueSemanticsPropagation::MaybeCreateGradientSemantics(
    HloInstruction* gradient_candidate,
    HloValueSemanticLabel fallback_label) const {
  const EinsumDepthMap& einsum_depth_map = analysis_->GetEinsumDepthMap();
  auto depth_iter = einsum_depth_map.find(gradient_candidate);
  CHECK(depth_iter != einsum_depth_map.end());
  int gradient_depth = depth_iter->second.element({});
  if (gradient_depth < 0) {
    // There is dependency between the two operands of the dot, but the dot
    // is not used by root. This is likely eval computation in a TF program.
    return HloValueSemantics(HloValueSemanticLabel::kActivation,
                             {gradient_candidate, {}});
  }
  // If the gradient has no einsum users, then it's a WeightGradient.
  if (gradient_depth == 0) {
    return HloValueSemantics(HloValueSemanticLabel::kWeightGradient,
                             {gradient_candidate, {}});
  }
  return HloValueSemantics(fallback_label, {gradient_candidate, {}});
}

StatusOr<HloValueSemantics>
HloValueSemanticsPropagation::ComputeSemanticsFromWeightAndOther(
    const HloValueSemantics& weight_semantics,
    const HloValueSemantics& other_semantics,
    HloInstruction* instruction) const {
  CHECK(weight_semantics.label() == HloValueSemanticLabel::kWeight);
  CHECK(other_semantics.label() != HloValueSemanticLabel::kStatic &&
        other_semantics.label() != HloValueSemanticLabel::kRandom);
  bool is_dot_or_convolution = instruction->opcode() == HloOpcode::kDot ||
                               instruction->opcode() == HloOpcode::kConvolution;
  if (other_semantics.label() == HloValueSemanticLabel::kWeight) {
    if (!is_dot_or_convolution) {
      if (weight_semantics.origin() == other_semantics.origin()) {
        return CopySemantics(other_semantics);
      }
      return CopySemanticsWithNewOrigin(other_semantics, instruction);
    }
    return HloValueSemantics(HloValueSemanticLabel::kActivation,
                             {instruction, {}});
  }
  if (!is_dot_or_convolution) {
    return CopySemantics(other_semantics);
  }
  if (other_semantics.label() == HloValueSemanticLabel::kActivation) {
    // In our analysis, loss is classified as Activation. So an einsum between
    // a Weight (W) and an Activation (X) could be an ActivationGradient when X
    // is the loss. We distinguish this case from regular Activations by
    // checking whether X is computed from some einsum that takes W as an
    //  operand.
    if (OriginDependsOn(other_semantics, weight_semantics.origin(),
                        /*recursive=*/true)) {
      return MaybeCreateGradientSemantics(
          instruction, HloValueSemanticLabel::kActivationGradient);
    }
    return CopySemanticsWithNewOrigin(other_semantics, instruction);
  }
  if (other_semantics.label() == HloValueSemanticLabel::kActivationGradient) {
    // Since we classify input data as Weight, there are Weight-Weight einsums
    // which produce an Activation. The ActivationGradient to this Activation
    // could be used in an einsum with one of the Weights to compute
    // the WeightGradient for the other Weight.
    return MaybeCreateGradientSemantics(
        instruction, HloValueSemanticLabel::kActivationGradient);
  }
  CHECK(other_semantics.label() == HloValueSemanticLabel::kWeightGradient);
  return CopySemantics(other_semantics);
}

StatusOr<HloValueSemantics>
HloValueSemanticsPropagation::ComputeSemanticsFromActivationAndOther(
    const HloValueSemantics& activation_semantics,
    const HloValueSemantics& other_semantics,
    HloInstruction* instruction) const {
  CHECK(activation_semantics.label() == HloValueSemanticLabel::kActivation);
  CHECK(other_semantics.label() != HloValueSemanticLabel::kStatic &&
        other_semantics.label() != HloValueSemanticLabel::kRandom &&
        other_semantics.label() != HloValueSemanticLabel::kWeight);
  bool is_dot_or_convolution = instruction->opcode() == HloOpcode::kDot ||
                               instruction->opcode() == HloOpcode::kConvolution;
  if (!is_dot_or_convolution) {
    if (activation_semantics.origin() == other_semantics.origin()) {
      return CopySemantics(other_semantics);
    }
    return CopySemanticsWithNewOrigin(other_semantics, instruction);
  }
  if (other_semantics.label() == HloValueSemanticLabel::kActivation) {
    // Like said above, since loss is classified as Activation, an einsum
    // between an Activation X and an Activation Y could be WeightGradient if
    // either X or Y is the loss. This case is different from other Activation
    // einsums because there must a dependency between X and Y.
    bool other_depends_on_activation = OriginDependsOn(
        other_semantics, activation_semantics.origin(), /*recursive=*/true);
    bool activation_depends_on_other =
        OriginDependsOn(activation_semantics, other_semantics.origin(),
                        /*recursive=*/true);
    CHECK(!other_depends_on_activation || !activation_depends_on_other);
    // If there is no dependency between the two Activations, the output must
    // be an Activation.
    if (other_depends_on_activation || activation_depends_on_other) {
      // We check if the einsum is actually weight gradient. If it is not, fall
      // back to activation, since we expect the loss to be computed from an
      // activation-weight einsum.
      return MaybeCreateGradientSemantics(instruction,
                                          HloValueSemanticLabel::kActivation);
    }
    return CopySemanticsWithNewOrigin(other_semantics, instruction);
  }
  if (other_semantics.label() == HloValueSemanticLabel::kActivationGradient) {
    // An Activation-ActivationGradient einsum could be computing
    // WeightGradient or ActivationGradient.
    return MaybeCreateGradientSemantics(
        instruction, HloValueSemanticLabel::kActivationGradient);
  }
  CHECK(other_semantics.label() == HloValueSemanticLabel::kWeightGradient)
      << "instruction:  " << instruction->ToString()
      << ", semantics: " << other_semantics.ToString()
      << ", expected: WeightGradient.";

  return CopySemantics(other_semantics);
}

StatusOr<HloValueSemantics>
HloValueSemanticsPropagation::ComputeSemanticsFromActivationGradientAndOther(
    const HloValueSemantics& activation_gradient_semantics,
    const HloValueSemantics& other_semantics,
    HloInstruction* instruction) const {
  CHECK(activation_gradient_semantics.label() ==
        HloValueSemanticLabel::kActivationGradient);
  CHECK(other_semantics.label() != HloValueSemanticLabel::kStatic &&
        other_semantics.label() != HloValueSemanticLabel::kRandom &&
        other_semantics.label() != HloValueSemanticLabel::kWeight &&
        other_semantics.label() != HloValueSemanticLabel::kActivation);
  if (other_semantics.label() == HloValueSemanticLabel::kActivationGradient) {
    return CopySemanticsWithNewOrigin(other_semantics, instruction);
  }

  CHECK(other_semantics.label() == HloValueSemanticLabel::kWeightGradient);
  return CopySemantics(other_semantics);
}

StatusOr<HloValueSemantics>
HloValueSemanticsPropagation::ComputeSemanticsFromWeightGradientAndOther(
    const HloValueSemantics& weight_gradient_semantics,
    const HloValueSemantics& other_semantics,
    HloInstruction* instruction) const {
  CHECK(weight_gradient_semantics.label() ==
        HloValueSemanticLabel::kWeightGradient);
  CHECK(other_semantics.label() != HloValueSemanticLabel::kStatic &&
        other_semantics.label() != HloValueSemanticLabel::kRandom &&
        other_semantics.label() != HloValueSemanticLabel::kWeight &&
        other_semantics.label() != HloValueSemanticLabel::kActivation &&
        other_semantics.label() != HloValueSemanticLabel::kActivationGradient);
  return CopySemantics(weight_gradient_semantics);
}

StatusOr<HloValueSemantics>
HloValueSemanticsPropagation::ComputeSemanticsFromOperands(
    HloInstruction* instruction, absl::Span<const int64_t> operand_indices,
    absl::Span<const ShapeIndex> operand_shape_indices) const {
  CHECK(!operand_indices.empty());
  CHECK(operand_shape_indices.empty() ||
        operand_indices.size() == operand_shape_indices.size());
  VLOG(3) << __func__ << ", instruction: " << instruction->ToString();
  std::vector<HloValueSemantics> semantics_vec;
  for (int64_t operand_index : operand_indices) {
    const HloInstruction* operand = instruction->operand(operand_index);
    const HloValueSemantics* operand_semantics = analysis_->GetSemantics(
        operand, operand_shape_indices.empty()
                     ? ShapeIndex()
                     : operand_shape_indices[operand_index]);
    VLOG(3) << __func__ << ", operand_index: " << operand_index
            << ", operand: " << operand->name()
            << ", operand_semantics: " << operand_semantics->ToString();
    semantics_vec.push_back(*operand_semantics);
  }
  while (semantics_vec.size() >= 2) {
    absl::Span<const HloValueSemantics> operand_list =
        absl::MakeConstSpan(semantics_vec).subspan(semantics_vec.size() - 2, 2);
    auto find_operand_index_with_label =
        [&operand_list](
            HloValueSemanticLabel label) -> std::optional<int64_t> {
      auto iter = absl::c_find_if(operand_list,
                                  [label](const HloValueSemantics& operand) {
                                    return operand.label() == label;
                                  });
      return (iter != operand_list.end())
                 ? std::optional<int64_t>(
                       std::distance(operand_list.begin(), iter))
                 : std::nullopt;
    };
    auto replace_operands_semantics_with =
        [&semantics_vec](const HloValueSemantics& result_semantics) {
          semantics_vec.pop_back();
          semantics_vec.pop_back();
          semantics_vec.push_back(result_semantics);
        };
    if (auto index =
            find_operand_index_with_label(HloValueSemanticLabel::kStatic)) {
      TF_ASSIGN_OR_RETURN(
          HloValueSemantics semantics,
          ComputeSemanticsFromStaticAndOther(
              operand_list[*index], operand_list[1 - *index], instruction));
      replace_operands_semantics_with(semantics);
      continue;
    }
    if (auto index =
            find_operand_index_with_label(HloValueSemanticLabel::kRandom)) {
      TF_ASSIGN_OR_RETURN(
          HloValueSemantics semantics,
          ComputeSemanticsFromRandomAndOther(
              operand_list[*index], operand_list[1 - *index], instruction));
      replace_operands_semantics_with(semantics);
      continue;
    }
    if (auto index =
            find_operand_index_with_label(HloValueSemanticLabel::kWeight)) {
      TF_ASSIGN_OR_RETURN(
          HloValueSemantics semantics,
          ComputeSemanticsFromWeightAndOther(
              operand_list[*index], operand_list[1 - *index], instruction));
      replace_operands_semantics_with(semantics);
      continue;
    }
    if (auto index =
            find_operand_index_with_label(HloValueSemanticLabel::kActivation)) {
      TF_ASSIGN_OR_RETURN(
          HloValueSemantics semantics,
          ComputeSemanticsFromActivationAndOther(
              operand_list[*index], operand_list[1 - *index], instruction));
      replace_operands_semantics_with(semantics);
      continue;
    }
    if (auto index = find_operand_index_with_label(
            HloValueSemanticLabel::kActivationGradient)) {
      TF_ASSIGN_OR_RETURN(
          HloValueSemantics semantics,
          ComputeSemanticsFromActivationGradientAndOther(
              operand_list[*index], operand_list[1 - *index], instruction));
      replace_operands_semantics_with(semantics);
      continue;
    }
    if (auto index = find_operand_index_with_label(
            HloValueSemanticLabel::kWeightGradient)) {
      TF_ASSIGN_OR_RETURN(
          HloValueSemantics semantics,
          ComputeSemanticsFromWeightGradientAndOther(
              operand_list[*index], operand_list[1 - *index], instruction));
      replace_operands_semantics_with(semantics);
      continue;
    }
    LOG(FATAL) << "We don't expect to handle operands of label "
               << HloValueSemanticLabelToString(operand_list[0].label())
               << " and "
               << HloValueSemanticLabelToString(operand_list[1].label())
               << " in ComputeSemanticsFromOperands. Instruction: "
               << instruction->name()
               << " should be handled in its own handler instead of the "
                  "default handler.";
  }
  VLOG(3) << __func__
          << ", result semantics: " << semantics_vec.back().ToString();
  return semantics_vec.back();
}

#define RETURN_IF_ALREADY_PROPAGATED(instruction) \
  if (analysis_->HasSemanticsFor(instruction)) {  \
    return OkStatus();                            \
  }

Status HloValueSemanticsPropagation::DefaultAction(
    HloInstruction* instruction) {
  RETURN_IF_ALREADY_PROPAGATED(instruction);
  std::vector<int64_t> operand_indices(instruction->operand_count());
  std::iota(operand_indices.begin(), operand_indices.end(), 0);
  TF_ASSIGN_OR_RETURN(
      HloValueSemantics semantics,
      ComputeSemanticsFromOperands(instruction, operand_indices));
  const HloValueSemantics* semantics_ptr = AddSemantics(semantics);
  ShapeTree<const HloValueSemantics*> semantics_shape_tree(instruction->shape(),
                                                           semantics_ptr);
  analysis_->SetHloValueSemantics(instruction, semantics_shape_tree);
  return OkStatus();
}

Status HloValueSemanticsPropagation::HandleParameter(
    HloInstruction* parameter) {
  return OkStatus();
}

Status HloValueSemanticsPropagation::HandleConstant(HloInstruction* constant) {
  RETURN_IF_ALREADY_PROPAGATED(constant);
  const HloValueSemantics* constant_semantics = analysis_->NewHloValueSemantics(
      HloValueSemanticLabel::kStatic, {constant, {}});
  ShapeTree<const HloValueSemantics*> semantics_shape_tree(constant->shape(),
                                                           constant_semantics);
  analysis_->SetHloValueSemantics(constant, semantics_shape_tree);
  return OkStatus();
}

Status HloValueSemanticsPropagation::HandleIota(HloInstruction* iota) {
  RETURN_IF_ALREADY_PROPAGATED(iota);
  const HloValueSemantics* semantics = analysis_->NewHloValueSemantics(
      HloValueSemanticLabel::kStatic, {iota, {}});
  ShapeTree<const HloValueSemantics*> semantics_shape_tree(iota->shape(),
                                                           semantics);
  analysis_->SetHloValueSemantics(iota, semantics_shape_tree);
  return OkStatus();
}

Status HloValueSemanticsPropagation::HandlePartitionId(
    HloInstruction* partition_id) {
  RETURN_IF_ALREADY_PROPAGATED(partition_id);
  const HloValueSemantics* semantics = analysis_->NewHloValueSemantics(
      HloValueSemanticLabel::kStatic, {partition_id, {}});
  ShapeTree<const HloValueSemantics*> semantics_shape_tree(
      partition_id->shape(), semantics);
  analysis_->SetHloValueSemantics(partition_id, semantics_shape_tree);
  return OkStatus();
}
Status HloValueSemanticsPropagation::HandleReplicaId(
    HloInstruction* replica_id) {
  RETURN_IF_ALREADY_PROPAGATED(replica_id);
  const HloValueSemantics* semantics = analysis_->NewHloValueSemantics(
      HloValueSemanticLabel::kStatic, {replica_id, {}});
  ShapeTree<const HloValueSemantics*> semantics_shape_tree(replica_id->shape(),
                                                           semantics);
  analysis_->SetHloValueSemantics(replica_id, semantics_shape_tree);
  return OkStatus();
}

Status HloValueSemanticsPropagation::HandleRngBitGenerator(
    HloInstruction* rng_bit_generator) {
  const HloValueSemantics* semantics = analysis_->NewHloValueSemantics(
      HloValueSemanticLabel::kRandom, {rng_bit_generator, {}});
  ShapeTree<const HloValueSemantics*> rbg_semantics_tree(
      rng_bit_generator->shape(), semantics);
  analysis_->SetHloValueSemantics(rng_bit_generator, rbg_semantics_tree);
  return OkStatus();
}

Status HloValueSemanticsPropagation::HandleClamp(HloInstruction* clamp) {
  RETURN_IF_ALREADY_PROPAGATED(clamp);
  const ShapeTree<const HloValueSemantics*>& operand_semantics =
      analysis_->GetInstructionSemantics(clamp->operand(1));
  analysis_->DeepCopyHloValueSemantics(clamp, operand_semantics);
  return OkStatus();
}

Status HloValueSemanticsPropagation::HandleTuple(HloInstruction* tuple) {
  RETURN_IF_ALREADY_PROPAGATED(tuple);
  ShapeTree<const HloValueSemantics*> semantics_shape_tree(tuple->shape(),
                                                           nullptr);
  for (int operand_index = 0; operand_index < tuple->operand_count();
       ++operand_index) {
    const HloInstruction* operand = tuple->operand(operand_index);
    const ShapeTree<const HloValueSemantics*>& operand_semantics =
        analysis_->GetInstructionSemantics(operand);
    analysis_->DeepCopyHloValueSemantics(
        semantics_shape_tree, operand_semantics, {}, {operand_index});
  }
  semantics_shape_tree.ForEachMutableElement(
      [tuple, this](const ShapeIndex& index,
                    const HloValueSemantics** semantics) {
        if (index.empty()) {
          *semantics = analysis_->NewHloValueSemantics(
              HloValueSemanticLabel::kTupleOrToken, {tuple, {}});
          return;
        }
      });
  analysis_->SetHloValueSemantics(tuple, semantics_shape_tree);
  return OkStatus();
}

Status HloValueSemanticsPropagation::HandleGetTupleElement(
    HloInstruction* get_tuple_element) {
  RETURN_IF_ALREADY_PROPAGATED(get_tuple_element);
  const HloInstruction* tuple = get_tuple_element->operand(0);
  int64_t tuple_index = get_tuple_element->tuple_index();
  const ShapeTree<const HloValueSemantics*>& tuple_semantics =
      analysis_->GetInstructionSemantics(tuple);
  TF_ASSIGN_OR_RETURN(
      ShapeTree<const HloValueSemantics*> tuple_element_semantics,
      tuple_semantics.SubShapeTree({tuple_index}));
  analysis_->DeepCopyHloValueSemantics(get_tuple_element,
                                       tuple_element_semantics);
  return OkStatus();
}

Status HloValueSemanticsPropagation::HandleCall(HloInstruction* call) {
  RETURN_IF_ALREADY_PROPAGATED(call);
  HloComputation* computation = call->called_computations()[0];
  TF_RETURN_IF_ERROR(
      analysis_->RunOnComputation(*computation, call->operands()));
  const ShapeTree<const HloValueSemantics*>& root_semantics =
      analysis_->GetInstructionSemantics(computation->root_instruction());
  analysis_->DeepCopyHloValueSemantics(call, root_semantics);
  return OkStatus();
}

Status HloValueSemanticsPropagation::HandleFusion(HloInstruction* fusion) {
  RETURN_IF_ALREADY_PROPAGATED(fusion);
  HloComputation* computation = fusion->called_computations()[0];
  TF_RETURN_IF_ERROR(
      analysis_->RunOnComputation(*computation, fusion->operands()));
  const ShapeTree<const HloValueSemantics*>& root_semantics =
      analysis_->GetInstructionSemantics(computation->root_instruction());
  analysis_->DeepCopyHloValueSemantics(fusion, root_semantics);
  return OkStatus();
}

Status HloValueSemanticsPropagation::HandleWhile(HloInstruction* xla_while) {
  RETURN_IF_ALREADY_PROPAGATED(xla_while);
  TF_RETURN_IF_ERROR(analysis_->RunOnComputation(*xla_while->while_condition(),
                                                 xla_while->operands()));
  HloComputation* computation = xla_while->while_body();
  TF_RETURN_IF_ERROR(
      analysis_->RunOnComputation(*computation, xla_while->operands()));
  const ShapeTree<const HloValueSemantics*>& root_semantics =
      analysis_->GetInstructionSemantics(computation->root_instruction());
  analysis_->DeepCopyHloValueSemantics(xla_while, root_semantics);
  return OkStatus();
}

Status HloValueSemanticsPropagation::HandleCustomCall(
    HloInstruction* custom_call) {
  RETURN_IF_ALREADY_PROPAGATED(custom_call);
  if (custom_call->custom_call_target() == "Sharding" ||
      custom_call->custom_call_target() == "SPMDFullToShardShape" ||
      custom_call->custom_call_target() == "SPMDShardToFullShape") {
    const ShapeTree<const HloValueSemantics*>& operand_semantics =
        analysis_->GetInstructionSemantics(custom_call->operand(0));
    analysis_->DeepCopyHloValueSemantics(custom_call, operand_semantics);
    return OkStatus();
  }
  return Unimplemented("Unimplemented custom-call: %s",
                       custom_call->custom_call_target());
}

Status HloValueSemanticsPropagation::HandleConditional(
    HloInstruction* conditional) {
  RETURN_IF_ALREADY_PROPAGATED(conditional);
  for (int i = 0; i < conditional->called_computations().size(); ++i) {
    TF_RETURN_IF_ERROR(
        analysis_->RunOnComputation(*conditional->called_computations()[i],
                                    {conditional->operands()[i + 1]}));
  }
  HloComputation* computation = conditional->called_computations()[0];
  const ShapeTree<const HloValueSemantics*>& root_semantics =
      analysis_->GetInstructionSemantics(computation->root_instruction());
  analysis_->DeepCopyHloValueSemantics(conditional, root_semantics);
  return OkStatus();
}

Status HloValueSemanticsPropagation::HandleSelect(HloInstruction* select) {
  RETURN_IF_ALREADY_PROPAGATED(select);
  TF_ASSIGN_OR_RETURN(HloValueSemantics semantics,
                      ComputeSemanticsFromOperands(select, {1, 2}));
  const HloValueSemantics* semantics_ptr = AddSemantics(semantics);
  ShapeTree<const HloValueSemantics*> semantics_shape_tree(select->shape(),
                                                           semantics_ptr);
  analysis_->SetHloValueSemantics(select, semantics_shape_tree);
  return OkStatus();
}

Status HloValueSemanticsPropagation::HandleConcatenate(
    HloInstruction* concatenate) {
  RETURN_IF_ALREADY_PROPAGATED(concatenate);
  const ShapeTree<const HloValueSemantics*>& operand_semantics =
      analysis_->GetInstructionSemantics(concatenate->operand(0));
  analysis_->DeepCopyHloValueSemantics(concatenate, operand_semantics);
  return OkStatus();
}

Status HloValueSemanticsPropagation::HandleDynamicSlice(
    HloInstruction* dynamic_slice) {
  RETURN_IF_ALREADY_PROPAGATED(dynamic_slice);
  const HloInstruction* dynamic_slice_operand = dynamic_slice->operand(0);
  const HloValueSemantics* operand_semantics =
      analysis_->GetSemantics(dynamic_slice_operand);
  const HloValueSemantics* semantics = AddSemantics(*operand_semantics);
  ShapeTree<const HloValueSemantics*> semantics_shape_tree(
      dynamic_slice->shape(), semantics);
  analysis_->SetHloValueSemantics(dynamic_slice, semantics_shape_tree);
  return OkStatus();
}

Status HloValueSemanticsPropagation::HandleDynamicUpdateSlice(
    HloInstruction* dynamic_update_slice) {
  RETURN_IF_ALREADY_PROPAGATED(dynamic_update_slice);
  TF_ASSIGN_OR_RETURN(
      HloValueSemantics semantics,
      ComputeSemanticsFromOperands(dynamic_update_slice, {0, 1}));
  const HloValueSemantics* semantics_ptr = AddSemantics(semantics);
  ShapeTree<const HloValueSemantics*> semantics_shape_tree(
      dynamic_update_slice->shape(), semantics_ptr);
  analysis_->SetHloValueSemantics(dynamic_update_slice, semantics_shape_tree);
  return OkStatus();
}

Status HloValueSemanticsPropagation::HandleCopyStart(
    HloInstruction* copy_start) {
  RETURN_IF_ALREADY_PROPAGATED(copy_start);
  ShapeTree<const HloValueSemantics*> semantics_shape_tree(copy_start->shape());
  const ShapeTree<const HloValueSemantics*>& operand_semantics_shape_tree =
      analysis_->GetInstructionSemantics(copy_start->operand(0));
  analysis_->DeepCopyHloValueSemantics(semantics_shape_tree,
                                       operand_semantics_shape_tree, {}, {0});
  analysis_->DeepCopyHloValueSemantics(semantics_shape_tree,
                                       operand_semantics_shape_tree, {}, {1});
  semantics_shape_tree.ForEachMutableElement(
      [this, copy_start](const ShapeIndex& shape_index,
                         const HloValueSemantics** semantics) {
        if (shape_index.empty()) {
          *semantics = analysis_->NewHloValueSemantics(
              HloValueSemanticLabel::kTupleOrToken, {copy_start, shape_index});
        }
        if (shape_index == ShapeIndex{2}) {
          *semantics = analysis_->NewHloValueSemantics(
              HloValueSemanticLabel::kRandom, {copy_start, shape_index});
        }
        if (shape_index == ShapeIndex{3}) {
          *semantics = analysis_->NewHloValueSemantics(
              HloValueSemanticLabel::kRandom, {copy_start, shape_index});
        }
      });
  analysis_->SetHloValueSemantics(copy_start, semantics_shape_tree);
  return OkStatus();
}

Status HloValueSemanticsPropagation::HandleCopyDone(HloInstruction* copy_done) {
  RETURN_IF_ALREADY_PROPAGATED(copy_done);
  const ShapeTree<const HloValueSemantics*>& operand_semantics_shape_tree =
      analysis_->GetInstructionSemantics(copy_done->operand(0));
  analysis_->DeepCopyHloValueSemantics(copy_done, operand_semantics_shape_tree,
                                       {0});
  return OkStatus();
}
Status HloValueSemanticsPropagation::HandleCollectivePermuteStart(
    HloInstruction* collective_permute_start) {
  RETURN_IF_ALREADY_PROPAGATED(collective_permute_start);
  ShapeTree<const HloValueSemantics*> semantics_shape_tree(
      collective_permute_start->shape());
  const ShapeTree<const HloValueSemantics*>& operand_semantics_shape_tree =
      analysis_->GetInstructionSemantics(collective_permute_start->operand(0));
  analysis_->DeepCopyHloValueSemantics(semantics_shape_tree,
                                       operand_semantics_shape_tree, {}, {0});
  analysis_->DeepCopyHloValueSemantics(semantics_shape_tree,
                                       operand_semantics_shape_tree, {}, {1});
  semantics_shape_tree.ForEachMutableElement(
      [this, collective_permute_start](const ShapeIndex& shape_index,
                                       const HloValueSemantics** semantics) {
        if (shape_index.empty()) {
          *semantics = analysis_->NewHloValueSemantics(
              HloValueSemanticLabel::kTupleOrToken,
              {collective_permute_start, {}});
        }
        if (shape_index == ShapeIndex{2}) {
          *semantics = analysis_->NewHloValueSemantics(
              HloValueSemanticLabel::kRandom,
              {collective_permute_start, shape_index});
        }
        if (shape_index == ShapeIndex{3}) {
          *semantics = analysis_->NewHloValueSemantics(
              HloValueSemanticLabel::kRandom,
              {collective_permute_start, shape_index});
        }
      });
  analysis_->SetHloValueSemantics(collective_permute_start,
                                  semantics_shape_tree);
  return OkStatus();
}
Status HloValueSemanticsPropagation::HandleCollectivePermuteDone(
    HloInstruction* collective_permute_done) {
  RETURN_IF_ALREADY_PROPAGATED(collective_permute_done);
  const ShapeTree<const HloValueSemantics*>& operand_semantics_shape_tree =
      analysis_->GetInstructionSemantics(collective_permute_done->operand(0));
  analysis_->DeepCopyHloValueSemantics(collective_permute_done,
                                       operand_semantics_shape_tree, {1});
  return OkStatus();
}
Status HloValueSemanticsPropagation::HandleGather(HloInstruction* gather) {
  RETURN_IF_ALREADY_PROPAGATED(gather);
  const ShapeTree<const HloValueSemantics*>& operand_semantics_shape_tree =
      analysis_->GetInstructionSemantics(gather->operand(0));
  analysis_->DeepCopyHloValueSemantics(gather, operand_semantics_shape_tree);
  return OkStatus();
}

Status HloValueSemanticsPropagation::HandleScatter(HloInstruction* scatter) {
  RETURN_IF_ALREADY_PROPAGATED(scatter);
  TF_ASSIGN_OR_RETURN(HloValueSemantics semantics,
                      ComputeSemanticsFromOperands(scatter, {0, 2}));
  const HloValueSemantics* semantics_ptr = AddSemantics(semantics);
  ShapeTree<const HloValueSemantics*> semantics_shape_tree(scatter->shape(),
                                                           semantics_ptr);
  analysis_->SetHloValueSemantics(scatter, semantics_shape_tree);
  return OkStatus();
}

Status HloValueSemanticsPropagation::HandleAfterAll(HloInstruction* after_all) {
  RETURN_IF_ALREADY_PROPAGATED(after_all);
  const HloValueSemantics* semantics = analysis_->NewHloValueSemantics(
      HloValueSemanticLabel::kTupleOrToken, {after_all, {}});
  ShapeTree<const HloValueSemantics*> semantics_shape_tree(after_all->shape(),
                                                           semantics);
  analysis_->SetHloValueSemantics(after_all, semantics_shape_tree);
  return OkStatus();
}

Status HloValueSemanticsPropagation::HandleAsyncStart(
    HloInstruction* async_start) {
  RETURN_IF_ALREADY_PROPAGATED(async_start);
  const HloValueSemantics* semantics = analysis_->NewHloValueSemantics(
      HloValueSemanticLabel::kTupleOrToken, {async_start, {}});
  ShapeTree<const HloValueSemantics*> semantics_shape_tree(async_start->shape(),
                                                           semantics);
  for (int operand_index = 0; operand_index < async_start->operand_count();
       ++operand_index) {
    HloInstruction* operand = async_start->mutable_operand(operand_index);
    const ShapeTree<const HloValueSemantics*>& operand_semantics_tree =
        analysis_->GetInstructionSemantics(operand);
    analysis_->DeepCopyHloValueSemantics(
        semantics_shape_tree, operand_semantics_tree, {}, {0, operand_index});
  }
  std::vector<int64_t> operand_indices(async_start->operand_count());
  std::iota(operand_indices.begin(), operand_indices.end(), 0);
  TF_ASSIGN_OR_RETURN(
      HloValueSemantics output_semantics,
      ComputeSemanticsFromOperands(async_start, operand_indices));
  semantics_shape_tree.ForEachMutableElement(
      [&output_semantics, &semantics_shape_tree, this, async_start](
          const ShapeIndex& index, const HloValueSemantics** semantics_ptr) {
        if (index.empty() || index.front() == 0) {
          return;
        }
        if (!semantics_shape_tree.IsLeaf(index)) {
          *semantics_ptr = analysis_->NewHloValueSemantics(
              HloValueSemanticLabel::kTupleOrToken, {async_start, {}});
          return;
        }
        if (index.front() == 1) {
          *semantics_ptr = AddSemantics(output_semantics);
          return;
        }
        if (index.front() == 2) {
          *semantics_ptr = analysis_->NewHloValueSemantics(
              HloValueSemanticLabel::kRandom, {async_start, {}});
        }
      });
  analysis_->SetHloValueSemantics(async_start, semantics_shape_tree);
  return OkStatus();
}
Status HloValueSemanticsPropagation::HandleAsyncDone(
    HloInstruction* async_done) {
  RETURN_IF_ALREADY_PROPAGATED(async_done);
  const ShapeTree<const HloValueSemantics*>& operand_semantics_tree =
      analysis_->GetInstructionSemantics(async_done->operand(0));
  analysis_->DeepCopyHloValueSemantics(async_done, operand_semantics_tree, {1});
  return OkStatus();
}

Status HloValueSemanticsPropagation::HandleInfeed(HloInstruction* infeed) {
  RETURN_IF_ALREADY_PROPAGATED(infeed);
  ShapeTree<const HloValueSemantics*> semantics_shape_tree(infeed->shape(),
                                                           nullptr);
  semantics_shape_tree.ForEachMutableElement(
      [this, &semantics_shape_tree, infeed](
          const ShapeIndex& shape_index, const HloValueSemantics** semantics) {
        if (semantics_shape_tree.IsLeaf(shape_index)) {
          *semantics = analysis_->NewHloValueSemantics(
              HloValueSemanticLabel::kWeight, {infeed, shape_index});
        } else {
          *semantics = analysis_->NewHloValueSemantics(
              HloValueSemanticLabel::kTupleOrToken, {infeed, shape_index});
        }
      });
  analysis_->SetHloValueSemantics(infeed, semantics_shape_tree);
  return OkStatus();
}

Status HloValueSemanticsPropagation::HandleDomain(HloInstruction* domain) {
  RETURN_IF_ALREADY_PROPAGATED(domain);
  HloInstruction* domain_operand = domain->mutable_operand(0);
  const ShapeTree<const HloValueSemantics*>& operand_semantics =
      analysis_->GetInstructionSemantics(domain_operand);
  analysis_->DeepCopyHloValueSemantics(domain, operand_semantics);
  return OkStatus();
}

Status HloValueSemanticsPropagation::HandleOptimizationBarrier(
    HloInstruction* opt_barrier) {
  RETURN_IF_ALREADY_PROPAGATED(opt_barrier);
  HloInstruction* opt_barrier_operand = opt_barrier->mutable_operand(0);
  const ShapeTree<const HloValueSemantics*>& operand_semantics =
      analysis_->GetInstructionSemantics(opt_barrier_operand);
  analysis_->DeepCopyHloValueSemantics(opt_barrier, operand_semantics);
  return OkStatus();
}

Status HloValueSemanticsPropagation::HandleSend(HloInstruction* send) {
  RETURN_IF_ALREADY_PROPAGATED(send);
  ShapeTree<const HloValueSemantics*> semantics_tree(send->shape(), nullptr);
  HloInstruction* source_buffer = send->mutable_operand(0);
  const ShapeTree<const HloValueSemantics*>& source_buffer_semantics =
      analysis_->GetInstructionSemantics(source_buffer);
  analysis_->DeepCopyHloValueSemantics(semantics_tree, source_buffer_semantics,
                                       {}, {0});

  semantics_tree.ForEachMutableElement(
      [this, send, &semantics_tree](const ShapeIndex& index,
                                    const HloValueSemantics** semantics) {
        if (!index.empty()) {
          if (index.front() == 1 && semantics_tree.IsLeaf(index)) {
            *semantics = analysis_->NewHloValueSemantics(
                HloValueSemanticLabel::kRandom, {send, index});
            return;
          }
          if (index.front() == 0) {
            return;
          }
        }
        *semantics = analysis_->NewHloValueSemantics(
            HloValueSemanticLabel::kTupleOrToken, {send, index});
      });
  analysis_->SetHloValueSemantics(send, semantics_tree);
  return OkStatus();
}

Status HloValueSemanticsPropagation::HandleRecv(HloInstruction* recv) {
  // Since recv is not a prerequisite of send, we might have not propagated
  // semantics to the corresponding send when we reach this recv. So we visit
  // the send first before visiting this recv.
  // We use RETURN_IF_ALREADY_PROPAGATED to avoid processing an HLO more than
  // once.
  RETURN_IF_ALREADY_PROPAGATED(recv);
  TF_ASSIGN_OR_RETURN(HloInstruction * send,
                      analysis_->GetMatchingSendOrRecv(recv));
  TF_RETURN_IF_ERROR(send->Accept(this));
  ShapeTree<const HloValueSemantics*> semantics_tree(recv->shape(), nullptr);
  const ShapeTree<const HloValueSemantics*>& send_buffer_semantics =
      analysis_->GetInstructionSemantics(send);
  analysis_->DeepCopyHloValueSemantics(semantics_tree, send_buffer_semantics,
                                       {0}, {0});
  semantics_tree.ForEachMutableElement(
      [this, recv, &semantics_tree](const ShapeIndex& index,
                                    const HloValueSemantics** semantics) {
        if (!index.empty()) {
          if (index.front() == 1 && semantics_tree.IsLeaf(index)) {
            *semantics = analysis_->NewHloValueSemantics(
                HloValueSemanticLabel::kRandom, {recv, index});
            return;
          }
          if (index.front() == 0) {
            return;
          }
        }
        *semantics = analysis_->NewHloValueSemantics(
            HloValueSemanticLabel::kTupleOrToken, {recv, index});
      });
  analysis_->SetHloValueSemantics(recv, semantics_tree);
  return OkStatus();
}

Status HloValueSemanticsPropagation::HandleSendDone(HloInstruction* send_done) {
  RETURN_IF_ALREADY_PROPAGATED(send_done);
  const HloValueSemantics* semantics = analysis_->NewHloValueSemantics(
      HloValueSemanticLabel::kTupleOrToken, {send_done, {}});
  ShapeTree<const HloValueSemantics*> send_done_semantics_tree(
      send_done->shape(), semantics);
  analysis_->SetHloValueSemantics(send_done, send_done_semantics_tree);
  return OkStatus();
}
Status HloValueSemanticsPropagation::HandleRecvDone(HloInstruction* recv_done) {
  RETURN_IF_ALREADY_PROPAGATED(recv_done);
  ShapeTree<const HloValueSemantics*> semantics_tree(recv_done->shape(),
                                                     nullptr);
  HloInstruction* recv = recv_done->mutable_operand(0);
  const ShapeTree<const HloValueSemantics*>& recv_semantics =
      analysis_->GetInstructionSemantics(recv);
  analysis_->DeepCopyHloValueSemantics(semantics_tree, recv_semantics, {0},
                                       {0});
  semantics_tree.ForEachMutableElement(
      [this, recv_done](const ShapeIndex& index,
                        const HloValueSemantics** semantics) {
        if (!index.empty() && index.front() == 0) {
          return;
        }
        *semantics = analysis_->NewHloValueSemantics(
            HloValueSemanticLabel::kTupleOrToken, {recv_done, index});
      });
  analysis_->SetHloValueSemantics(recv_done, semantics_tree);
  return OkStatus();
}

}  // namespace xla
