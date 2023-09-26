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

#ifndef XLA_SERVICE_HLO_VALUE_SEMANTICS_ANALYSIS_H_
#define XLA_SERVICE_HLO_VALUE_SEMANTICS_ANALYSIS_H_

#include <cstdint>
#include <memory>
#include <optional>
#include <string>
#include <vector>

#include "absl/container/flat_hash_map.h"
#include "absl/container/flat_hash_set.h"
#include "absl/types/span.h"
#include "xla/hlo/ir/dfs_hlo_visitor.h"
#include "xla/hlo/ir/dfs_hlo_visitor_with_default.h"
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/hlo/ir/hlo_module.h"
#include "xla/service/hlo_value.h"
#include "xla/shape_tree.h"
#include "xla/shape_util.h"
#include "xla/status.h"
#include "xla/statusor.h"

namespace xla {

class HloPreOrderDFS {
 public:
  HloPreOrderDFS() = default;
  ~HloPreOrderDFS() = default;
  Status Run(const HloComputation& computation,
             DfsHloVisitorBase<HloInstruction*>* visitor);

 private:
  bool IsReady(const HloInstruction* instruction) const;
  std::vector<HloInstruction*> stack_;
  absl::flat_hash_set<HloInstruction*> visited_;
};

using EinsumDepthMap =
    absl::flat_hash_map<const HloInstruction*, ShapeTree<int>>;

// The einsum depth is the length of the einsum dependency chain. And we
// distinguish instructions that are used by root and that are not used by
// root.
// The einsum depth of an HLO value A is defined as follows:
// for B = op(A, ...)
// 1) the root instruction has a depth of 0;
// 2) non-root instructions that have zero users have a depth of -1;
// 3) if op is a Dot or Convolution (i.e., einsum),
//    depth(A, B) = depth(B) >= 0 ? depth(B) + 1 : depth(B) - 1.
//    depth(A, B) means the depth of A because of B;
// 4) otherwise depth(A, B) = depth(B);
// 5) depth(A) is computed by merging all depth(A, u) where u is a user of A.
//    See MergeDepth for how user depths are merged.

class EinsumDepthAnalysis : public DfsHloVisitorWithDefault {
 public:
  static StatusOr<std::unique_ptr<EinsumDepthAnalysis>> Run(
      const HloComputation& computation);
  ~EinsumDepthAnalysis() override = default;
  Status DefaultAction(HloInstruction* instruction) override;
  Status HandleTuple(HloInstruction* tuple) override;
  Status HandleGetTupleElement(HloInstruction* get_tuple_element) override;
  Status HandleDot(HloInstruction* dot) override;
  Status HandleConvolution(HloInstruction* convolution) override;
  Status HandleCall(HloInstruction* call) override;
  Status HandleFusion(HloInstruction* fusion) override;
  Status HandleCustomCall(HloInstruction* custom_call) override;
  Status HandleWhile(HloInstruction* xla_while) override;
  Status HandleConditional(HloInstruction* conditional) override;
  Status HandleAfterAll(HloInstruction* after_all) override;
  Status HandleOutfeed(HloInstruction* outfeed) override;
  const EinsumDepthMap& GetEinsumDepthMap() const { return einsum_depth_map_; }

 private:
  EinsumDepthAnalysis() = default;
  Status RunInternal(const HloComputation& computation,
                     const std::optional<ShapeTree<int>>& root_depth);
  EinsumDepthMap::iterator GetOrCreateDepthTree(HloInstruction* instruction);
  Status SetInstructionDepth(HloInstruction* instruction, int depth);
  Status SetInstructionDepth(HloInstruction* instruction,
                             const ShapeTree<int>& depth);
  Status HandleDepthIncrementInstruction(HloInstruction* instruction);
  Status HandleCalledComputation(const HloComputation& called_computation,
                                 const ShapeTree<int>& root_depth,
                                 absl::Span<HloInstruction* const> operands);
  EinsumDepthMap einsum_depth_map_;
};

// The comment below explains where the labels could originate from. Once
// originated,  those labels are then propagated throughout the HLO module.
enum class HloValueSemanticLabel {
  // Values that are known or predictable at compile time, including constants,
  // iota, replica-id, and partition-id.
  kStatic,
  // Values that are not known or can't be predicated at compile time.
  kRandom,
  // HLO module parameters.
  kWeight,
  // Output of weight-weight or weight-activation matmuls.
  kActivation,
  // Output of weight-activation matmuls where the weight is a dependence of
  // that activation. Or output of weight-activation-gradient matmuls.
  kActivationGradient,
  // Output of activation-gradient-activation matmuls.
  kWeightGradient,
  kTupleOrToken,
};

std::string HloValueSemanticLabelToString(HloValueSemanticLabel label);

class HloValueSemantics {
 public:
  using Id = int64_t;
  HloValueSemantics(HloValueSemanticLabel label, const HloPosition& origin);
  HloValueSemantics(Id id, HloValueSemanticLabel label,
                    const HloPosition& origin);
  HloValueSemantics(const HloValueSemantics& other) = default;
  HloValueSemantics(HloValueSemantics&& other) = default;
  HloValueSemantics& operator=(const HloValueSemantics& other) = default;

  Id id() const { return id_; }
  HloValueSemanticLabel label() const { return label_; }
  const HloPosition& origin() const { return origin_; }
  std::string ToString() const;

 private:
  const Id id_;
  const HloValueSemanticLabel label_;
  const HloPosition origin_;
};

using HloValueSemanticsMap =
    absl::flat_hash_map<const HloInstruction*,
                        ShapeTree<const HloValueSemantics*>>;
class HloValueSemanticsPropagation;

class HloValueSemanticsAnalysis {
 public:
  static StatusOr<std::unique_ptr<HloValueSemanticsAnalysis>> Run(
      const HloModule& module);
  virtual ~HloValueSemanticsAnalysis() = default;
  const HloValueSemantics* GetSemantics(const HloInstruction* instruction,
                                        const ShapeIndex& index = {}) const;

  const HloValueSemanticsMap& GetSemanticsMap() const {
    return value_semantics_;
  }

  const EinsumDepthMap& GetEinsumDepthMap() const { return einsum_depth_map_; }

 protected:
  friend class HloValueSemanticsPropagation;
  explicit HloValueSemanticsAnalysis(const HloModule& module);
  Status InitializeEinsumDepth();
  void AnnotateWeights();

  // Infer semantics for all instructions in the computation. Computation
  // parameters are assigned the semantics of the corresponding operand.
  Status RunOnComputation(const HloComputation& computation,
                          absl::Span<const HloInstruction* const> operands);
  // Same as the above RunOnComputation, but computation parameters have
  // already been assigned with semantics.
  virtual Status RunOnComputation(const HloComputation& computation);
  HloValueSemantics::Id NextId();
  const HloValueSemantics* NewHloValueSemantics(HloValueSemanticLabel label,
                                                const HloPosition& origin);
  const ShapeTree<const HloValueSemantics*>& GetInstructionSemantics(
      const HloInstruction* instruction) const;
  void DeepCopyHloValueSemantics(
      ShapeTree<const HloValueSemantics*>& copy_to,
      const ShapeTree<const HloValueSemantics*>& copy_from,
      const ShapeIndex& source_index, const ShapeIndex& destination_index);
  void DeepCopyHloValueSemantics(
      const HloInstruction* target,
      const ShapeTree<const HloValueSemantics*>& copy_from,
      const ShapeIndex& source_index = {});
  void SetHloValueSemantics(
      const HloInstruction* target,
      const ShapeTree<const HloValueSemantics*>& semantics);
  void DeleteHloValueSemantics(
      const ShapeTree<const HloValueSemantics*>& to_delete);
  void DeleteHloValueSemantics(const HloValueSemantics* to_delete);
  const HloModule& module_;
  HloValueSemanticsMap value_semantics_;
  absl::flat_hash_map<HloValueSemantics::Id, std::unique_ptr<HloValueSemantics>>
      value_semantics_map_;
  HloValueSemantics::Id next_id_;
  EinsumDepthMap einsum_depth_map_;
};

class HloValueSemanticsPropagation : public DfsHloVisitorWithDefault {
 public:
  explicit HloValueSemanticsPropagation(HloValueSemanticsAnalysis* analysis);
  Status Run(const HloComputation& computation);
  // Infer the output semantics from all operands of the instruction.
  Status DefaultAction(HloInstruction* instruction) override;
  Status HandleParameter(HloInstruction* parameter) override;
  Status HandleConstant(HloInstruction* constant) override;
  Status HandleIota(HloInstruction* iota) override;
  Status HandlePartitionId(HloInstruction* partition_id) override;
  Status HandleReplicaId(HloInstruction* replica_id) override;
  Status HandleClamp(HloInstruction* clamp) override;
  Status HandleTuple(HloInstruction* tuple) override;
  Status HandleGetTupleElement(HloInstruction* get_tuple_element) override;
  Status HandleCall(HloInstruction* call) override;
  Status HandleFusion(HloInstruction* fusion) override;
  Status HandleCustomCall(HloInstruction* custom_call) override;
  Status HandleWhile(HloInstruction* xla_while) override;
  Status HandleConditional(HloInstruction* conditional) override;
  Status HandleSelect(HloInstruction* select) override;
  Status HandleConcatenate(HloInstruction* concatenate) override;
  Status HandleDynamicSlice(HloInstruction* dynamic_slice) override;
  Status HandleDynamicUpdateSlice(
      HloInstruction* dynamic_update_slice) override;
  Status HandleCopyStart(HloInstruction* copy_start) override;
  Status HandleCopyDone(HloInstruction* copy_done) override;
  Status HandleCollectivePermuteStart(
      HloInstruction* collective_permute_start) override;
  Status HandleCollectivePermuteDone(
      HloInstruction* collective_permute_done) override;
  Status HandleGather(HloInstruction* gather) override;
  Status HandleScatter(HloInstruction* scatter) override;
  Status HandleAfterAll(HloInstruction* after_all) override;
  Status HandleAsyncStart(HloInstruction* async_start) override;
  Status HandleAsyncDone(HloInstruction* async_done) override;
  Status HandleInfeed(HloInstruction* infeed) override;
  Status HandleDomain(HloInstruction* domain) override;

 protected:
  HloValueSemantics CopySemantics(const HloValueSemantics& semantics) const;
  HloValueSemantics CopySemanticsWithNewOrigin(
      const HloValueSemantics& semantics, HloInstruction* new_origin,
      const ShapeIndex& index = {}) const;
  const HloValueSemantics* AddSemantics(const HloValueSemantics& semantics);
  struct EinsumAndOperandIndex {
    HloInstruction* einsum;
    int64_t operand_index;
  };
  // Checks if the origin of `semantics` is an einsum that takes
  // `origin_dependence` as an operand.
  // If `recursive` is set to true, recursively checks all ancestors of the
  // `semantics`' origin (including itself) for the above condition.
  // Returns all such einsums and the operand index corresponding to
  // `origin_dependence`.
  // We use this function to find whether the output of an einsum who has an
  // operand X is used in another einsum who takes X as an operand. This is
  // the pattern for gradient.
  // For example, consider C = einsum(A, B), dC / dB = einsum(A, C).
  std::vector<EinsumAndOperandIndex> FindEinsumsWhereOriginDependsOnOther(
      const HloValueSemantics& semantics, const HloPosition& origin_dependence,
      bool recursive = false) const;
  bool OriginDependsOn(const HloValueSemantics& semantics,
                       const HloPosition& origin_dependence,
                       bool recursive = false) const;
  StatusOr<HloValueSemantics> CreateGradientSemantics(
      HloInstruction* gradient_candidate) const;
  StatusOr<HloValueSemantics> ComputeSemanticsFromStaticAndOther(
      const HloValueSemantics& static_semantics,
      const HloValueSemantics& other_semantics,
      HloInstruction* instruction) const;
  StatusOr<HloValueSemantics> ComputeSemanticsFromRandomAndOther(
      const HloValueSemantics& random_semantics,
      const HloValueSemantics& other_semantics,
      HloInstruction* instruction) const;
  StatusOr<HloValueSemantics> ComputeSemanticsFromWeightAndOther(
      const HloValueSemantics& weight_semantics,
      const HloValueSemantics& other_semantics,
      HloInstruction* instruction) const;
  StatusOr<HloValueSemantics> ComputeSemanticsFromActivationAndOther(
      const HloValueSemantics& activation_semantics,
      const HloValueSemantics& other_semantics,
      HloInstruction* instruction) const;
  StatusOr<HloValueSemantics> ComputeSemanticsFromActivationGradientAndOther(
      const HloValueSemantics& activation_gradient_semantics,
      const HloValueSemantics& other_semantics,
      HloInstruction* instruction) const;
  StatusOr<HloValueSemantics> ComputeSemanticsFromWeightGradientAndOther(
      const HloValueSemantics& weight_gradient_semantics,
      const HloValueSemantics& other_semantics,
      HloInstruction* instruction) const;
  StatusOr<HloValueSemantics> ComputeSemanticsFromOperands(
      HloInstruction* instruction, absl::Span<const int64_t> operand_indices,
      absl::Span<const ShapeIndex> operand_shape_indices = {}) const;
  HloValueSemanticsAnalysis* analysis_;
};

}  // namespace xla

#endif  // XLA_SERVICE_HLO_VALUE_SEMANTICS_ANALYSIS_H_
