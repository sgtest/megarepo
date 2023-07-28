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

#include "tensorflow/compiler/xla/mlir/backends/openxla/transforms/passes.h"

#include "mlir/Pass/PassManager.h"  // from @llvm-project
#include "mlir/Transforms/Passes.h"  // from @llvm-project

namespace impl {
namespace {
#define GEN_PASS_REGISTRATION
#include "tensorflow/compiler/xla/mlir/backends/openxla/transforms/passes.h.inc"
}  // namespace
}  // namespace impl

namespace xla::gpu {

using namespace mlir;  // NOLINT

void registerOpenXlaPases() { ::impl::registerPasses(); }

void populateOpenXlaRuntimePasses(mlir::OpPassManager& pm,
                                  ThunkSequence* thunk_sequence,
                                  OpenXlaBackend backend) {
  pm.addPass(createConvertToOpenXlaPass(thunk_sequence, backend));
  pm.addPass(createCanonicalizerPass());
}

}  // namespace xla::gpu
