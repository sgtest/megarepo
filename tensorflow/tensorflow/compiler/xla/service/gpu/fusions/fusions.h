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
#ifndef TENSORFLOW_COMPILER_XLA_SERVICE_GPU_FUSIONS_FUSIONS_H_
#define TENSORFLOW_COMPILER_XLA_SERVICE_GPU_FUSIONS_FUSIONS_H_

#include <memory>
#include <optional>

#include "tensorflow/compiler/xla/hlo/ir/hlo_instructions.h"
#include "tensorflow/compiler/xla/mlir_hlo/lhlo/IR/lhlo_ops.h"
#include "tensorflow/compiler/xla/service/elemental_ir_emitter.h"
#include "tensorflow/compiler/xla/service/gpu/fusions/fusion_emitter.h"
#include "tensorflow/compiler/xla/service/gpu/hlo_fusion_analysis.h"
#include "tensorflow/compiler/xla/service/gpu/ir_emitter_context.h"

namespace xla {
namespace gpu {

// Returns the emitter for the given fusion. Returns nullopt if the fusion
// type is not yet supported.
std::optional<std::unique_ptr<FusionInterface>> GetFusionEmitter(
    HloFusionAnalysis& analysis, IrEmitterContext& ir_emitter_context,
    ElementalIrEmitter& elemental_emitter, mlir::lmhlo::FusionOp fusion_op,
    const HloFusionInstruction& fusion);

}  // namespace gpu
}  // namespace xla

#endif  // TENSORFLOW_COMPILER_XLA_SERVICE_GPU_FUSIONS_FUSIONS_H_
