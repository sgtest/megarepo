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
#ifndef XLA_SERVICE_GPU_HLO_TRAVERSAL_H_
#define XLA_SERVICE_GPU_HLO_TRAVERSAL_H_

#include <functional>

#include "absl/container/inlined_vector.h"
#include "absl/types/span.h"
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/hlo/ir/hlo_opcode.h"

namespace xla {
namespace gpu {

// Treats HloInstructions as if they were unfused.
class HloInstructionAdaptor {
 public:
  HloInstructionAdaptor() = default;
  explicit HloInstructionAdaptor(const HloInstruction& instruction)
      : instruction_(&instruction) {}

  HloOpcode opcode() const { return instruction_->opcode(); }
  absl::string_view name() const { return instruction_->name(); }

  HloInstructionAdaptor GetOperand(int index) const;
  absl::InlinedVector<HloInstructionAdaptor, 2> GetOperands() const;
  absl::InlinedVector<HloInstructionAdaptor, 2> GetUsers() const;
  const xla::Shape& shape() const { return instruction_->shape(); }
  std::string ToString() const { return instruction_->ToString(); }

  friend bool operator==(const HloInstructionAdaptor& lhs,
                         const HloInstructionAdaptor& rhs);
  template <typename H>
  friend H AbslHashValue(H h, const HloInstructionAdaptor& m);

  // Use sparingly; prefer extending the interface.
  const HloInstruction& instruction() const { return *instruction_; }

 private:
  const HloInstruction* instruction_;
};

template <typename H>
H AbslHashValue(H h, const HloInstructionAdaptor& m) {
  return H::combine(std::move(h), m.instruction_->GetModule(),
                    m.instruction_->unique_id());
}

class HloFusionAdaptor {
 public:
  virtual ~HloFusionAdaptor() = default;
  virtual bool ContainsInstruction(HloInstructionAdaptor instruction) const = 0;
  virtual absl::InlinedVector<HloInstructionAdaptor, 2> GetRoots() const = 0;

  static std::unique_ptr<HloFusionAdaptor> ForInstruction(
      const HloInstruction* instruction);
  static std::unique_ptr<HloFusionAdaptor> ForComputation(
      const HloComputation* computation);
};

class ProducerConsumerFusion : public HloFusionAdaptor {
 public:
  ProducerConsumerFusion(std::unique_ptr<HloFusionAdaptor> producer,
                         std::unique_ptr<HloFusionAdaptor> consumer)
      : producer_(std::move(producer)), consumer_(std::move(consumer)) {}

  bool ContainsInstruction(HloInstructionAdaptor instruction) const override {
    return producer_->ContainsInstruction(instruction) ||
           consumer_->ContainsInstruction(instruction);
  }

  absl::InlinedVector<HloInstructionAdaptor, 2> GetRoots() const override {
    return consumer_->GetRoots();
  }

 private:
  std::unique_ptr<HloFusionAdaptor> producer_;
  std::unique_ptr<HloFusionAdaptor> consumer_;
};

enum class TraversalResult {
  // Visit the operands of this node.
  kVisitOperands,
  // Do not visit any more nodes.
  kAbortTraversal,
  // Do not visit the operands of this node (but continue the traversal
  // otherwise). If the node visitation function returns this, the `boundary`
  // condition will not be evaluated.
  kDoNotVisitOperands,
};

// Visit the HLO nodes starting from `roots` in BFS order (consumers before
// producers). Each node will be visited exactly once.
void HloBfsConsumersFirstTraversal(
    absl::Span<const HloInstructionAdaptor> roots,
    const HloFusionAdaptor& fusion,
    const std::function<TraversalResult(HloInstructionAdaptor node)>&
        visit_node,
    const std::function<void(HloInstructionAdaptor producer)>& visit_arg =
        [](HloInstructionAdaptor) {});

// Visit the HLO nodes starting from `roots`, returning true if the return value
// of `visit` for any of nodes is true. Uses the same order as
// `HloBfsConsumersFirstTraversal`.
bool HloAnyOf(absl::Span<const HloInstructionAdaptor> roots,
              const HloFusionAdaptor& fusion,
              const std::function<bool(HloInstructionAdaptor node)>& visit);

// Visit the HLO nodes stating from `roots`, returning the first
// node for which `visit` returns true, or `nullptr` if no node matches. Uses
// the same order as `HloBfsConsumersFirstTraversal`.
std::optional<HloInstructionAdaptor> HloFindIf(
    absl::Span<const HloInstructionAdaptor> roots,
    const HloFusionAdaptor& fusion,
    const std::function<bool(HloInstructionAdaptor node)>& visit);

// Visit the producers of all parameters that are needed by the fusion.
void FindFusionArguments(
    const HloFusionAdaptor& fusion,
    const std::function<void(HloInstructionAdaptor producer)>& visit);

}  // namespace gpu
}  // namespace xla

#endif  // XLA_SERVICE_GPU_HLO_TRAVERSAL_H_
