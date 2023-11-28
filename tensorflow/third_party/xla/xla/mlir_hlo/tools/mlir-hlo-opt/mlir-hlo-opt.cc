/* Copyright 2020 The TensorFlow Authors. All Rights Reserved.

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

#include "deallocation/transforms/passes.h"
#include "lhlo/IR/lhlo_ops.h"
#include "lhlo/transforms/passes.h"
#include "lhlo_gpu/IR/lhlo_gpu_ops.h"
#include "mhlo/IR/register.h"
#include "mhlo/transforms/passes.h"
#include "mlir/InitAllDialects.h"
#include "mlir/InitAllExtensions.h"
#include "mlir/InitAllPasses.h"
#include "mlir/Tools/mlir-opt/MlirOptMain.h"
#include "stablehlo/dialect/Register.h"
#include "transforms/gpu_passes.h"
#include "transforms/passes.h"

using namespace mlir;

int main(int argc, char** argv) {
  registerAllPasses();
  deallocation::registerDeallocationPasses();
  hlo::registerLMHLOTransformsPasses();
  lmhlo::registerAllLmhloPasses();
  mhlo::registerAllMhloPasses();
  registerLMHLOGPUTransformsPasses();

  DialectRegistry registry;
  registerAllDialects(registry);
  registerAllExtensions(registry);
  mhlo::registerAllMhloDialects(registry);
  stablehlo::registerAllDialects(registry);
  registry.insert<lmhlo::LmhloDialect, lmhlo_gpu::LmhloGpuDialect>();
  return failed(MlirOptMain(argc, argv, "MLIR HLO pass driver\n", registry));
}
