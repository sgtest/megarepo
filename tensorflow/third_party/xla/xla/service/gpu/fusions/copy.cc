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
#include "xla/service/gpu/fusions/copy.h"

#include <memory>

#include "llvm/ADT/STLExtras.h"
#include "llvm/IR/IRBuilder.h"
#include "xla/hlo/ir/hlo_instructions.h"
#include "xla/mlir_hlo/lhlo/IR/lhlo_ops.h"
#include "xla/service/elemental_ir_emitter.h"
#include "xla/service/gpu/copy_thunk.h"
#include "xla/service/gpu/fusions/fusion_emitter.h"
#include "xla/service/gpu/ir_emission_utils.h"
#include "xla/service/gpu/ir_emitter_context.h"
#include "xla/service/gpu/kernel_reuse_cache.h"
#include "xla/service/gpu/thunk.h"
#include "xla/shape_util.h"
#include "xla/statusor.h"

namespace xla {
namespace gpu {

StatusOr<FusionEmissionResult> MemcpyFusion::Emit(
    IrEmitterContext& ir_emitter_context, ElementalIrEmitter&,
    mlir::lmhlo::FusionOp fusion_op, const HloFusionInstruction& fusion,
    KernelReuseCache&, llvm::IRBuilder<>*) const {
  FusionEmissionResult result;
  for (int i = 0; i < src_buffers_.size(); ++i) {
    if (src_buffers_[i] != dst_buffers_[i]) {
      result.thunks.emplace_back(std::make_unique<DeviceToDeviceCopyThunk>(
          ir_emitter_context.emit_ir_from_hlo()
              ? Thunk::ThunkInfo::WithProfileAnnotation(&fusion)
              : Thunk::ThunkInfo::WithProfileAnnotation(fusion_op),
          /*source_buffer=*/src_buffers_[i],
          /*destination_buffer=*/dst_buffers_[i],
          /*mem_size=*/src_buffers_[i].size(),
          /*source_value=*/ir_emitter_context.emit_ir_from_hlo() ? nullptr
                                                                 : srcs_[i],
          /*destination_value=*/ir_emitter_context.emit_ir_from_hlo()
              ? nullptr
              : dsts_[i]));
    }
  }
  return result;
}

}  // namespace gpu
}  // namespace xla
