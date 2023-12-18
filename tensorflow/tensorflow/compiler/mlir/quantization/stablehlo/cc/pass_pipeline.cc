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
#include "tensorflow/compiler/mlir/quantization/stablehlo/cc/pass_pipeline.h"

#include "mlir/Pass/PassManager.h"  // from @llvm-project
#include "mlir/Transforms/Passes.h"  // from @llvm-project
#include "tensorflow/compiler/mlir/quantization/stablehlo/passes/passes.h"
#include "tensorflow/compiler/mlir/tensorflow/transforms/passes.h"

namespace mlir::quant::stablehlo {

void AddXlaCallModuleOpDeserializationPasses(OpPassManager& pm) {
  pm.addPass(TF::CreateXlaCallModuleDeserializationPass());
  pm.addPass(createRestoreFunctionNamePass());
  pm.addPass(createUnwrapXlaCallModuleOpPass());
  pm.addPass(createSymbolDCEPass());
}

}  // namespace mlir::quant::stablehlo
