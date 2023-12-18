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
#ifndef TENSORFLOW_COMPILER_MLIR_QUANTIZATION_STABLEHLO_CC_POST_CALIBRATION_H_
#define TENSORFLOW_COMPILER_MLIR_QUANTIZATION_STABLEHLO_CC_POST_CALIBRATION_H_

#include "absl/log/die_if_null.h"
#include "absl/status/statusor.h"
#include "mlir/IR/BuiltinOps.h"  // from @llvm-project
#include "mlir/IR/MLIRContext.h"  // from @llvm-project
#include "mlir/Pass/PassManager.h"  // from @llvm-project
#include "tensorflow/compiler/mlir/quantization/stablehlo/cc/component.h"
#include "tensorflow/compiler/mlir/quantization/stablehlo/quantization_config.pb.h"

namespace mlir::quant::stablehlo {

// Performs post-calibration graph transformation as part of post-training
// static-range quantization.
//
// The resulting `ModuleOp` contains quantized StableHLO ops serialized in
// `TF::XlaCallModuleOp`s. They are quantized using the statistics collected
// after the calibration step, corresponding to each `TF::CustomAggregatorOp`s
// in the input module op.
class PostCalibrationComponent : public Component {
 public:
  explicit PostCalibrationComponent(MLIRContext* ctx)
      : ctx_(*ABSL_DIE_IF_NULL(ctx)) {}  // Crash OK

  absl::StatusOr<ModuleOp> Run(
      ModuleOp module_op,
      const ::stablehlo::quantization::QuantizationConfig& config) override;

  // Adds MLIR passes to the pass manager. `Run` will essentially run these
  // passes on the module op.
  void AddPasses(OpPassManager& pm);

 private:
  MLIRContext& ctx_;
};

}  // namespace mlir::quant::stablehlo

#endif  // TENSORFLOW_COMPILER_MLIR_QUANTIZATION_STABLEHLO_CC_POST_CALIBRATION_H_
