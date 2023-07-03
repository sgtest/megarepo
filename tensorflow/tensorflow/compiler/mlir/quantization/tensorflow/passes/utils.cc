/* Copyright 2022 The TensorFlow Authors. All Rights Reserved.

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
#include "tensorflow/compiler/mlir/quantization/tensorflow/passes/utils.h"

#include <memory>

#include "llvm/ADT/STLExtras.h"
#include "tensorflow/compiler/mlir/lite/quantization/quantization_utils.h"
#include "tensorflow/compiler/mlir/tensorflow/ir/tf_ops.h"
#include "tensorflow/compiler/mlir/tensorflow/utils/eval_util.h"

namespace mlir {
namespace quant {

bool HasQuantizedTensors(Operation* op) {
  if (!IsOpQuantizable(op)) return false;
  for (Type operand_type : op->getOperandTypes()) {
    auto tensor_type = operand_type.dyn_cast<TensorType>();
    if (tensor_type && tensor_type.getElementType().isa<QuantizedType>()) {
      return true;
    }
  }
  for (Type result_type : op->getResultTypes()) {
    auto tensor_type = result_type.dyn_cast<TensorType>();
    if (tensor_type && tensor_type.getElementType().isa<QuantizedType>()) {
      return true;
    }
  }
  return false;
}

bool HasStaticShape(Value value) {
  auto shaped_type = value.getType().dyn_cast<ShapedType>();
  if (!shaped_type) return false;

  return shaped_type.hasStaticShape();
}

bool HasStaticShapeAtDims(Value value, ArrayRef<int> dims) {
  auto shaped_type = value.getType().dyn_cast<ShapedType>();
  if (!shaped_type || !shaped_type.hasRank()) return false;

  for (auto dim : dims) {
    if (shaped_type.isDynamicDim(dim)) return false;
  }
  return true;
}

Type CloneTypeWithNewElementType(Type old_type, Type element_type) {
  if (!old_type.isa<ShapedType>()) return {};

  return old_type.cast<ShapedType>().clone(element_type);
}

SmallVector<Value> CloneOpWithReplacedOperands(
    OpBuilder& builder, Operation* op, const SmallVector<Value>& new_operands) {
  IRMapping mapping;
  for (const auto& arg : enumerate(new_operands)) {
    mapping.map(op->getOperand(arg.index()), arg.value());
  }
  return builder.clone(*op, mapping)->getResults();
}

}  // namespace quant
}  // namespace mlir
