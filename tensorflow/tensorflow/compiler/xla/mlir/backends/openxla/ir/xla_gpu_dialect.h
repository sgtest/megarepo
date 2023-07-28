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

#ifndef TENSORFLOW_COMPILER_XLA_MLIR_BACKENDS_OPENXLA_IR_XLA_GPU_DIALECT_H_
#define TENSORFLOW_COMPILER_XLA_MLIR_BACKENDS_OPENXLA_IR_XLA_GPU_DIALECT_H_

#include "mlir/IR/Dialect.h"  // from @llvm-project  // IWYU pragma: keep
#include "mlir/IR/OpImplementation.h"  // from @llvm-project  // IWYU pragma: keep

// XLA GPU dialect definition.
#include "tensorflow/compiler/xla/mlir/backends/openxla/ir/xla_gpu_dialect.h.inc"

#define GET_TYPEDEF_CLASSES
#include "tensorflow/compiler/xla/mlir/backends/openxla/ir/xla_gpu_types.h.inc"

#endif  // TENSORFLOW_COMPILER_XLA_MLIR_BACKENDS_OPENXLA_IR_XLA_GPU_DIALECT_H_
