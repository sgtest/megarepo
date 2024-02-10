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

#include "xla/service/while_loop_unroller.h"

#include <cstdint>
#include <iterator>
#include <memory>
#include <optional>
#include <utility>
#include <vector>

#include "absl/algorithm/algorithm.h"
#include "absl/algorithm/container.h"
#include "absl/container/flat_hash_map.h"
#include "absl/container/flat_hash_set.h"
#include "absl/log/check.h"
#include "absl/log/log.h"
#include "absl/strings/str_cat.h"
#include "absl/strings/string_view.h"
#include "xla/hlo/evaluator/hlo_evaluator.h"
#include "xla/hlo/ir/hlo_computation.h"
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/hlo/ir/hlo_module.h"
#include "xla/hlo/ir/hlo_opcode.h"
#include "xla/hlo/utils/hlo_query.h"
#include "xla/literal.h"
#include "xla/literal_util.h"
#include "xla/overflow_util.h"
#include "xla/primitive_util.h"
#include "xla/service/call_inliner.h"
#include "xla/service/collective_ops_utils.h"
#include "xla/service/flatten_call_graph.h"
#include "xla/service/hlo_cse.h"
#include "xla/service/hlo_pass_fix.h"
#include "xla/service/tuple_simplifier.h"
#include "xla/service/while_loop_analysis.h"
#include "xla/service/while_loop_constant_sinking.h"
#include "xla/shape.h"
#include "xla/statusor.h"
#include "xla/util.h"
#include "xla/xla_data.pb.h"
#include "tsl/platform/errors.h"
#include "tsl/platform/statusor.h"

namespace xla {
namespace {

using hlo_query::ContainsInstrWithOpcode;

// Parameters for the unroller that can be adjusted.
const int kUnrollTripCountThreshold = 64;
const int kUnrollInstructionCountThreshold = 800;
const int kUnrollExpandFactorThreshold = 10000;

// The following sequence of passes are necessary to prepare loops for
// unrolling. Failure to run these passes will prevent unroller from unrolling
// loops that would have been otherwise unrollable.
//
// Instead of placing these passes in compiler, they are placed
// here to indicate explicit dependency to these passes.
StatusOr<bool> PrepareModuleForUnrolling(
    HloModule* module,
    const absl::flat_hash_set<absl::string_view>& execution_threads) {
  bool changed = false;
  TF_ASSIGN_OR_RETURN(
      bool applied_cse,
      HloCSE{/*is_layout_sensitive=*/true}.Run(module, execution_threads));
  if (applied_cse) {
    changed = true;
    VLOG(3) << "Applied hlo cse to module " << module->name();
  }

  TF_ASSIGN_OR_RETURN(bool applied_tuple_simplifier,
                      TupleSimplifier{}.Run(module, execution_threads));
  if (applied_tuple_simplifier) {
    changed = true;
    VLOG(3) << "Applied tuple simplifier to module " << module->name();
  }

  // We apply constant sinking to fix point.
  HloPassFix<WhileLoopConstantSinking> constant_sinking(
      /*sink_broadcast_of_constants=*/true);
  TF_ASSIGN_OR_RETURN(bool applied_constant_sinking,
                      constant_sinking.Run(module, execution_threads));
  if (applied_constant_sinking) {
    changed = true;
    VLOG(3) << "Applied constant sinking to module " << module->name();
  }
  return changed;
}

// A utility function that decides whether a loop is unrollable or not.
std::optional<WhileLoopConfig> IsLoopUnrollable(HloInstruction* while_op) {
  CHECK_EQ(while_op->opcode(), HloOpcode::kWhile);

  // TODO(b/300668690): Add support for unrolling loops with control dependency.
  // For now, we bail.
  //
  // Finding all the while loops where other instructions have explicit control
  // dependencies on them.
  std::vector<HloInstruction*> while_dependees;
  for (HloComputation* comp : while_op->GetModule()->computations()) {
    for (HloInstruction* instr : comp->instructions()) {
      for (HloInstruction* control_dep : instr->control_predecessors()) {
        if (control_dep->opcode() == HloOpcode::kWhile) {
          while_dependees.push_back(control_dep);
        }
      }
    }
  }
  if (absl::linear_search(while_dependees.begin(), while_dependees.end(),
                          while_op)) {
    VLOG(2) << "Not attempting to unroll " << while_op->name()
            << " due to control dependency: " << while_op->ToShortString();
    return std::nullopt;
  }

  // We can't remove while loops that contain send/recv nodes, because we
  // rely on the particular loop structure around the node matching on the
  // send and recv sides.
  if (ContainsInstrWithOpcode(while_op->while_body(),
                              {HloOpcode::kSend, HloOpcode::kSendDone,
                               HloOpcode::kRecv, HloOpcode::kRecvDone}) ||
      ContainsInstrWithOpcode(while_op->while_condition(),
                              {HloOpcode::kSend, HloOpcode::kSendDone,
                               HloOpcode::kRecv, HloOpcode::kRecvDone})) {
    VLOG(2) << "Not attempting to unroll " << while_op->name()
            << " because it contains a send/recv node: "
            << while_op->ToShortString();
    return std::nullopt;
  }

  if (while_op->operand(0)->opcode() != HloOpcode::kTuple) {
    VLOG(2) << "Not attempting to unroll " << while_op->name()
            << " because the operand is not a tuple: "
            << while_op->ToShortString();
    return std::nullopt;
  }

  // We cannot unroll loops that have side effecting condition because the
  // condition will be removed after unrolling. This might be relaxed
  // later when we add partial unrolling.
  if (while_op->while_condition()->HasSideEffect()) {
    VLOG(2) << "Not attempting to remove while loop whose condition contains "
               "side-effecting instructions: "
            << while_op->ToShortString();
    return std::nullopt;
  }

  std::optional<int64_t> indvar_tuple_idx =
      GetLoopInductionVarTupleIdx(while_op);
  if (!indvar_tuple_idx.has_value()) {
    return std::nullopt;
  }

  HloEvaluator evaluator(/*max_loop_iterations=*/0);
  const HloInstruction* while_init = while_op->operand(0);
  const HloInstruction* indvar_init = while_init->operand(*indvar_tuple_idx);
  StatusOr<Literal> indvar_init_result = evaluator.Evaluate(indvar_init);
  if (!indvar_init_result.ok()) {
    VLOG(2) << "Couldn't evaluate induction variable init, "
            << indvar_init_result.status() << ", " << indvar_init->ToString();
    return std::nullopt;
  }
  Literal indvar_iter_val = std::move(indvar_init_result).value();

  std::optional<int64_t> trip_count =
      MatchTrivialLoopTripCount(while_op, *indvar_tuple_idx, indvar_iter_val);
  if (!trip_count.has_value()) {
    return std::nullopt;
  }

  VLOG(3) << "Loop trip count " << trip_count.value();

  WhileLoopConfig config;
  config.init =
      LiteralUtil::LiteralAsScalarInt64(std::move(indvar_iter_val)).value();
  config.trip_count = trip_count.value();
  config.induction_var_idx = *indvar_tuple_idx;

  return config;
}

// Helper function that replaces a single iteration of a while loop with
// induction variable equal to induction_value.
StatusOr<std::unique_ptr<HloComputation>> UnrollSingleIterationOfTrivialLoop(
    HloInstruction* while_op, const int64_t indvar_idx,
    const int64_t induction_value) {
  // We clone the body since we are changing the computation.
  std::unique_ptr<HloComputation> while_body_clone =
      while_op->while_body()->Clone(absl::StrCat(induction_value));

  const HloInstruction* induction_var_hlo =
      while_op->operand(0)->operand(indvar_idx);

  // We record the next channel id to utilize when unrolling loops with
  // collective communication instructions. During unrolling a single iteration
  // of the body, we can reuse the same unique_channel_id. For the later
  // iterations, we obtain it again.
  int64_t unique_channel_id = hlo_query::NextChannelId(*while_op->GetModule());

  // Go through the instructions in while body to get the instruction that
  // points to the induction var. Then replace it everywhere with the concrete
  // value.
  for (HloInstruction* body_inst : while_body_clone->instructions()) {
    // We need to assign a unique channel_id for the collective ops that are
    // unrolled within the while loop body or fusions containing collectives.
    if (IsCollectiveWithChannelId(body_inst)) {
      // To obtain the channel_id for the collective ops we only need to
      // increment the `unique_channel_id` since it records the next available
      // channel_id across the module.
      body_inst->set_channel_id(unique_channel_id++);
    }

    if (body_inst->opcode() != HloOpcode::kGetTupleElement) {
      continue;
    }
    if (body_inst->operand(0) != while_body_clone->parameter_instruction(0)) {
      continue;
    }
    const int64_t idx = body_inst->tuple_index();

    std::vector<HloInstruction*> body_uses;
    body_uses.reserve(body_inst->users().size());
    for (HloInstruction* indvar_use : body_inst->users()) {
      body_uses.push_back(indvar_use);
    }

    // We found our instruction
    if (idx == indvar_idx) {
      // Finds all the uses of induction var within the while body
      for (HloInstruction* indvar_use : body_uses) {
        for (int64_t i = 0; i < indvar_use->operand_count(); ++i) {
          const HloInstruction* indvar_use_operand = indvar_use->operand(i);
          // Found the induction var as an operand of body instruction.
          if (indvar_use_operand == body_inst) {
            std::unique_ptr<HloInstruction> constant =
                primitive_util::PrimitiveTypeSwitch<
                    std::unique_ptr<HloInstruction>>(
                    [&](auto literal_constant)
                        -> std::unique_ptr<HloInstruction> {
                      if constexpr (primitive_util::IsIntegralType(
                                        literal_constant)) {
                        using NativeT =
                            primitive_util::NativeTypeOf<literal_constant>;
                        return HloInstruction::CreateConstant(
                            LiteralUtil::CreateR0(
                                static_cast<NativeT>(induction_value)));
                      }
                      LOG(FATAL) << "literal is of non-integral type";
                    },
                    induction_var_hlo->shape().element_type());

            // Assign the same shape of the old instruction to the new
            // instruction.
            *constant->mutable_shape() = body_inst->shape();
            CHECK_OK(indvar_use->ReplaceOperandWith(
                i, while_body_clone->AddInstruction(std::move(constant))));
          }
        }
      }
    }
  }
  return while_body_clone;
}

StatusOr<bool> UnrollInternal(HloInstruction* while_op, int64_t unroll_factor,
                              WhileLoopConfig config) {
  CHECK_EQ(while_op->opcode(), HloOpcode::kWhile);

  VLOG(3) << "Trying to unroll " << while_op->ToShortString();

  // TODO(b/288130138): For now, we only support full unrolling. Will add
  // partial unrolling if needed.
  if (unroll_factor != -1) {
    return UnimplementedStrCat(
        "Currently, only full unrolling is supported, unroll factor: ",
        unroll_factor);
  }

  // TODO(b/291628533): Extract this parameter to the unroller config. We don't
  // attempt to unroll loops where the body has more than
  // kUnrollInstructionCountThreshold instructions.
  if (while_op->while_body()->instruction_count() >
      kUnrollInstructionCountThreshold) {
    return FailedPrecondition(
        "%s",
        absl::StrCat(
            "Cannot unroll while loop. Too many instructions in the body: ",
            while_op->while_body()->instruction_count()));
  }

  // TODO(b/291628533): Extract this parameter to the an unroller config. We
  // only unroll loops up to a threshold.
  if (config.trip_count > kUnrollTripCountThreshold) {
    return FailedPrecondition(
        "%s",
        absl::StrCat("Cannot unroll while loop. The tip count is greater "
                     "than the threshold: ",
                     config.trip_count, " vs ", kUnrollTripCountThreshold));
  }

  // TODO(b/291628533): Extract this parameter to the unroller config. We don't
  // unroll loops that increase the instruction count by more than
  // kUnrollExpandFactorThreshold.
  if (config.trip_count * while_op->while_body()->instruction_count() >
      kUnrollExpandFactorThreshold) {
    return FailedPrecondition(
        "%s", absl::StrCat("Not attempting to unroll due to instruction count "
                           "increase explosion. New instruction count: ",
                           config.trip_count *
                               while_op->while_body()->instruction_count(),
                           " vs ", kUnrollExpandFactorThreshold));
  }

  VLOG(3) << "Unrolling while instruction " << while_op->ToShortString()
          << " with body instruction count "
          << while_op->while_body()->instruction_count();

  HloModule* module = while_op->GetModule();
  HloComputation* computation = while_op->parent();
  HloInstruction* unrolled_body_call_op;
  std::vector<HloInstruction*> call_operands;

  // While loop must have a single tuple operand.
  CHECK_EQ(while_op->operands().size(), 1);

  call_operands.emplace_back(while_op->operands().at(0));
  for (int64_t i = config.init; i < config.trip_count + config.init; ++i) {
    CHECK(OverflowSafeAdd(i, (int64_t)1).has_value());

    HloComputation* unrolled_body = module->AddEmbeddedComputation(
        UnrollSingleIterationOfTrivialLoop(while_op, config.induction_var_idx,
                                           i)
            .value());
    unrolled_body_call_op =
        computation->AddInstruction(HloInstruction::CreateCall(
            while_op->shape(), call_operands, unrolled_body));
    call_operands.clear();
    call_operands.emplace_back(unrolled_body_call_op);
  }
  CHECK_OK(computation->ReplaceInstruction(while_op, unrolled_body_call_op));

  // Needed for the nested while loops in which the outer loop has been
  // unrolled which leaves the call graph non-flat.
  TF_RETURN_IF_ERROR(FlattenCallGraph().Run(module).status());
  return true;
}

};  // namespace

absl::flat_hash_map<HloInstruction*, WhileLoopConfig> GetUnrollableLoops(
    HloModule* module,
    const absl::flat_hash_set<absl::string_view>& execution_threads) {
  // Processing the while loops in the reverse topological order. If the body
  // of while loop A calls while loop B, B comes before A.
  std::vector<HloInstruction*> all_while_ops;
  for (auto* comp : module->MakeComputationPostOrder(execution_threads)) {
    absl::c_copy_if(comp->instructions(), std::back_inserter(all_while_ops),
                    HloPredicateIsOp<HloOpcode::kWhile>);
  }

  absl::flat_hash_map<HloInstruction*, WhileLoopConfig> while_loop_configs;
  for (HloInstruction* instr : all_while_ops) {
    std::optional<WhileLoopConfig> config = IsLoopUnrollable(instr);
    if (config.has_value()) {
      while_loop_configs[instr] = *config;
    }
  }
  return while_loop_configs;
}

StatusOr<bool> Unroll(HloInstruction* while_op, int64_t unroll_factor) {
  bool changed = false;
  HloModule* module = while_op->GetModule();

  // Make sure all the necessary passes are executed before unrolling in order
  // to unroll every possible loop.
  TF_ASSIGN_OR_RETURN(
      changed, PrepareModuleForUnrolling(module, /*execution_threads=*/{}));

  // Construct the loop config
  std::optional<WhileLoopConfig> config = IsLoopUnrollable(while_op);
  if (!config.has_value()) {
    return false;
  }

  TF_ASSIGN_OR_RETURN(bool unrolled,
                      UnrollInternal(while_op, unroll_factor, config.value()));

  // We need to inline the calls created for unrolling since later passes rely
  // on the calls to be inlined.
  if (unrolled) {
    TF_RETURN_IF_ERROR(CallInliner().Run(module).status());
  }
  return unrolled;
}

StatusOr<bool> WhileLoopUnroller::Run(
    HloModule* module,
    const absl::flat_hash_set<absl::string_view>& execution_threads) {
  // TODO(b/288130138) For now, we only support full unrolling. Will add partial
  // unrolling if needed.
  if (unroll_factor_ != -1) {
    return false;
  }
  XLA_VLOG_LINES(3, "WhileLoopUnroller::Run(), before:\n" + module->ToString());
  bool changed = false;

  // Make sure all the necessary passes are executed before unrolling in order
  // to unroll every possible loop.
  TF_ASSIGN_OR_RETURN(changed,
                      PrepareModuleForUnrolling(module, execution_threads));

  // Processing the while loops in the reverse of topological order. If the body
  // of while loop A calls while loop B, B comes before A.
  std::vector<HloInstruction*> all_while_ops;
  for (auto* comp : module->MakeComputationPostOrder(execution_threads)) {
    absl::c_copy_if(comp->instructions(), std::back_inserter(all_while_ops),
                    HloPredicateIsOp<HloOpcode::kWhile>);
  }

  // Gather a preliminary vector of all the while ops that we think we can
  // unroll. We do this ahead of time so we don't have to worry about mutating
  // the lists of computations or instructions while we iterate.
  absl::flat_hash_map<HloInstruction*, WhileLoopConfig> unrollable_while_ops =
      GetUnrollableLoops(module, execution_threads);

  VLOG(3) << "Number of while instructions in the module to unroll: "
          << unrollable_while_ops.size();

  for (auto& [while_op, config] : unrollable_while_ops) {
    TF_ASSIGN_OR_RETURN(bool unrolled,
                        UnrollInternal(while_op, unroll_factor_, config));
    changed |= unrolled;
  }

  // We need to inline the calls created for unrolling since later passes rely
  // on the calls to be inlined.
  if (changed) {
    TF_RETURN_IF_ERROR(CallInliner().Run(module, execution_threads).status());
  }

  XLA_VLOG_LINES(3, "WhileLoopUnroller::Run(), after:\n" + module->ToString());
  return changed;
}

}  // namespace xla
