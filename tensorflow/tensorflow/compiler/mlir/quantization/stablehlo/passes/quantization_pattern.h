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

#ifndef TENSORFLOW_COMPILER_MLIR_QUANTIZATION_STABLEHLO_PASSES_QUANTIZATION_PATTERN_H_
#define TENSORFLOW_COMPILER_MLIR_QUANTIZATION_STABLEHLO_PASSES_QUANTIZATION_PATTERN_H_

#include <string>
#include <type_traits>
#include <utility>

#include "absl/container/flat_hash_set.h"
#include "llvm/ADT/DenseMap.h"
#include "llvm/ADT/STLExtras.h"
#include "llvm/ADT/SmallVector.h"
#include "llvm/Support/Casting.h"
#include "mlir/Dialect/Func/IR/FuncOps.h"  // from @llvm-project
#include "mlir/Dialect/Quant/QuantTypes.h"  // from @llvm-project
#include "mlir/IR/Attributes.h"  // from @llvm-project
#include "mlir/IR/BuiltinAttributes.h"  // from @llvm-project
#include "mlir/IR/BuiltinTypes.h"  // from @llvm-project
#include "mlir/IR/IRMapping.h"  // from @llvm-project
#include "mlir/IR/Location.h"  // from @llvm-project
#include "mlir/IR/MLIRContext.h"  // from @llvm-project
#include "mlir/IR/Matchers.h"  // from @llvm-project
#include "mlir/IR/OpDefinition.h"  // from @llvm-project
#include "mlir/IR/OperationSupport.h"  // from @llvm-project
#include "mlir/IR/PatternMatch.h"  // from @llvm-project
#include "mlir/IR/TypeUtilities.h"  // from @llvm-project
#include "mlir/IR/Types.h"  // from @llvm-project
#include "mlir/IR/Value.h"  // from @llvm-project
#include "mlir/Support/LLVM.h"  // from @llvm-project
#include "mlir/Support/LogicalResult.h"  // from @llvm-project
#include "tensorflow/compiler/mlir/lite/quantization/ir/QuantOps.h"
#include "tensorflow/compiler/mlir/lite/quantization/quantization_utils.h"
#include "tensorflow/compiler/mlir/quantization/stablehlo/ops/stablehlo_op_quant_spec.h"
#include "tensorflow/compiler/mlir/tensorflow/ir/tf_ops.h"
#include "tensorflow/compiler/mlir/tensorflow/utils/xla_call_module_attrs.h"
#include "tensorflow/core/framework/types.pb.h"

namespace mlir::quant::stablehlo {

// Checks if an op is quantizable in StableHLO quantizer. Argument op is not
// necessarily a StableHLO op.
bool IsOpQuantizableStableHlo(Operation* op);

// A base rewrite pattern which matches any N-in-M-out operations with
// quantization parameters propagated to at least one of its operands. The
// quantization parameters are annotated by the QuantizeOp/DequantizeOp pairs.
// Each matched pattern are rewritten by its quantized alternatives.
//
// The concrete pattern, extends from this base pattern, can specify whether it
// allows dynamic range quantized operands and results for the operations in the
// current context. These "DynamicRangeQuantized" operands and results don't
// have quantization parameters propagated to, so will be in float in the
// quantized results. The concrete pattern should define the following two
// functions:
//
//   bool AllowDynamicRangeQuantizedOperand(Operation&) const
//   bool AllowDynamicRangeQuantizedResult(Operation&) const
//
// Full integer quantization disallows "DynamicRangeQuantized" operands or
// results. Dynamic range quantization allows "DynamicRangeQuantized" operands
// and results.
//
// Implementation of this pattern is mostly copied from QuantizationPattern in
// third_party/tensorflow/compiler/mlir/lite/quantization/quantization_utils.h.
template <typename ConcreteT, typename QuantizeOpT, typename DequantizeOpT,
          typename VerifierT, typename RootOpT = DequantizeOpT>
class StableHloQuantizationPattern : public RewritePattern {
 public:
  using BaseType =
      StableHloQuantizationPattern<ConcreteT, QuantizeOpT, DequantizeOpT,
                                   VerifierT, RootOpT>;

  explicit StableHloQuantizationPattern(
      MLIRContext* context, const mlir::quant::QuantPassSpec& quant_params)
      // Set the score to a large number so it is always preferred.
      : RewritePattern(RootOpT::getOperationName(), 300, context),
        quant_params_(quant_params) {}

  LogicalResult matchAndRewrite(Operation* op,
                                PatternRewriter& rewriter) const override {
    llvm::SmallVector<Operation*, 4> quantizing_ops;

    // Collect all the ops to quantize, as the user / producer of the root op.
    if constexpr (std::is_same_v<RootOpT, DequantizeOpT>) {
      if (op->getNumResults() != 1) {
        op->emitError("Dequantize op should have exactly one result.");
        return failure();
      }
      auto users = op->getResult(0).getUsers();
      quantizing_ops.append(users.begin(), users.end());
    } else if constexpr (std::is_same_v<RootOpT, QuantizeOpT>) {
      if (op->getNumOperands() != 1) {
        op->emitError("Quantize op should have exactly one operand.");
        return failure();
      }
      Value quantize_operand = op->getOperand(0);
      if (QuantizedType::getQuantizedElementType(quantize_operand.getType())) {
        // The input of the quantize op has already been quantized, i.e.
        // rescale.
        return failure();
      }
      DenseFPElementsAttr attr;
      if (matchPattern(quantize_operand, m_Constant(&attr))) {
        // Const-> QuantizeOp pattern will be handled separately.
        return failure();
      }
      if (Operation* quantizing_op = quantize_operand.getDefiningOp()) {
        quantizing_ops.push_back(quantizing_op);
      }
    }

    absl::flat_hash_set<std::string> ops_blocklist =
        quant_params_.quant_spec.ops_blocklist;
    absl::flat_hash_set<std::string> nodes_blocklist =
        quant_params_.quant_spec.nodes_blocklist;
    CustomMap custom_map = quant_params_.quant_spec.custom_map;

    // Rewrite the floating-point ops to the quantized version, by fusing
    // preceding dequantize ops and succeding quantize ops.
    for (Operation* quantizing_op : quantizing_ops) {
      // If it is requantize op, we shouldn't rewrite this op.
      if (llvm::isa<QuantizeOpT, DequantizeOpT>(quantizing_op)) {
        return failure();
      }

      // If the op is terminator, we shouldn't rewrite.
      if (quantizing_op->hasTrait<OpTrait::IsTerminator>()) {
        return failure();
      }

      if (!IsOpQuantizableStableHlo(quantizing_op) &&
          !static_cast<const ConcreteT*>(this)->IsQuantizableCustomOp(
              *quantizing_op, custom_map)) {
        return failure();
      }

      if (GetStableHloQuantScaleSpec(quantizing_op)
              ->has_same_scale_requirement &&
          !IsConnectedWithQuantizedCompsiteFunction(quantizing_op)) {
        return failure();
      }

      // Blocklist op is checked in advance for non-dynamic range quantization
      // case.
      if (!quant_params_.quant_spec.weight_quantization &&
          (ops_blocklist.find(quantizing_op->getName().getStringRef().str()) !=
           ops_blocklist.end())) {
        return failure();
      }

      if (!nodes_blocklist.empty()) {
        if (auto name_loc = quantizing_op->getLoc().dyn_cast<NameLoc>()) {
          std::string sloc = name_loc.getName().str();
          if (!sloc.empty() &&
              (nodes_blocklist.find(sloc) != nodes_blocklist.end())) {
            return failure();
          }
        }
      }

      // Collect all the quantized inputs and "clone" the matched op by these
      // inputs.
      SmallVector<Value, 4> inputs;
      inputs.reserve(quantizing_op->getNumOperands());
      for (auto operand : quantizing_op->getOperands()) {
        Type operand_type = operand.getType();
        if (operand_type.isa<NoneType>()) {
          inputs.push_back(operand);
          continue;
        }

        auto ele_type = operand.getType().cast<TensorType>().getElementType();
        if (auto dq_op =
                dyn_cast_or_null<DequantizeOpT>(operand.getDefiningOp())) {
          inputs.push_back(dq_op.getOperand());
        } else if (!ele_type.isF32()) {
          // If the operand is an integer tensor, then it doesn't require the
          // DequantizeOp in the pattern.
          inputs.push_back(operand);
        } else {
          return failure();
        }
      }

      // Collect all the quantized outputs and replace them by the results of
      // the new quantized op.
      llvm::SmallDenseMap<Value, int> outputs_replaced;
      SmallVector<Type, 4> output_types;
      output_types.reserve(quantizing_op->getNumResults());
      for (const auto& enumerated_result :
           llvm::enumerate(quantizing_op->getResults())) {
        Value result = enumerated_result.value();
        Type result_type = result.getType();
        // Add this to the test coverage once we create test ops with none type
        // results.
        if (result_type.isa<NoneType>()) {
          outputs_replaced.insert({result, enumerated_result.index()});
          output_types.push_back(result_type);
          continue;
        }
        Type result_ele_type =
            result.getType().cast<TensorType>().getElementType();
        // If the user is the QuantizeOp, it must be the only user.
        if (result.hasOneUse() &&
            llvm::isa<QuantizeOpT>(*result.user_begin())) {
          auto user = llvm::cast<QuantizeOpT>(*result.user_begin());
          outputs_replaced.insert(
              {user.getResult(), enumerated_result.index()});
          output_types.push_back(user.getType());
        } else if (!result_ele_type.isF32()) {
          // If the result is an integer tensor, then it doesn't require the
          // D op in the pattern.
          outputs_replaced.insert({result, enumerated_result.index()});
          output_types.push_back(result.getType());
        } else if (static_cast<const ConcreteT*>(this)
                       ->AllowDynamicRangeQuantizedResult(*quantizing_op,
                                                          custom_map)) {
          outputs_replaced.insert({result, enumerated_result.index()});
          output_types.push_back(result.getType());
        } else {
          return failure();
        }
      }

      rewriter.setInsertionPointAfter(quantizing_op);
      OperationState new_state(quantizing_op->getLoc(),
                               quantizing_op->getName().getStringRef(), inputs,
                               output_types, quantizing_op->getAttrs());
      for (int i = 0; i < quantizing_op->getNumRegions(); ++i) {
        new_state.addRegion();
      }
      Operation* quantized_op = rewriter.create(new_state);
      if (quantizing_op->getNumRegions() != 0) {
        for (const auto& indexed_regions :
             llvm::enumerate(quantizing_op->getRegions())) {
          Region& target_region =
              quantized_op->getRegion(indexed_regions.index());
          IRMapping mapping;
          indexed_regions.value().cloneInto(&target_region, mapping);
        }
      }
      for (auto output : outputs_replaced) {
        output.getFirst().replaceAllUsesWith(
            quantized_op->getResult(output.getSecond()));
      }
    }
    return success();
  }

 private:
  QuantPassSpec quant_params_;

  // Checks whether the operation is connnected with a quantized composite
  // function. If not, the same-scale op will not be quantized. This decision is
  // based on the current assumption that the performance gain of the same-scale
  // op itself could not beat the overhead of the quantize and dequantize
  // routines need to be added around that op. When the assumption changes,
  // this policy might change as well.
  bool IsConnectedWithQuantizedCompsiteFunction(
      Operation* same_scale_op) const {
    for (const auto& operand : same_scale_op->getOperands()) {
      auto dq_op = dyn_cast_or_null<quantfork::DequantizeCastOp>(
          operand.getDefiningOp());
      if (!dq_op) continue;

      Operation* preceding_op = dq_op.getArg().getDefiningOp();
      if (!preceding_op) continue;

      // Check whether the preceding op is a quantized composite function.
      if (llvm::isa<TF::XlaCallModuleOp>(preceding_op)) {
        auto call_op = llvm::cast<TF::XlaCallModuleOp>(preceding_op);
        if (!IsQuantizedCompositeFunction(call_op)) continue;
        return true;
      }

      // Check whether the preceding op is a quantized same-scale op.
      if (GetStableHloQuantScaleSpec(preceding_op)
              ->has_same_scale_requirement) {
        for (auto result : preceding_op->getResults()) {
          auto element_type = getElementTypeOrSelf(result.getType());
          if (element_type.isa<UniformQuantizedType>()) {
            return true;
          }
        }
      }
    }

    for (const auto& result : same_scale_op->getResults()) {
      // If the user is the Quantize op, it must be the only user.
      if (!result.hasOneUse() ||
          !llvm::isa<quantfork::QuantizeCastOp>(*result.user_begin())) {
        continue;
      }

      auto q_op = llvm::cast<quantfork::QuantizeCastOp>(*result.user_begin());
      for (auto following_op : q_op->getUsers()) {
        // Check whether the following op is a quantized composite function.
        if (llvm::isa<TF::XlaCallModuleOp>(following_op)) {
          auto call_op = llvm::cast<TF::XlaCallModuleOp>(following_op);
          if (!IsQuantizedCompositeFunction(call_op)) continue;
          return true;
        }

        // Check whether the following op is a quantized same-scale op.
        if (GetStableHloQuantScaleSpec(following_op)
                ->has_same_scale_requirement) {
          for (auto operand : following_op->getOperands()) {
            auto element_type = getElementTypeOrSelf(operand.getType());
            if (element_type.isa<UniformQuantizedType>()) {
              return true;
            }
          }
        }
      }
    }

    return false;
  }

  // Checks if op calls a composite function and all the inputs and outputs are
  // quantized.
  bool IsQuantizedCompositeFunction(TF::XlaCallModuleOp call_op) const {
    if (!call_op->hasAttr(kQuantTraitAttrName)) {
      return false;
    }

    const auto function_name = call_op->getAttrOfType<FlatSymbolRefAttr>(
        TF::kStablehloEntryFunctionAttrName);
    if (!function_name || !function_name.getValue().startswith("composite_")) {
      return false;
    }

    bool has_quantized_types = false;
    for (Value input : call_op.getArgs()) {
      if (auto type = input.getType().dyn_cast<TensorType>()) {
        if (type.getElementType().isa<FloatType>()) {
          return false;
        }
        if (type.getElementType().isa<UniformQuantizedType>()) {
          has_quantized_types = true;
        }
      }
    }
    for (Value output : call_op.getOutput()) {
      if (auto type = output.getType().dyn_cast<TensorType>()) {
        if (type.getElementType().isa<FloatType>()) {
          return false;
        }
        if (type.getElementType().isa<UniformQuantizedType>()) {
          has_quantized_types = true;
        }
      }
    }
    return has_quantized_types;
  }
};

}  // namespace mlir::quant::stablehlo

#endif  // TENSORFLOW_COMPILER_MLIR_QUANTIZATION_STABLEHLO_PASSES_QUANTIZATION_PATTERN_H_
