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

#ifndef XLA_SERVICE_GPU_MODEL_TILE_ANALYSIS_H_
#define XLA_SERVICE_GPU_MODEL_TILE_ANALYSIS_H_

#include <cstddef>
#include <cstdint>
#include <ostream>
#include <string>
#include <vector>

#include "absl/container/flat_hash_map.h"
#include "absl/container/flat_hash_set.h"
#include "absl/log/check.h"
#include "absl/types/span.h"
#include "llvm/ADT/Hashing.h"
#include "mlir/IR/AffineMap.h"  // from @llvm-project
#include "mlir/IR/MLIRContext.h"  // from @llvm-project
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/statusor.h"

namespace xla {
namespace gpu {

// Contains an affine map with N dimension expressions and M symbols:
//   (d0, ..., d_{N - 1})[s_0, ..., s_{M - 1}] -> f(d_i, s_j)
// Dimensions d_i correspond to the iteration space of the output tensor. Some
// or all of the dimensions of the input operands can be expressed as a function
// of dimensions of output. For example, for broadcasts and cwise ops all
// dimensions of the inputs are covered by the output dimensions.
// Symbols s_j correspond to the dimensions that are present ONLY in inputs.
// `input_dims_sizes` is an array that holds the upper bounds for the iteration
// sizes for every input-only dimension. Note, that the sizes have upper
// bounds only and the lower bounds are always 0, since we can encode the
// offsets in the affine map. The sizes for the output dimensions can be deduced
// from the shape of the output tensor.
//
// Example:
//
// 1. Indexing map for the input of the following reduction
// ```
//   p0 = f32[150, 20, 10, 50] parameter(0)
//   reduce = f32[150, 10] reduce(p0, p0_init), dimensions={3, 1}
// ```
// can be written as `(d0, d1)[s0, s1] -> (d0, s0, d1, s1)`  with the input
// dimensions sizes `[/*s0 size=*/20, /*s1 size=*/50]`.
//
// 2. Indexing map for the input of the reverse op
// ```
//  %p0 = f32[1, 17, 9, 9] parameter(0)
//  reverse = f32[1, 17, 9, 9] reverse(%p0), dimensions={1, 2}
// ```
// can be written as `(d0, d1, d2, d3) -> (d0, -d1 + 17, -d2 + 9, d3)` with the
// empty 'input_dims_sizes`, because there are no dimensions in the input that
// could not be expressed via dimensions of the output.
struct IndexingMap {
  std::string ToString() const;

  // Returns true if the map was simplified.
  bool Simplify(absl::Span<const int64_t> dimension_sizes);

  mlir::AffineMap affine_map;
  // Upper iteration bounds for dimensions only present in the input.
  std::vector<int64_t> input_dims_sizes;
};
std::ostream& operator<<(std::ostream& out, const IndexingMap& indexing_map);
bool operator==(const IndexingMap& lhs, const IndexingMap& rhs);

template <typename H>
H AbslHashValue(H h, const IndexingMap& indexing_map) {
  llvm::hash_code affine_map_hash = llvm::hash_combine(indexing_map.affine_map);
  return H::combine(std::move(h), static_cast<size_t>(affine_map_hash));
}

// Contains indexing maps for all N-dimensional tensor input operands that
// correspond to a particular output.
struct HloInstructionIndexing {
  std::string ToString() const;

  // Returns true if the indexing was simplified.
  bool Simplify(absl::Span<const int64_t> dimension_sizes);

  // Creates a HloInstructionIndexing from a list of indexing maps for all
  // operands and sorted w.r.t. operand index, i.e. indexing_maps[i] corresponds
  // to operand[i] of the instruction.
  static HloInstructionIndexing FromIndexingMaps(
      absl::Span<const IndexingMap> indexing_maps);

  // Maps input operand index to the indexing map for one particular output.
  absl::flat_hash_map<int64_t, absl::flat_hash_set<IndexingMap>> indexing_maps;
};
std::ostream& operator<<(std::ostream& out,
                         const HloInstructionIndexing& instr_indexing);

std::string ToString(const mlir::AffineMap& affine_map);

// Computes indexing maps for all input operands necessary to compute an element
// of the `output_id` instruction output.
StatusOr<HloInstructionIndexing> ComputeOutputToInputIndexing(
    const HloInstruction* instr, int output_id,
    mlir::MLIRContext* mlir_context);

// Computes indexing maps for all output operands that the element of the
// `input_id` instruction input will participate in.
StatusOr<HloInstructionIndexing> ComputeInputToOutputIndexing(
    const HloInstruction* instr, int input_id, mlir::MLIRContext* mlir_context);

// Groups indexing maps by instructions.
absl::flat_hash_map<const HloInstruction*, absl::flat_hash_set<IndexingMap>>
GroupIndexingMapsByProducers(const HloInstructionIndexing& indexing,
                             const HloInstruction* instr);

// Computes producer indexing maps and fuse/compose them with the consumer
// indexing maps.
Status FuseProducerConsumerOutputToInputIndexing(
    const HloInstruction* producer_instr,
    absl::flat_hash_map<const HloInstruction*,
                        absl::flat_hash_set<IndexingMap>>* consumer_indexing,
    mlir::MLIRContext* mlir_context);

}  // namespace gpu
}  // namespace xla

#endif  // XLA_SERVICE_GPU_MODEL_TILE_ANALYSIS_H_
