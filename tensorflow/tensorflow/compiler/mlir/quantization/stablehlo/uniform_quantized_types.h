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
#ifndef TENSORFLOW_COMPILER_MLIR_QUANTIZATION_STABLEHLO_UNIFORM_QUANTIZED_TYPES_H_
#define TENSORFLOW_COMPILER_MLIR_QUANTIZATION_STABLEHLO_UNIFORM_QUANTIZED_TYPES_H_

#include <cstdint>

#include "mlir/Dialect/Quant/QuantTypes.h"  // from @llvm-project
#include "mlir/IR/BuiltinTypes.h"  // from @llvm-project
#include "mlir/IR/Location.h"  // from @llvm-project
#include "mlir/IR/MLIRContext.h"  // from @llvm-project
#include "mlir/IR/Types.h"  // from @llvm-project
#include "mlir/Support/LLVM.h"  // from @llvm-project

namespace mlir {
namespace quant {

// Creates a `UniformQuantizedType` with the given `scale` and `zero_point`
// values. The produced type has f32 as its expressed type and i8 as its
// storage type. The available values use the full range of the storage value,
// i.e. [-128, 127]. Assumes asymmetric quantization, meaning the zero point
// values can be non-zero values.
UniformQuantizedType CreateI8F32UniformQuantizedType(Location loc,
                                                     MLIRContext& context,
                                                     double scale,
                                                     int64_t zero_point);

// Creates a `UniformQuantizedType` with the given `scale` and `zero_point`
// values. The produced type has f32 as its expressed type and i32 as its
// storage type. The available values use the full range of the storage value.
// Assumes asymmetric quantization, meaning the zero point values can be
// non-zero values.
UniformQuantizedType CreateI32F32UniformQuantizedType(Location loc,
                                                      MLIRContext& context,
                                                      double scale,
                                                      int64_t zero_point);

// Creates a `UniformQuantizedPerAxisType` with the given `scales` and
// `zero_points` values. The produced type has f32 as its expressed type and
// i8 as its storage type. The available values use the full range of the
// storage value, i.e. [-128, 127]. Assumes asymmetric quantization, meaning the
// zero point values can be non-zero values.
UniformQuantizedPerAxisType CreateI8F32UniformQuantizedPerAxisType(
    Location loc, MLIRContext& context, ArrayRef<double> scales,
    ArrayRef<int64_t> zero_points, int quantization_dimension);

bool IsStorageTypeI8(QuantizedType quantized_type);

bool IsStorageTypeI32(QuantizedType quantized_type);

bool IsExpressedTypeF32(QuantizedType quantized_type);

// Returns true iff `type` is a uniform quantized type whose storage type is
// 8-bit integer and expressed type is f32.
bool IsI8F32UniformQuantizedType(Type type);

// Returns true iff `type` is a uniform quantized per-axis (per-channel) type
// whose storage type is 8-bit integer and expressed type is f32.
bool IsI8F32UniformQuantizedPerAxisType(Type type);

// Returns true iff `type` is a uniform quantized type whose storage type is
// 32-bit integer and expressed type is f32.
bool IsI32F32UniformQuantizedType(Type type);

// Determines whether the storage type of a quantized type is supported by
// `tfl.quantize` or `tfl.dequantize` ops. ui8, i8 and i16 are supported.
bool IsSupportedByTfliteQuantizeOrDequantizeOps(IntegerType storage_type);

}  // namespace quant
}  // namespace mlir

#endif  // TENSORFLOW_COMPILER_MLIR_QUANTIZATION_STABLEHLO_UNIFORM_QUANTIZED_TYPES_H_
