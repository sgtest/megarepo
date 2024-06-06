/* Copyright 2024 The OpenXLA Authors.

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

#include "xla/service/cpu/thunk_emitter.h"

#include <string>
#include <utility>
#include <vector>

#include "absl/status/status.h"
#include "absl/strings/str_cat.h"
#include "xla/hlo/ir/hlo_casting_utils.h"
#include "xla/hlo/ir/hlo_computation.h"
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/hlo/ir/hlo_instructions.h"
#include "xla/hlo/ir/hlo_module.h"
#include "xla/hlo/ir/hlo_opcode.h"
#include "xla/hlo/ir/hlo_schedule.h"
#include "xla/service/buffer_assignment.h"
#include "xla/service/cpu/ir_emitter2.h"
#include "xla/service/cpu/runtime/call_thunk.h"
#include "xla/service/cpu/runtime/conditional_thunk.h"
#include "xla/service/cpu/runtime/copy_thunk.h"
#include "xla/service/cpu/runtime/infeed_thunk.h"
#include "xla/service/cpu/runtime/kernel_thunk.h"
#include "xla/service/cpu/runtime/outfeed_thunk.h"
#include "xla/service/cpu/runtime/rng_state_thunk.h"
#include "xla/service/cpu/runtime/thunk.h"
#include "xla/service/cpu/runtime/while_thunk.h"
#include "xla/shape.h"
#include "xla/shape_util.h"
#include "xla/stream_executor/launch_dim.h"
#include "xla/util.h"
#include "tsl/platform/errors.h"
#include "tsl/platform/logging.h"
#include "tsl/platform/statusor.h"

namespace xla::cpu {

ThunkEmitter::ThunkEmitter(IrEmitter2* ir_emitter,
                           const BufferAssignment* buffer_assignment)
    : ir_emitter_(ir_emitter), buffer_assignment_(buffer_assignment) {}

static Thunk::Info ThunkInfo(const HloInstruction* instruction) {
  const HloModule* module = instruction->GetModule();
  return Thunk::Info{std::string(instruction->name()),
                     std::string(module->name()), module->unique_id()};
}

absl::StatusOr<ThunkSequence> ThunkEmitter::EmitEntryComputation(
    const HloModule& module) {
  if (!module.has_schedule()) {
    return absl::InternalError("HLO module must be scheduled to emit thunks");
  }
  return EmitHloComputation(module.entry_computation());
}

absl::StatusOr<BufferAllocation::Slice> ThunkEmitter::GetAllocationSlice(
    const HloInstruction* instruction, const ShapeIndex& index) {
  return buffer_assignment_->GetUniqueSlice(instruction, index);
}

absl::StatusOr<ThunkSequence> ThunkEmitter::EmitHloComputation(
    const HloComputation* computation) {
  ThunkSequence thunks;

  const HloSchedule& schedule = computation->parent()->schedule();
  if (!schedule.is_computation_scheduled(computation)) {
    return absl::InternalError(
        absl::StrCat("Computation ", computation->name(),
                     " must be scheduled to emit thunks"));
  }

  const HloInstructionSequence& sequence = schedule.sequence(computation);
  for (HloInstruction* instr : sequence.instructions()) {
    TF_ASSIGN_OR_RETURN(ThunkSequence instr_thunks, EmitHloInstruction(instr));
    thunks.Append(std::move(instr_thunks));
  }

  return thunks;
}

absl::StatusOr<ThunkSequence> ThunkEmitter::EmitHloInstruction(
    const HloInstruction* instruction) {
  switch (instruction->opcode()) {
    // Instructions that do not have a thunk implementation and instead fully
    // defined by the corresponding buffer assignment.
    case HloOpcode::kBitcast:
    case HloOpcode::kGetTupleElement:
    case HloOpcode::kParameter:
    case HloOpcode::kTuple:
      return ThunkSequence::Empty();

    // No-op operations that are used only to define an execution order for the
    // HLO dataflow graph.
    case HloOpcode::kAfterAll:
      return ThunkSequence::Empty();

    // Call operations are simply converted to a ThunkSequence emitted from the
    // called computation and embedded into the "main" one.
    case HloOpcode::kCall:
      return EmitCallThunk(instruction);

    // Control flow thunks check predicates on the host and launch nested thunk
    // sequences for branches and loops.
    case HloOpcode::kConditional:
      return EmitConditionThunk(instruction);
    case HloOpcode::kWhile:
      return EmitWhileThunk(instruction);

    // Allocations for constants owned by the executable, and resolved at run
    // time according to the buffer assignment (using allocation index). We do
    // not need to emit any thunks for constant instructions.
    case HloOpcode::kConstant:
      return ThunkSequence::Empty();

    // Simple HLO instructions lowered to elemental host kernels (plain loops
    // behind the HostKernel API).
    case HloOpcode::kAbs:
    case HloOpcode::kAdd:
    case HloOpcode::kAnd:
    case HloOpcode::kAtan2:
    case HloOpcode::kBroadcast:
    case HloOpcode::kBitcastConvert:
    case HloOpcode::kCbrt:
    case HloOpcode::kCeil:
    case HloOpcode::kClamp:
    case HloOpcode::kClz:
    case HloOpcode::kCompare:
    case HloOpcode::kConvert:
    case HloOpcode::kCos:
    case HloOpcode::kDivide:
    case HloOpcode::kErf:
    case HloOpcode::kExp:
    case HloOpcode::kExpm1:
    case HloOpcode::kFloor:
    case HloOpcode::kImag:
    case HloOpcode::kIota:
    case HloOpcode::kIsFinite:
    case HloOpcode::kLog1p:
    case HloOpcode::kLog:
    case HloOpcode::kMap:
    case HloOpcode::kMaximum:
    case HloOpcode::kMinimum:
    case HloOpcode::kMultiply:
    case HloOpcode::kNegate:
    case HloOpcode::kNot:
    case HloOpcode::kOr:
    case HloOpcode::kPopulationCount:
    case HloOpcode::kPower:
    case HloOpcode::kReal:
    case HloOpcode::kRemainder:
    case HloOpcode::kReverse:
    case HloOpcode::kRoundNearestAfz:
    case HloOpcode::kRoundNearestEven:
    case HloOpcode::kRsqrt:
    case HloOpcode::kSelect:
    case HloOpcode::kShiftLeft:
    case HloOpcode::kShiftRightArithmetic:
    case HloOpcode::kShiftRightLogical:
    case HloOpcode::kSign:
    case HloOpcode::kSin:
    case HloOpcode::kSqrt:
    case HloOpcode::kSubtract:
    case HloOpcode::kTan:
    case HloOpcode::kTanh:
    case HloOpcode::kXor:
      return EmitElementalKernelThunk(instruction);

    // TODO(ezhulenev): Implement slice operations as separate Thunks because
    // it's much easier to get peak performance from hand written code.
    case HloOpcode::kSlice:
    case HloOpcode::kDynamicSlice:
    // TODO(ezhulenev): Port dynamic update slice optimizations from IrEmitter.
    case HloOpcode::kDynamicUpdateSlice:
      return EmitElementalKernelThunk(instruction);

    case HloOpcode::kConcatenate:
      return EmitConcatenateThunk(instruction);

    case HloOpcode::kFusion:
      return EmitFusionKernelThunk(instruction);

    case HloOpcode::kReduce:
    case HloOpcode::kReduceWindow:
      return EmitReductionKernelThunk(instruction);

    case HloOpcode::kRngGetAndUpdateState:
      return EmitRngGetAndUpdateStateThunk(instruction);

    case HloOpcode::kInfeed:
      return EmitInfeedThunk(instruction);

    case HloOpcode::kOutfeed:
      return EmitOutfeedThunk(instruction);

    case HloOpcode::kCopy:
      return EmitCopyThunk(instruction);

    default:
      return absl::UnimplementedError(
          absl::StrCat("HLO opcode `", HloOpcodeString(instruction->opcode()),
                       "` is not supported by XLA:CPU ThunkEmitter"));
  }
}

absl::StatusOr<ThunkSequence> ThunkEmitter::EmitCallThunk(
    const HloInstruction* instruction) {
  TF_ASSIGN_OR_RETURN(
      ThunkSequence called_sequence,
      EmitHloComputation(instruction->called_computations().front()));
  return ThunkSequence::Of<CallThunk>(ThunkInfo(instruction),
                                      std::move(called_sequence));
}

absl::StatusOr<ThunkSequence> ThunkEmitter::EmitConcatenateThunk(
    const HloInstruction* instruction) {
  // TODO(ezhulenev): Port optimized concat implementation from IrEmitter.
  return EmitElementalKernelThunk(instruction);
}

absl::StatusOr<ThunkSequence> ThunkEmitter::EmitCopyThunk(
    const HloInstruction* instruction) {
  const HloInstruction* source = instruction->operand(0);
  TF_ASSIGN_OR_RETURN(auto source_buffer, GetAllocationSlice(source));
  TF_ASSIGN_OR_RETURN(auto destination_buffer, GetAllocationSlice(instruction));
  return ThunkSequence::Of<CopyThunk>(ThunkInfo(instruction), source_buffer,
                                      source->shape(), destination_buffer,
                                      instruction->shape());
}

absl::StatusOr<ThunkSequence> ThunkEmitter::EmitElementalKernelThunk(
    const HloInstruction* instruction) {
  TF_ASSIGN_OR_RETURN(auto kernel,
                      ir_emitter_->EmitElementalHostKernel(instruction));
  TF_ASSIGN_OR_RETURN(auto buffers, GetHostKernelAllocationSlices(instruction));

  return ThunkSequence::Of<KernelThunk>(ThunkInfo(instruction),
                                        buffers.arguments, buffers.results,
                                        kernel.name, kernel.thread_dims);
}

absl::StatusOr<ThunkSequence> ThunkEmitter::EmitFusionKernelThunk(
    const HloInstruction* instruction) {
  auto* fusion = Cast<HloFusionInstruction>(instruction);
  TF_ASSIGN_OR_RETURN(auto kernel, ir_emitter_->EmitFusionHostKernel(fusion));
  TF_ASSIGN_OR_RETURN(auto buffers, GetHostKernelAllocationSlices(instruction));

  return ThunkSequence::Of<KernelThunk>(ThunkInfo(instruction),
                                        buffers.arguments, buffers.results,
                                        kernel.name, kernel.thread_dims);
}

absl::StatusOr<ThunkSequence> ThunkEmitter::EmitReductionKernelThunk(
    const HloInstruction* instruction) {
  TF_ASSIGN_OR_RETURN(auto kernel,
                      ir_emitter_->EmitReductionHostKernel(instruction));
  TF_ASSIGN_OR_RETURN(auto buffers, GetHostKernelAllocationSlices(instruction));

  return ThunkSequence::Of<KernelThunk>(ThunkInfo(instruction),
                                        buffers.arguments, buffers.results,
                                        kernel.name, kernel.thread_dims);
}

absl::StatusOr<ThunkSequence> ThunkEmitter::EmitRngGetAndUpdateStateThunk(
    const HloInstruction* instruction) {
  TF_ASSIGN_OR_RETURN(auto state_buffer, GetAllocationSlice(instruction));
  auto* rng_state = Cast<HloRngGetAndUpdateStateInstruction>(instruction);
  return ThunkSequence::Of<RngGetAndUpdateStateThunk>(
      ThunkInfo(instruction), state_buffer, rng_state->delta());
}

absl::StatusOr<ThunkSequence> ThunkEmitter::EmitInfeedThunk(
    const HloInstruction* instruction) {
  auto* infeed = Cast<HloInfeedInstruction>(instruction);
  const Shape& infeed_shape = infeed->infeed_shape();

  // Collect buffer allocation slices corresponding to data buffers produced by
  // the infeed instruction;
  std::vector<InfeedThunk::InfeedBuffer> infeed_buffers;
  for (auto& infeed_leaf : ShapeUtil::GetLeafShapes(infeed_shape)) {
    infeed_leaf.index.push_front(0);  // prepend infeed tuple index

    TF_ASSIGN_OR_RETURN(BufferAllocation::Slice infeed_slice,
                        GetAllocationSlice(infeed, infeed_leaf.index));

    infeed_buffers.push_back(InfeedThunk::InfeedBuffer{
        infeed_slice,
        infeed_leaf.shape,
    });
  }

  return ThunkSequence::Of<InfeedThunk>(ThunkInfo(instruction), infeed_buffers);
}

absl::StatusOr<ThunkSequence> ThunkEmitter::EmitOutfeedThunk(
    const HloInstruction* instruction) {
  auto* outfeed = Cast<HloOutfeedInstruction>(instruction);
  const Shape& outfeed_shape = outfeed->outfeed_shape();

  // Collect buffer allocation slices corresponding to data buffers fed into the
  // outfeed instruction as first operand.
  std::vector<OutfeedThunk::OutfeedBuffer> outfeed_buffers;
  for (auto& outfeed_leaf : ShapeUtil::GetLeafShapes(outfeed_shape)) {
    TF_ASSIGN_OR_RETURN(
        BufferAllocation::Slice outfeed_slice,
        GetAllocationSlice(outfeed->operand(0), outfeed_leaf.index));

    outfeed_buffers.push_back(OutfeedThunk::OutfeedBuffer{
        outfeed_slice,
        outfeed_leaf.shape,
    });
  }

  return ThunkSequence::Of<OutfeedThunk>(ThunkInfo(instruction),
                                         outfeed_buffers);
}

absl::StatusOr<ThunkSequence> ThunkEmitter::EmitConditionThunk(
    const HloInstruction* instruction) {
  std::vector<ThunkSequence> branches;
  TF_ASSIGN_OR_RETURN(auto branch_index_buffer,
                      GetAllocationSlice(instruction->operand(0)));

  for (HloComputation* branch : instruction->branch_computations()) {
    TF_ASSIGN_OR_RETURN(branches.emplace_back(), EmitHloComputation(branch));
  }

  return ThunkSequence::Of<ConditionalThunk>(
      ThunkInfo(instruction), branch_index_buffer, std::move(branches));
}

absl::StatusOr<ThunkSequence> ThunkEmitter::EmitWhileThunk(
    const HloInstruction* instruction) {
  HloInstruction* cond = instruction->while_condition()->root_instruction();
  TF_ASSIGN_OR_RETURN(auto cond_buffer, GetAllocationSlice(cond));

  TF_ASSIGN_OR_RETURN(ThunkSequence cond_thunk,
                      EmitHloComputation(instruction->while_condition()));
  TF_ASSIGN_OR_RETURN(ThunkSequence body_thunk,
                      EmitHloComputation(instruction->while_body()));

  return ThunkSequence::Of<WhileThunk>(ThunkInfo(instruction), cond_buffer,
                                       std::move(cond_thunk),
                                       std::move(body_thunk));
}

absl::StatusOr<ThunkEmitter::HostKernelAllocationSlices>
ThunkEmitter::GetHostKernelAllocationSlices(const HloInstruction* instruction) {
  HostKernelAllocationSlices slices;

  auto add_buffers = [&](std::vector<BufferAllocation::Slice>& buffers,
                         const HloInstruction* instr) -> absl::Status {
    for (const auto& indexed : ShapeUtil::GetLeafShapes(instr->shape())) {
      TF_ASSIGN_OR_RETURN(buffers.emplace_back(),
                          GetAllocationSlice(instr, indexed.index));
    }
    return absl::OkStatus();
  };

  for (HloInstruction* operand : instruction->operands()) {
    TF_RETURN_IF_ERROR(add_buffers(slices.arguments, operand));
  }

  TF_RETURN_IF_ERROR(add_buffers(slices.results, instruction));

  return slices;
}

}  // namespace xla::cpu
