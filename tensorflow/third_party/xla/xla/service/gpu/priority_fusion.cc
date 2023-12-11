/* Copyright 2017 The TensorFlow Authors. All Rights Reserved.

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

#include "xla/service/gpu/priority_fusion.h"

#include <cstddef>
#include <cstdint>
#include <functional>
#include <iterator>
#include <limits>
#include <map>
#include <memory>
#include <optional>
#include <utility>
#include <vector>

#include "absl/algorithm/container.h"
#include "absl/container/flat_hash_map.h"
#include "absl/container/flat_hash_set.h"
#include "absl/container/inlined_vector.h"
#include "absl/log/check.h"
#include "absl/meta/type_traits.h"
#include "absl/synchronization/mutex.h"
#include "absl/time/time.h"
#include "xla/hlo/ir/hlo_computation.h"
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/hlo/ir/hlo_opcode.h"
#include "xla/service/dump.h"
#include "xla/service/fusion_node_indexing_evaluation.h"
#include "xla/service/fusion_queue.h"
#include "xla/service/gpu/fusion_process_dump.pb.h"
#include "xla/service/gpu/gpu_fusible.h"
#include "xla/service/gpu/hlo_traversal.h"
#include "xla/service/gpu/model/fusion_analysis_cache.h"
#include "xla/service/gpu/model/gpu_hlo_cost_analysis.h"
#include "xla/service/gpu/model/gpu_performance_model.h"
#include "xla/service/instruction_fusion.h"
#include "xla/shape.h"
#include "xla/stream_executor/device_description.h"
#include "xla/xla_data.pb.h"
#include "tsl/platform/blocking_counter.h"
#include "tsl/platform/logging.h"
#include "tsl/platform/status.h"

namespace xla {
namespace gpu {

namespace {
bool ElementIsF32OrF16(const Shape& shape) {
  PrimitiveType type = shape.element_type();
  return type == F32 || type == F16;
}

bool IsFusible(const HloInstruction& instr) {
  // Side-effecting operations are not fusible.
  if (!instr.IsFusible()) {
    return false;
  }

  // Element-wise operations are always fusible.
  if (instr.IsElementwise()) {
    return true;
  }

  // Other non-elementwise ops also supported by elemental fusion.
  switch (instr.opcode()) {
    case HloOpcode::kFusion:
      return instr.fusion_kind() != HloInstruction::FusionKind::kCustom;

    case HloOpcode::kCopy:
    case HloOpcode::kIota:
    case HloOpcode::kConstant:
    case HloOpcode::kReduce:
    case HloOpcode::kBitcast:
    case HloOpcode::kBroadcast:
    case HloOpcode::kConcatenate:
    case HloOpcode::kDynamicSlice:
    case HloOpcode::kDynamicUpdateSlice:
    case HloOpcode::kGather:
    case HloOpcode::kPad:
    case HloOpcode::kReduceWindow:
    case HloOpcode::kReshape:
    case HloOpcode::kReverse:
    case HloOpcode::kScatter:
    case HloOpcode::kSlice:
    case HloOpcode::kTranspose:
      return true;
    default:
      return false;
  }
}

// An implementation of FusionQueue that determines whether to fuse instructions
// according to a cost model, and chooses the next fusion candidate according to
// dynamically updated priorities. The elements in the queue are producer nodes
// that could be fused, and the priority of a producer is the benefit in
// performance when fusing it to all of its fusible users. We greedily pick the
// max-benefit producer to fuse, and update the estimated benefits of the fused
// nodes and their operands.
class GpuPriorityFusionQueue : public FusionQueue {
  using Priority = int64_t;
  using CanFuseCallback = std::function<FusionDecision(
      HloInstruction* /*producer*/, int64_t /*consumer operand_index*/)>;

 public:
  GpuPriorityFusionQueue(
      HloComputation* computation,
      const GpuHloCostAnalysis::Options& cost_analysis_options,
      const se::DeviceDescription* device_info,
      FusionProcessDumpProto* fusion_process_dump,
      tsl::thread::ThreadPool* thread_pool,
      HloFusionAnalysisCache& fusion_analysis_cache)
      : computation_(computation),
        cost_analysis_(cost_analysis_options, device_info),
        fusion_process_dump_(fusion_process_dump),
        thread_pool_(thread_pool),
        fusion_analysis_cache_(fusion_analysis_cache) {
    VLOG(2) << "Running full HLO cost analysis for " << computation_->name();
    TF_CHECK_OK(computation_->Accept(&cost_analysis_));

    // Initializes the priority queue.
    std::vector<HloInstruction*> instructions;
    for (auto* instruction : computation->MakeInstructionPostOrder()) {
      if (instruction->opcode() == HloOpcode::kParameter ||
          instruction->user_count() == 0 || !instruction->IsFusible() ||
          instruction->opcode() == HloOpcode::kTuple ||
          instruction->opcode() == HloOpcode::kGetTupleElement) {
        continue;
      }
      instructions.push_back(instruction);
    }
    std::vector<Priority> priorities = ComputePriorities(instructions);

    for (auto [instruction, priority] : llvm::zip(instructions, priorities)) {
      auto emplace_result = producer_priority_queue_.emplace(
          std::make_pair(priority, instruction->unique_id()), instruction);
      CHECK(emplace_result.second);
      reverse_map_.emplace(instruction, emplace_result.first);
    }
  }

  std::vector<Priority> ComputePriorities(
      const std::vector<HloInstruction*>& instructions) {
    auto schedule_or_run = [this](std::function<void()> fn) {
      if (thread_pool_) {
        thread_pool_->Schedule(std::move(fn));
      } else {
        fn();
      }
    };
    tsl::BlockingCounter counter(instructions.size());
    std::vector<Priority> priorities(instructions.size());

    for (size_t i = 0; i < instructions.size(); ++i) {
      schedule_or_run([&, i] {
        priorities[i] = CalculateProducerPriority(instructions[i]);
        counter.DecrementCount();
      });
    }
    counter.Wait();
    return priorities;
  }

  std::pair<HloInstruction*, std::vector<int64_t>>
  DequeueNextInstructionAndOperandsToFuseInOrder() override {
    while (current_consumers_.empty()) {
      if (producer_priority_queue_.empty()) {
        return {};
      }
      auto next_it = std::prev(producer_priority_queue_.end());
      auto priority = next_it->first.first;

      current_producer_ = next_it->second;
      producer_priority_queue_.erase(next_it);
      reverse_map_.erase(current_producer_);

      // If the priority is negative, it's not helpful to perform fusion on this
      // instruction.
      if (priority < 0) {
        continue;
      }
      current_consumers_ = current_producer_->users();

      if (current_producer_->opcode() == HloOpcode::kBitcast) {
        // We don't check if bitcasts can be fused with all consumers, so we
        // have to do it here.
        llvm::erase_if(current_consumers_, [&](HloInstruction* consumer) {
          return !CanFuseCached(current_producer_, consumer);
        });
      }
    }

    auto next_consumer = current_consumers_.back();
    int64_t producer_operand_index =
        next_consumer->operand_index(current_producer_);
    current_consumers_.pop_back();
    VLOG(5) << "next: " << next_consumer->name() << "(" << next_consumer
            << ") + " << current_producer_->name() << "(" << current_producer_
            << ")";
    return {next_consumer, {producer_operand_index}};
  }

  // Prepares producer and consumer instruction to be fused. Invalidates caches
  // and writes logs.
  void PreFusion(HloInstruction* producer, HloInstruction* consumer) override {
    InvalidateCaches(producer);
    InvalidateCaches(consumer);
  }

  // Invalidates all cached value related to this instruction. Called before the
  // instruction is fused. The instruction can be either producer or consumer.
  void InvalidateCaches(HloInstruction* instruction) {
    HloInstructionAdaptor instruction_adaptor(*instruction);

    can_fuse_cache_.erase(instruction_adaptor);
    for (auto operand : instruction_adaptor.GetOperands()) {
      auto it = can_fuse_cache_.find(operand);
      if (it != can_fuse_cache_.end()) {
        it->second.erase(instruction_adaptor);
      }
    }

    gpu_performance_model_cache_.Invalidate(*instruction);
    fusion_analysis_cache_.Invalidate(*instruction);

    for (auto* user : instruction->users()) {
      fusion_node_evaluations_.erase(user);
    }
    fusion_node_evaluations_.erase(instruction);
  }

  // Updates data for the new fusion instruction and its users and operands.
  void OnFusingInstruction(HloInstruction* fusion,
                           HloInstruction* original_producer,
                           HloInstruction* original_consumer) override {
    if (fusion_process_dump_) {
      auto* fusion_step =
          fusion_process_dump_->add_fusion_steps()->mutable_fusion();

      // Explicit std::string is needed for OSS proto implementation.
      fusion_step->set_fusion_name(std::string(fusion->name()));
      fusion_step->set_producer_name(std::string(original_producer->name()));
      fusion_step->set_consumer_name(std::string(original_consumer->name()));
    }

    // The original consumer was replaced with the fusion, but it's pointer can
    // still be referenced somewhere, for example, in to_update_priority_.
    // Priority recomputation is called before DCE. Remove all references to
    // the original consumer here.
    if (fusion != original_consumer) {
      RemoveInstruction(original_consumer);
    }

    // Detach 'original_producer' from its operands if it has no users.
    // This avoids having it appear as a "phantom" user in subsequent priority
    // calculations on 'fusion.operands' below, before it is finally removed
    // in 'RemoveInstruction'.
    if (original_producer->user_count() == 0) {
      original_producer->DetachFromOperandsAndUsers();
    }

    // Collect the instructions whose priorities need to be updated.
    for (HloInstruction* operand : fusion->operands()) {
      if (operand == original_producer ||
          original_producer->opcode() == HloOpcode::kBroadcast ||
          operand->opcode() == HloOpcode::kBroadcast ||
          operand->opcode() == HloOpcode::kConstant ||
          operand->opcode() == HloOpcode::kGetTupleElement) {
        continue;
      }
      // Need to consider only instructions that are fusible, e.g., rng with
      // greater than one user is not fusible.
      if (!operand->IsFusible()) {
        continue;
      }

      to_update_priority_.insert(operand);
    }
    to_update_priority_.insert(fusion);

    // When current_consumers_ is empty, we will need to dequeue a new producer
    // next time, so we update the priorities now.
    if (current_consumers_.empty()) {
      // Revisit costs of all updated ops. It's important to update cost
      // analysis before recalculating priorities.
      for (auto instruction : to_update_priority_) {
        TF_CHECK_OK(cost_analysis_.RevisitInstruction(instruction));
      }

      std::vector<HloInstruction*> to_update_vector{to_update_priority_.begin(),
                                                    to_update_priority_.end()};
      std::vector<Priority> new_priorities =
          ComputePriorities(to_update_vector);

      for (auto [instruction, new_priority] :
           llvm::zip(to_update_vector, new_priorities)) {
        auto reverse_it = reverse_map_.find(instruction);
        const auto new_key =
            std::make_pair(new_priority, instruction->unique_id());
        if (reverse_it != reverse_map_.end()) {
          if (new_key == reverse_it->second->first) {
            continue;
          }
          producer_priority_queue_.erase(reverse_it->second);
        }
        auto emplace_result =
            producer_priority_queue_.emplace(new_key, instruction);
        CHECK(emplace_result.second);
        if (reverse_it != reverse_map_.end()) {
          reverse_it->second = emplace_result.first;
        } else {
          reverse_map_.emplace(instruction, emplace_result.first);
        }
      }
      to_update_priority_.clear();
    }
  }

  // Removes data for the instruction.
  void RemoveInstruction(HloInstruction* instruction) override {
    to_update_priority_.erase(instruction);
    fusion_analysis_cache_.Invalidate(*instruction);

    auto reverse_it = reverse_map_.find(instruction);
    if (reverse_it == reverse_map_.end()) {
      return;
    }
    producer_priority_queue_.erase(reverse_it->second);
    reverse_map_.erase(reverse_it);
  }

  const std::vector<bool>* FusionConfiguration() override { return nullptr; }

 private:
  // Returns the priority of the producer based on its current operands and
  // users.
  Priority CalculateProducerPriority(HloInstruction* producer) {
    // Bitcasts should always be fused first, since they are no-ops.
    if (producer->opcode() == HloOpcode::kBitcast) {
      return std::numeric_limits<Priority>::max();
    }
    // We always fuse constants, but the cost model doesn't handle them very
    // well: fusing constants changes costs significantly. Also, there's no
    // point recomputing priorities. Therefore, we fuse all of them at the end.
    if (producer->opcode() == HloOpcode::kConstant) {
      return std::numeric_limits<Priority>::min();
    }

    // Don't fuse if we can't fuse in all users.
    if (auto fusion_decision = CanFuseWithAllUsers(producer);
        !fusion_decision) {
      if (fusion_process_dump_) {
        absl::MutexLock lock(&fusion_process_dump_mutex_);
        auto* step = fusion_process_dump_->add_fusion_steps()
                         ->mutable_producer_ineligible();
        step->set_producer_name(std::string(producer->name()));
        step->set_reason(fusion_decision.Explain());
      }
      return std::numeric_limits<Priority>::min();
    }

    GpuPerformanceModel::RunTimes run_times =
        GpuPerformanceModel::EstimateRunTimes(
            producer, &cost_analysis_,
            GpuPerformanceModelOptions::PriorityFusion(
                &fusion_analysis_cache_, &gpu_performance_model_cache_),
            producer->users());
    if (fusion_process_dump_) {
      absl::MutexLock lock(&fusion_process_dump_mutex_);
      auto* step =
          fusion_process_dump_->add_fusion_steps()->mutable_update_priority();
      step->set_producer_name(std::string(producer->name()));
      for (auto* consumer : producer->users()) {
        step->add_consumer_names(std::string(consumer->name()));
      }
      step->set_us_fused(absl::ToDoubleMicroseconds(run_times.time_fused));
      step->set_us_unfused(absl::ToDoubleMicroseconds(run_times.time_unfused));
    }
    return absl::ToInt64Nanoseconds(run_times.time_unfused -
                                    run_times.time_fused);
  }

  FusionDecision CanFuse(HloInstruction* producer, HloInstruction* consumer) {
    if (!IsFusible(*producer)) {
      return "the producer is not fusible";
    }

    if (!IsFusible(*consumer)) {
      return "the consumer is not fusible";
    }

    // Scatter is special as it has no elemental version but is still input
    // fusible. Block attempts to create scatter fusions we can't codegen.
    if (auto can_fuse = CanEmitInputFusedScatter(*producer, *consumer);
        !can_fuse) {
      return can_fuse;
    }

    // Avoid fusing reduce into reduce. Our cost model doesn't currently
    // understand this case due to a lack of tiling analysis.
    // TODO(b/312200883): Remove this.
    auto contains_signficant_reduce = [&](const HloInstruction* instr) {
      auto fusion = HloFusionAdaptor::ForInstruction(instr);
      return HloAnyOf(fusion->GetRoots(), *fusion, [](auto node) {
        if (node.opcode() != HloOpcode::kReduce) return false;

        int64_t reduction_size =
            ShapeUtil::ElementsIn(node.instruction().operand(0)->shape()) /
            ShapeUtil::ElementsIn(node.shape());

        // Small reductions are emitted using the elemental emitter anyway.
        return reduction_size >= 16;
      });
    };
    if (contains_signficant_reduce(producer) &&
        contains_signficant_reduce(consumer)) {
      return "both the producer and the consumer contain a reduce";
    }

    // Avoid doing fusions into the output of an "input" fusion when it would
    // switch it to the loop emitter. This often occurs during epilog fusion for
    // reductions, which suffer from limited emitter support.
    // TODO(b/312686229): Cost model should handle this.
    const auto& analysis_fused =
        fusion_analysis_cache_.Get(*producer, *consumer);
    if (producer->IsInputFusion() && analysis_fused &&
        analysis_fused->GetEmitterFusionKind() ==
            HloFusionAnalysis::EmitterFusionKind::kLoop) {
      const auto& analysis = fusion_analysis_cache_.Get(*producer);
      if (!analysis || analysis->GetEmitterFusionKind() ==
                           HloFusionAnalysis::EmitterFusionKind::kReduction) {
        return "fusion into output of a reduce fusion would create a loop "
               "fusion";
      }
    }

    // Avoid cases where we'd create a fusion that hit limitations in ptxas.
    // Would be nice to model this with cost instead.
    if (auto fits_budget = FusionFitsInBudget(
            *consumer, *producer, *cost_analysis_.device_info_,
            /*is_consumer_producer_fusion=*/true);
        !fits_budget) {
      return fits_budget;
    }

    // Also check that our emitter can handle the fusion node. We currently can
    // have exponential time/memory requirements for emitting certain fusion
    // kernels, in which case we don't want to fuse.
    // TODO(b/119692968): Remove this once we have fixed our fusion emitter.
    if (consumer->opcode() == HloOpcode::kFusion) {
      absl::MutexLock lock(&fusion_node_evaluations_mutex_);
      if (fusion_node_evaluations_.find(consumer) ==
          fusion_node_evaluations_.end()) {
        // We have no cached results for this fusion node yet. Compute it now.
        fusion_node_evaluations_.emplace(
            consumer, FusionNodeIndexingEvaluation(consumer));
      }
      if (fusion_node_evaluations_.at(consumer).CodeDuplicationTooHigh(
              producer)) {
        return "the fusion would result in an overly large code duplication";
      }
    }

    return InstructionFusion::ShouldFuseInPlaceOp(producer, consumer);
  }

  FusionDecision CanFuseCached(HloInstruction* producer,
                               HloInstruction* consumer) {
    HloInstructionAdaptor producer_adaptor(*producer);
    HloInstructionAdaptor consumer_adaptor(*consumer);

    {
      absl::MutexLock lock(&can_fuse_cache_mutex_);
      auto& producer_cache = can_fuse_cache_[producer_adaptor];

      auto it = producer_cache.find(consumer_adaptor);
      if (it != producer_cache.end()) {
        return it->second;
      }
    }

    auto fusion_decision = CanFuse(producer, consumer);

    // The lock is required, because writing to a flat_hash_map is not
    // thread-safe even for different keys. We never call this computation
    // concurrently for the same producer, so it's guaranteed that we don't
    // override any value.
    {
      absl::MutexLock lock(&can_fuse_cache_mutex_);
      can_fuse_cache_[producer_adaptor][consumer_adaptor] = fusion_decision;
    }

    return fusion_decision;
  }

  FusionDecision CanFuseWithAllUsers(HloInstruction* producer) {
    if (producer->users().size() == 0) {
      return "No users to fuse";
    }

    FusionDecision result;
    for (const auto& user : producer->users()) {
      if (auto fusion_decision = CanFuseCached(producer, user);
          !fusion_decision) {
        VLOG(10) << "Cannot fuse " << producer->name() << " with "
                 << user->name() << ", because: " << fusion_decision.Explain();
        return fusion_decision;
      }
    }
    return {};
  }

  // Store computation for cost analysis.
  HloComputation* computation_;

  // Reference to cost model that defines priorities in the queue.
  GpuHloCostAnalysis cost_analysis_;

  // The priority queue of producers, implemented as an ordered map, where a
  // key is a pair: the first element is the priority and the second element is
  // the unique ID of the instruction to break ties.
  using PriorityQueue = std::map<std::pair<Priority, int>, HloInstruction*>;
  PriorityQueue producer_priority_queue_;

  // A reverse map that helps find an instruction in the priority queue.
  absl::flat_hash_map<HloInstruction*, PriorityQueue::iterator> reverse_map_;

  // The current producer being visited.
  HloInstruction* current_producer_;

  // The current consumers being visited.
  std::vector<HloInstruction*> current_consumers_;

  // The set of producers whose priorities need to be updated. Their
  // priorities are changed because their neighbors got fused, but we delay
  // the priority updates until current_consumers_ becomes empty. This is to
  // avoid recomputing priorities multiple times before we dequeue a new
  // producer.
  absl::flat_hash_set<HloInstruction*> to_update_priority_;

  // Proto with structured logs of fusion decisions. Used only for debugging. If
  // null, logging is disabled.
  FusionProcessDumpProto* fusion_process_dump_;
  absl::Mutex fusion_process_dump_mutex_;

  tsl::thread::ThreadPool* thread_pool_;

  HloFusionAnalysisCache& fusion_analysis_cache_;

  // Caches result of can_fuse for a (producer, consumer) pair. A cache entry is
  // invalidated if producer or consumer is modified.
  absl::flat_hash_map<
      HloInstructionAdaptor,
      absl::flat_hash_map<HloInstructionAdaptor, FusionDecision>>
      can_fuse_cache_;
  absl::Mutex can_fuse_cache_mutex_;

  GpuPerformanceModelCache gpu_performance_model_cache_;

  // Keep track of the number of times each instruction inside a fusion node is
  // indexed with different index vectors.
  absl::Mutex fusion_node_evaluations_mutex_;
  absl::flat_hash_map<const HloInstruction*, FusionNodeIndexingEvaluation>
      fusion_node_evaluations_;
};

}  // namespace

/*static*/ bool GpuPriorityFusion::IsExpensive(
    const HloInstruction& instruction) {
  // Some floating-point math ops are cheap on the GPU.
  switch (instruction.opcode()) {
    case HloOpcode::kDivide:
    case HloOpcode::kSqrt:
    case HloOpcode::kRsqrt:
    case HloOpcode::kExp:
      if (ElementIsF32OrF16(instruction.shape())) {
        return false;
      }
      break;
    // Loop fusions are cheap.
    case HloOpcode::kFusion:
      return false;
    default:
      break;
  }
  return InstructionFusion::IsExpensive(instruction);
}

StatusOr<bool> GpuPriorityFusion::Run(
    HloModule* module,
    const absl::flat_hash_set<absl::string_view>& execution_threads) {
  bool dump_enabled = DumpingEnabledForHloModule(*module);
  if (dump_enabled) {
    fusion_process_dump_ = std::make_unique<FusionProcessDumpProto>();
  }

  // Appends ".0" suffix to all instructions.
  //
  // Every time an instruction is duplicated, the last integer suffix is
  // incremented.
  // Before: broadcast.123 -> broadcast.124
  // After: broadcast.123.0 -> broadcast.123.1
  //
  // With this modification it will be easier to match intructions before and
  // after fusion passes, because they will have the same unique prefix. Names
  // are not used in the pipeline, but it makes debugging much easier.
  for (auto* computation : GetFusionComputations(module, execution_threads)) {
    for (auto* instruction : computation->instructions()) {
      instruction->SetAndSanitizeName(absl::StrCat(instruction->name(), ".0"));
    }
  }

  auto result = InstructionFusion::Run(module, execution_threads);

  // Fuse all constants.
  if (result.ok()) {
    // Note: `GetFusionComputations` doesn't return the fusion computations, but
    // the computations to be fused.
    for (auto* computation : GetFusionComputations(module, execution_threads)) {
      std::vector<HloInstruction*> constants;
      for (auto* instruction : computation->instructions()) {
        if (instruction->opcode() == HloOpcode::kConstant) {
          constants.push_back(instruction);
        }
      }
      for (auto* constant : constants) {
        auto users = constant->users();
        for (auto* user : users) {
          if (IsFusible(*user)) {
            result.value() = true;
            InstructionFusion::Fuse(constant, user, computation);
          }
        }
      }
    }
  }

  // FusionAnalysis cache uses unique_id as key. IDs are only unique inside one
  // module. It's important to fully clear the cache if the same instance of the
  // pass will be called on a different module.
  fusion_analysis_cache_.Clear();

  if (dump_enabled) {
    DumpPerModuleProtobufToFile(*module, *fusion_process_dump_,
                                module->config().debug_options(),
                                "priority_fusion_dump");
  }

  return result;
}

FusionDecision GpuPriorityFusion::ShouldFuse(HloInstruction* consumer,
                                             int64_t operand_index) {
  // This method is called in `InstructionFusion::Run` right before fusion, but
  // it will always return true. Fusion decision are fully controlled by the
  // PriorityQueue. If the queue returns a producer that shouldn't be fused,
  // it's a bug and should be fixed in the queue logic.
  return {};
}

HloInstruction::FusionKind GpuPriorityFusion::ChooseKind(
    const HloInstruction* producer, const HloInstruction* consumer) {
  // Derive kInput/kLoop fusion kinds from fusion analysis. This shouldn't
  // matter but some passes downstream still query these instead of fusion
  // analysis.
  // TODO: Don't recompute this all the time.
  const auto& analysis = fusion_analysis_cache_.Get(*producer, *consumer);
  if (!analysis) return HloInstruction::FusionKind::kLoop;
  switch (analysis->GetEmitterFusionKind()) {
    case HloFusionAnalysis::EmitterFusionKind::kLoop:
      return HloInstruction::FusionKind::kLoop;
    case HloFusionAnalysis::EmitterFusionKind::kTriton:
    case HloFusionAnalysis::EmitterFusionKind::kCustomFusion:
      return HloInstruction::FusionKind::kCustom;
    case HloFusionAnalysis::EmitterFusionKind::kReduction:
    case HloFusionAnalysis::EmitterFusionKind::kTranspose:
    case HloFusionAnalysis::EmitterFusionKind::kInputSlices:
    case HloFusionAnalysis::EmitterFusionKind::kScatter:
      return HloInstruction::FusionKind::kInput;
  }
}

HloInstruction* GpuPriorityFusion::FuseInstruction(
    HloInstruction* fusion_instruction, HloInstruction* producer) {
  HloInstruction* result = fusion_instruction;
  if (producer->opcode() == HloOpcode::kFusion) {
    fusion_instruction->MergeFusionInstruction(producer);
  } else {
    result = InstructionFusion::FuseInstruction(fusion_instruction, producer);
  }
  return result;
}

std::unique_ptr<FusionQueue> GpuPriorityFusion::GetFusionQueue(
    HloComputation* computation) {
  return std::unique_ptr<FusionQueue>(new GpuPriorityFusionQueue(
      computation, cost_analysis_options_, &device_info_,
      fusion_process_dump_.get(), thread_pool_, fusion_analysis_cache_));
}

}  // namespace gpu
}  // namespace xla
