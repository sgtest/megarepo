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
#ifndef TENSORFLOW_COMPILER_XLA_SERVICE_GPU_HLO_TRAVERSAL_H_
#define TENSORFLOW_COMPILER_XLA_SERVICE_GPU_HLO_TRAVERSAL_H_

#include <functional>

#include "tensorflow/compiler/xla/hlo/ir/hlo_instruction.h"

namespace xla {
namespace gpu {

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

// Visit the HLO nodes starting from `root` in BFS order (consumers before
// producers). Each node will be visited exactly once. The graph is not
// traversed along edges for which `boundary` returns true.
void HloBfsConsumersFirstTraversal(
    const HloInstruction& root,
    const std::function<bool(const HloInstruction& producer,
                             const HloInstruction& consumer)>& boundary,
    const std::function<TraversalResult(const HloInstruction& node)>& visit);

}  // namespace gpu
}  // namespace xla

#endif  // TENSORFLOW_COMPILER_XLA_SERVICE_GPU_HLO_TRAVERSAL_H_
