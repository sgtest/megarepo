/* Copyright 2021 The TensorFlow Authors. All Rights Reserved.

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

#include <cstdint>
#include <cstring>
#include <memory>
#include <optional>
#include <string>
#include <string_view>
#include <utility>

#include "llvm/ADT/STLExtras.h"
#include "llvm/ADT/SmallVector.h"
#include "llvm/Support/Casting.h"
#include "mlir/Dialect/Func/IR/FuncOps.h"  // from @llvm-project
#include "mlir/Dialect/Quant/QuantOps.h"  // from @llvm-project
#include "mlir/Dialect/Quant/QuantTypes.h"  // from @llvm-project
#include "mlir/Dialect/Shape/IR/Shape.h"  // from @llvm-project
#include "mlir/Dialect/SparseTensor/IR/SparseTensor.h"  // from @llvm-project
#include "mlir/IR/BuiltinAttributes.h"  // from @llvm-project
#include "mlir/IR/BuiltinTypes.h"  // from @llvm-project
#include "mlir/IR/MLIRContext.h"  // from @llvm-project
#include "mlir/IR/OperationSupport.h"  // from @llvm-project
#include "mlir/IR/PatternMatch.h"  // from @llvm-project
#include "mlir/IR/TypeUtilities.h"  // from @llvm-project
#include "mlir/Pass/Pass.h"  // from @llvm-project
#include "mlir/Support/LLVM.h"  // from @llvm-project
#include "mlir/Support/LogicalResult.h"  // from @llvm-project
#include "mlir/Transforms/DialectConversion.h"  // from @llvm-project
#include "stablehlo/dialect/ChloOps.h"  // from @stablehlo
#include "tensorflow/compiler/mlir/quantization/stablehlo/passes/bridge/passes.h"
#include "tensorflow/compiler/mlir/quantization/stablehlo/utils/math_utils.h"
#include "tensorflow/compiler/mlir/tf2xla/transforms/xla_legalize_targets.h"
#include "xla/mlir_hlo/mhlo/IR/hlo_ops.h"
#include "xla/mlir_hlo/mhlo/transforms/rewriters.h"

namespace mlir {
namespace stablehlo {
namespace {

#define GEN_PASS_DEF_CONVERTMHLOQUANTTOINT
#include "tensorflow/compiler/mlir/quantization/stablehlo/passes/bridge/passes.h.inc"

// This helper function create ops to requantize `input` tensor and output to
// `res_int32` tensor. Clamping is omitted because for some ops clamping can be
// done later to avoid duplicate.
LogicalResult RequantizeWithoutClamping(
    mlir::OpState op, Value input, TensorType int32_tensor_type,
    quant::UniformQuantizedType input_quantized_type,
    quant::UniformQuantizedType result_quantized_type, Value &res_int32,
    ConversionPatternRewriter &rewriter) {
  // Skip requantization when input and result have the same type.
  if (input_quantized_type == result_quantized_type) {
    res_int32 = rewriter.create<mhlo::ConvertOp>(op->getLoc(),
                                                 int32_tensor_type, input);
    return success();
  }

  // Convert input to int32 tensor.
  res_int32 =
      rewriter.create<mhlo::ConvertOp>(op->getLoc(), int32_tensor_type, input);
  // Undo the input zero point.
  Value input_zero_point = rewriter.create<mhlo::ConstantOp>(
      op->getLoc(), rewriter.getI32IntegerAttr(static_cast<int32_t>(
                        input_quantized_type.getZeroPoint())));
  res_int32 = rewriter.create<chlo::BroadcastSubOp>(
      op->getLoc(), int32_tensor_type, res_int32, input_zero_point, nullptr);

  // Adjust the scale.
  const double effective_scale =
      input_quantized_type.getScale() / result_quantized_type.getScale();
  int32_t effective_quantized_fraction;
  int32_t effective_shift;
  if (failed(stablehlo::QuantizeMultiplier(
          effective_scale, effective_quantized_fraction, effective_shift))) {
    op->emitError("Invalid effective quantization scale.");
    return failure();
  }
  Value multiplier = rewriter.create<mhlo::ConstantOp>(
      op->getLoc(), rewriter.getI32IntegerAttr(
                        static_cast<int32_t>(effective_quantized_fraction)));
  // The effective_quantized_fraction value has been quantized by multiplying
  // (1 << 15).  So, we have to shift it back by (15 - effective_shift) to get
  // the desired outcome.
  Value total_shift = rewriter.create<mhlo::ConstantOp>(
      op->getLoc(),
      rewriter.getI32IntegerAttr(static_cast<int32_t>(15 - effective_shift)));

  // Apply the effective scale with rounding.
  Value half = rewriter.create<mhlo::ConstantOp>(
      op->getLoc(), rewriter.getI32IntegerAttr(
                        static_cast<int32_t>(1 << (14 - effective_shift))));
  res_int32 = rewriter.create<chlo::BroadcastMulOp>(
      op->getLoc(), int32_tensor_type, res_int32, multiplier, nullptr);
  res_int32 = rewriter.create<chlo::BroadcastAddOp>(
      op->getLoc(), int32_tensor_type, res_int32, half, nullptr);
  res_int32 = rewriter.create<chlo::BroadcastShiftRightArithmeticOp>(
      op->getLoc(), int32_tensor_type, res_int32, total_shift, nullptr);

  // Apply the output zero point.
  Value output_zero_point = rewriter.create<mhlo::ConstantOp>(
      op->getLoc(), rewriter.getI32IntegerAttr(static_cast<int32_t>(
                        result_quantized_type.getZeroPoint())));
  res_int32 = rewriter.create<chlo::BroadcastAddOp>(
      op->getLoc(), int32_tensor_type, res_int32, output_zero_point, nullptr);

  return success();
}

class ConvertMHLOQuantToInt
    : public impl::ConvertMHLOQuantToIntBase<ConvertMHLOQuantToInt> {
 public:
  ConvertMHLOQuantToInt() = default;
  ConvertMHLOQuantToInt(const ConvertMHLOQuantToInt &) {}

  explicit ConvertMHLOQuantToInt(bool legalize_chlo) {
    legalize_chlo_ = legalize_chlo;
  }

  // Performs conversion of MHLO quant ops to primitive ops.
  void runOnOperation() override;
};

class ConvertUniformQuantizeOp
    : public OpConversionPattern<mhlo::UniformQuantizeOp> {
 public:
  using OpConversionPattern::OpConversionPattern;

  LogicalResult matchAndRewrite(
      mhlo::UniformQuantizeOp op, mhlo::UniformQuantizeOpAdaptor adaptor,
      ConversionPatternRewriter &rewriter) const override {
    auto quantized_type = getElementTypeOrSelf(op.getResult().getType())
                              .dyn_cast<quant::UniformQuantizedType>();
    // Currently for activation, PTQ supports per-tensor quantization only, and
    // UniformQuantize op is only for activation.
    if (!quantized_type) {
      return rewriter.notifyMatchFailure(
          op, "Legalization supports only per-tensor quantization.");
    }
    auto input_element_type = getElementTypeOrSelf(op.getOperand().getType());
    if (input_element_type.isF32()) {
      return matchAndRewriteQuantize(op, adaptor, rewriter, quantized_type);
    } else if (input_element_type.isa<quant::UniformQuantizedType>()) {
      return matchAndRewriteRequantize(op, adaptor, rewriter, quantized_type);
    }
    return rewriter.notifyMatchFailure(op, "Unsupported input element type.");
  }

  LogicalResult matchAndRewriteQuantize(
      mhlo::UniformQuantizeOp op, mhlo::UniformQuantizeOpAdaptor adaptor,
      ConversionPatternRewriter &rewriter,
      const quant::UniformQuantizedType &quantized_type) const {
    Value scale = rewriter.create<mhlo::ConstantOp>(
        op->getLoc(), rewriter.getF32FloatAttr(quantized_type.getScale()));
    Value zero_point = rewriter.create<mhlo::ConstantOp>(
        op->getLoc(), rewriter.getF32FloatAttr(
                          static_cast<float>(quantized_type.getZeroPoint())));
    Value quantization_min = rewriter.create<mhlo::ConstantOp>(
        op->getLoc(), rewriter.getF32FloatAttr(static_cast<float>(
                          quantized_type.getStorageTypeMin())));
    Value quantization_max = rewriter.create<mhlo::ConstantOp>(
        op->getLoc(), rewriter.getF32FloatAttr(static_cast<float>(
                          quantized_type.getStorageTypeMax())));

    auto res_float_tensor_type =
        op.getOperand().getType().clone(rewriter.getF32Type());
    Value res_float = rewriter.create<chlo::BroadcastDivOp>(
        op->getLoc(), res_float_tensor_type, adaptor.getOperand(), scale,
        nullptr);
    res_float = rewriter.create<chlo::BroadcastAddOp>(
        op->getLoc(), res_float_tensor_type, res_float, zero_point, nullptr);

    res_float = rewriter.create<mhlo::ClampOp>(
        op->getLoc(), res_float_tensor_type, quantization_min, res_float,
        quantization_max);
    res_float = rewriter.create<mhlo::RoundNearestEvenOp>(
        op->getLoc(), res_float_tensor_type, res_float);
    auto res_final_tensor_type =
        res_float_tensor_type.clone(quantized_type.getStorageType());
    rewriter.replaceOpWithNewOp<mhlo::ConvertOp>(op, res_final_tensor_type,
                                                 res_float);
    return success();
  }

  // Requantization is essentially dequantize --> quantize.
  //
  // Dequantize: (input - zp) * scale
  // Quantize: input / scale + zp
  //
  // Hence,
  //   result = (input - input_zp) * input_scale / output_scale + output_zp
  //
  // This is simplified as:
  //   result = input * merged_scale + merged_zp
  // where:
  //   merged_zp = output_zp - input_zp * merged_scale.
  //   merged_scale = input_scale / output_scale.
  LogicalResult matchAndRewriteRequantize(
      mhlo::UniformQuantizeOp op, mhlo::UniformQuantizeOpAdaptor adaptor,
      ConversionPatternRewriter &rewriter,
      const quant::UniformQuantizedType &output_quantized_type) const {
    auto input_quantized_type = getElementTypeOrSelf(op.getOperand().getType())
                                    .cast<quant::UniformQuantizedType>();
    auto result_quantized_type = getElementTypeOrSelf(op.getResult().getType())
                                     .cast<quant::UniformQuantizedType>();

    double merged_scale_fp =
        input_quantized_type.getScale() / result_quantized_type.getScale();
    Value merged_scale = rewriter.create<mhlo::ConstantOp>(
        op->getLoc(),
        rewriter.getF32FloatAttr(static_cast<float>(merged_scale_fp)));

    auto res_float_tensor_type =
        op.getOperand().getType().clone(rewriter.getF32Type());
    Value res_float = rewriter.create<mhlo::ConvertOp>(
        op->getLoc(), res_float_tensor_type, adaptor.getOperand());

    res_float = rewriter.create<chlo::BroadcastMulOp>(
        op->getLoc(), res_float_tensor_type, res_float, merged_scale, nullptr);

    // Add merged_zp only when it is non-zero.
    double merged_zp_fp = result_quantized_type.getZeroPoint() -
                          input_quantized_type.getZeroPoint() * merged_scale_fp;
    if (merged_zp_fp != 0) {
      Value merged_zp = rewriter.create<mhlo::ConstantOp>(
          op->getLoc(),
          rewriter.getF32FloatAttr(static_cast<float>(merged_zp_fp)));
      res_float = rewriter.create<chlo::BroadcastAddOp>(
          op->getLoc(), res_float_tensor_type, res_float, merged_zp, nullptr);
    }

    Value quantization_min = rewriter.create<mhlo::ConstantOp>(
        op->getLoc(), rewriter.getF32FloatAttr(static_cast<float>(
                          output_quantized_type.getStorageTypeMin())));
    Value quantization_max = rewriter.create<mhlo::ConstantOp>(
        op->getLoc(), rewriter.getF32FloatAttr(static_cast<float>(
                          output_quantized_type.getStorageTypeMax())));

    // Clamp results by [quantization_min, quantization_max].
    res_float = rewriter.create<mhlo::ClampOp>(
        op->getLoc(), res_float_tensor_type, quantization_min, res_float,
        quantization_max);
    res_float = rewriter.create<mhlo::RoundNearestEvenOp>(
        op->getLoc(), res_float_tensor_type, res_float);

    auto res_final_tensor_type =
        res_float_tensor_type.clone(output_quantized_type.getStorageType());
    rewriter.replaceOpWithNewOp<mhlo::ConvertOp>(op, res_final_tensor_type,
                                                 res_float);
    return success();
  }
};

class ConvertUniformDequantizeOp
    : public OpConversionPattern<mhlo::UniformDequantizeOp> {
 public:
  using OpConversionPattern::OpConversionPattern;

  LogicalResult matchAndRewrite(
      mhlo::UniformDequantizeOp op, mhlo::UniformDequantizeOpAdaptor adaptor,
      ConversionPatternRewriter &rewriter) const override {
    auto element_type = getElementTypeOrSelf(op.getOperand().getType())
                            .dyn_cast<quant::UniformQuantizedType>();
    // Currently for activation, PTQ supports per-tensor quantization only, and
    // UniformQuantize op is only for activation.
    if (!element_type) {
      return rewriter.notifyMatchFailure(
          op, "Legalization supports only per-tensor quantization.");
    }
    Value scale = rewriter.create<mhlo::ConstantOp>(
        op->getLoc(), rewriter.getF32FloatAttr(element_type.getScale()));
    Value zero_point = rewriter.create<mhlo::ConstantOp>(
        op->getLoc(), rewriter.getI32IntegerAttr(
                          static_cast<int32_t>(element_type.getZeroPoint())));

    Value input = adaptor.getOperand();
    // TODO: b/260280919 - Consider avoiding conversion to int32.
    auto res_int32_tensor_type =
        input.getType().cast<TensorType>().clone(rewriter.getI32Type());
    Value res_int32 = rewriter.create<mhlo::ConvertOp>(
        op->getLoc(), res_int32_tensor_type, input);
    res_int32 = rewriter.create<chlo::BroadcastSubOp>(
        op->getLoc(), res_int32_tensor_type, res_int32, zero_point, nullptr);
    auto res_float_tensor_type =
        res_int32.getType().cast<TensorType>().clone(rewriter.getF32Type());
    Value res_float = rewriter.create<mhlo::ConvertOp>(
        op->getLoc(), res_float_tensor_type, res_int32);
    res_float = rewriter.replaceOpWithNewOp<chlo::BroadcastMulOp>(
        op, res_float_tensor_type, res_float, scale, nullptr);
    return success();
  }
};

class ConvertUniformQuantizedAddOp : public OpConversionPattern<mhlo::AddOp> {
 public:
  using OpConversionPattern::OpConversionPattern;

  LogicalResult matchAndRewrite(
      mhlo::AddOp op, mhlo::AddOpAdaptor adaptor,
      ConversionPatternRewriter &rewriter) const override {
    auto lhs_element_type = op.getLhs()
                                .getType()
                                .getElementType()
                                .dyn_cast<quant::UniformQuantizedType>();
    auto rhs_element_type = op.getRhs()
                                .getType()
                                .getElementType()
                                .dyn_cast<quant::UniformQuantizedType>();
    auto result_element_type = op.getResult()
                                   .getType()
                                   .getElementType()
                                   .dyn_cast<quant::UniformQuantizedType>();

    // We only handle cases where lhs, rhs and results all have quantized
    // element type.
    if (!lhs_element_type || !rhs_element_type || !result_element_type) {
      op->emitError(
          "AddOp requires the same quantized element type for all operands and "
          "results");
      return failure();
    }

    // TODO: b/260280919 - Consider avoiding conversion to int32.
    auto res_int32_tensor_type =
        op.getResult().getType().clone(rewriter.getI32Type());

    // When lhs, rhs and result have different scale and zps, requantize them to
    // be the same as the result.
    // TODO: b/260280919 - Consider avoiding conversion to int32.
    Value lhs = adaptor.getLhs();
    Value lhs_int32_tensor;
    if (failed(RequantizeWithoutClamping(op, lhs, res_int32_tensor_type,
                                         lhs_element_type, result_element_type,
                                         lhs_int32_tensor, rewriter))) {
      return failure();
    }

    Value rhs = adaptor.getRhs();
    Value rhs_int32_tensor;
    if (failed(RequantizeWithoutClamping(op, rhs, res_int32_tensor_type,
                                         rhs_element_type, result_element_type,
                                         rhs_int32_tensor, rewriter))) {
      return failure();
    }

    Value zero_point = rewriter.create<mhlo::ConstantOp>(
        op->getLoc(), rewriter.getI32IntegerAttr(static_cast<int32_t>(
                          result_element_type.getZeroPoint())));

    // Now the lhs and rhs have been coverted to the same scale and zps.
    // Given:
    // lhs_fp = (lhs_quant - zp) * scale
    // rhs_fp = (rhs_quant - zp) * scale
    // res_fp = lhs_fp + rhs_fp
    //        = ((lhs_quant + rhs_quant - zp) - zp) * scale
    // res_quant = res_fp / scale + zp
    //           = lhs_quant + rhs_quant - zp
    // The following add the inputs and then substract by zero point.
    Value add_result = rewriter.create<chlo::BroadcastAddOp>(
        op->getLoc(), res_int32_tensor_type, lhs_int32_tensor, rhs_int32_tensor,
        nullptr);
    Value res_int32 = rewriter.create<chlo::BroadcastSubOp>(
        op->getLoc(), res_int32_tensor_type, add_result, zero_point, nullptr);

    if (result_element_type.getStorageType().isInteger(32)) {
      // For i32, clamping is not needed.
      rewriter.replaceOp(op, res_int32);
    } else {
      // Clamp results by [quantization_min, quantization_max] when storage type
      // is not i32.
      Value result_quantization_min = rewriter.create<mhlo::ConstantOp>(
          op->getLoc(), rewriter.getI32IntegerAttr(static_cast<int32_t>(
                            result_element_type.getStorageTypeMin())));
      Value result_quantization_max = rewriter.create<mhlo::ConstantOp>(
          op->getLoc(), rewriter.getI32IntegerAttr(static_cast<int32_t>(
                            result_element_type.getStorageTypeMax())));
      res_int32 = rewriter.create<mhlo::ClampOp>(
          op->getLoc(), res_int32_tensor_type, result_quantization_min,
          res_int32, result_quantization_max);
      // Convert results back to result storage type.
      auto res_final_tensor_type =
          res_int32_tensor_type.clone(result_element_type.getStorageType());
      rewriter.replaceOpWithNewOp<mhlo::ConvertOp>(op, res_final_tensor_type,
                                                   res_int32);
    }

    return success();
  }
};

// A shared matchAndRewrite implementation for dot-like hybrid quantized
// operators. Hybrid ops are currently only interpreted as weight-only
// quantization ops, this might change in the future.
//
// All attrs of the original op are preserved after the conversion.
template <typename OpType, typename OpAdaptorType>
LogicalResult matchAndRewriteDotLikeHybridOp(
    OpType &op, OpAdaptorType &adaptor, ConversionPatternRewriter &rewriter,
    const quant::UniformQuantizedType &rhs_element_type) {
  // For dot like hybrid ops, lhs is float type, rhs is uniform
  // quantized type and result is float type.
  // For weight-only quantization:
  // result = hybridOp(lhs, dequant(rhs))
  Value lhs_float32_tensor = adaptor.getLhs();
  Value rhs = adaptor.getRhs();
  auto res_float32_tensor_type =
      op.getResult().getType().template cast<TensorType>();

  // Get scales and zero points for rhs.
  Value rhs_zero_point = rewriter.create<mhlo::ConstantOp>(
      op->getLoc(),
      rewriter.getF32FloatAttr((rhs_element_type.getZeroPoint())));
  Value rhs_scale_constant = rewriter.create<mhlo::ConstantOp>(
      op->getLoc(), rewriter.getF32FloatAttr(
                        static_cast<float_t>(rhs_element_type.getScale())));

  // Dequantize rhs_float32_tensor.
  Value rhs_float32_tensor = rewriter.create<mhlo::ConvertOp>(
      op->getLoc(), res_float32_tensor_type, rhs);
  rhs_float32_tensor = rewriter.create<chlo::BroadcastSubOp>(
      op->getLoc(), res_float32_tensor_type, rhs_float32_tensor, rhs_zero_point,
      nullptr);
  rhs_float32_tensor = rewriter.create<chlo::BroadcastMulOp>(
      op->getLoc(), res_float32_tensor_type, rhs_float32_tensor,
      rhs_scale_constant, nullptr);

  // Execute conversion target op.
  SmallVector<Value, 2> operands{lhs_float32_tensor, rhs_float32_tensor};
  Value res_float32 = rewriter.create<OpType>(
      op->getLoc(), res_float32_tensor_type, operands, op->getAttrs());

  Value half = rewriter.create<mhlo::ConstantOp>(
      op->getLoc(), rewriter.getF32FloatAttr(0.5f));
  res_float32 = rewriter.create<chlo::BroadcastAddOp>(
      op->getLoc(), res_float32_tensor_type, res_float32, half, nullptr);
  rewriter.replaceOpWithNewOp<mhlo::FloorOp>(op, res_float32);
  return success();
}

// A shared matchAndRewrite implementation for dot-like quantized operators.
//
// Dot-like operators refer to operators that generate a tensor where each
// element is obtained by multiplying an element from the lhs with an element
// from the rhs, possibly followed by summation.
// e.g. Dot, Multiply, Convolution
//
// All attrs of the original op are preserved after the conversion.
template <typename OpType, typename OpAdaptorType>
LogicalResult matchAndRewriteDotLikeOp(OpType &op, OpAdaptorType &adaptor,
                                       ConversionPatternRewriter &rewriter) {
  auto lhs_element_type = getElementTypeOrSelf(op.getLhs().getType());
  auto rhs_element_quant_type =
      op.getRhs()
          .getType()
          .getElementType()
          .template dyn_cast<quant::UniformQuantizedType>();
  auto res_element_type = getElementTypeOrSelf(op.getResult());

  // Check if the right operand is UniformQuantizedTypes.
  if (!rhs_element_quant_type) {
    return rewriter.notifyMatchFailure(
        op, "Legalization failed: supports only per-tensor quantization.");
  }

  if (lhs_element_type.template isa<quant::UniformQuantizedType>()) {
    // If lhs is uniform quantized type, result should also be uniform
    // quantized type, representing none-hybrid op.
    if (!res_element_type.template isa<quant::UniformQuantizedType>()) {
      op->emitError("Unsupported result element type for " +
                    op->getName().getStringRef().str());
      return failure();
    }
  } else if (lhs_element_type.isF32()) {
    // If lhs is float32 type, result should also be float32 type,
    // representing hybrid op.
    if (!res_element_type.isF32()) {
      op->emitError("Unsupported result element type for " +
                    op->getName().getStringRef().str());
      return failure();
    }
    return matchAndRewriteDotLikeHybridOp(op, adaptor, rewriter,
                                          rhs_element_quant_type);
  } else {
    return rewriter.notifyMatchFailure(op, "Unsupported input element type.");
  }

  auto lhs_float32_tensor_type =
      op.getLhs().getType().clone(rewriter.getF32Type());
  auto rhs_float32_tensor_type =
      op.getRhs().getType().clone(rewriter.getF32Type());
  auto res_float32_tensor_type =
      op.getResult().getType().clone(rewriter.getF32Type());

  auto lhs_element_quant_type =
      lhs_element_type.template dyn_cast<quant::UniformQuantizedType>();
  auto res_element_quant_type =
      res_element_type.template dyn_cast<quant::UniformQuantizedType>();
  Value lhs = adaptor.getLhs();
  Value rhs = adaptor.getRhs();

  // result =
  // op((lhs - zp_l) * scale_l, (rhs - zp_r) * scale_r) / scale_res + zp_res
  // =
  // op(lhs - zp_l, rhs - zp_r) * scale_l * scale_r / scale_res + zp_res
  // Get scales and zero points for both operands.
  Value lhs_zero_point = rewriter.create<mhlo::ConstantOp>(
      op->getLoc(),
      rewriter.getF32FloatAttr((lhs_element_quant_type.getZeroPoint())));
  Value rhs_zero_point = rewriter.create<mhlo::ConstantOp>(
      op->getLoc(),
      rewriter.getF32FloatAttr((rhs_element_quant_type.getZeroPoint())));

  // Offset xxx_int32_tensor according to zero points.
  Value lhs_float32_tensor = rewriter.create<mhlo::ConvertOp>(
      op->getLoc(), lhs_float32_tensor_type, lhs);
  lhs_float32_tensor = rewriter.create<chlo::BroadcastSubOp>(
      op->getLoc(), lhs_float32_tensor_type, lhs_float32_tensor, lhs_zero_point,
      nullptr);
  Value rhs_float32_tensor = rewriter.create<mhlo::ConvertOp>(
      op->getLoc(), rhs_float32_tensor_type, rhs);
  rhs_float32_tensor = rewriter.create<chlo::BroadcastSubOp>(
      op->getLoc(), rhs_float32_tensor_type, rhs_float32_tensor, rhs_zero_point,
      nullptr);

  // Execute the conversion target op.
  SmallVector<Value, 2> operands{lhs_float32_tensor, rhs_float32_tensor};
  Value res_float32 = rewriter.create<OpType>(
      op->getLoc(), res_float32_tensor_type, operands, op->getAttrs());

  // Get scale and zero point of result and offset res_int32 according to
  // scales.
  Value result_zero_point = rewriter.create<mhlo::ConstantOp>(
      op->getLoc(),
      rewriter.getF32FloatAttr((res_element_quant_type.getZeroPoint())));
  const double effective_scale = lhs_element_quant_type.getScale() *
                                 rhs_element_quant_type.getScale() /
                                 res_element_quant_type.getScale();
  Value effective_scale_constant = rewriter.create<mhlo::ConstantOp>(
      op->getLoc(),
      rewriter.getF32FloatAttr(static_cast<float_t>(effective_scale)));
  res_float32 = rewriter.create<chlo::BroadcastMulOp>(
      op->getLoc(), res_float32_tensor_type, res_float32,
      effective_scale_constant, nullptr);
  // MOT team figured out using floor(x+0.5) is much faster than using
  // round(x) on some TPU chips, see cl/449626238.
  Value half = rewriter.create<mhlo::ConstantOp>(
      op->getLoc(), rewriter.getF32FloatAttr(0.5f));
  res_float32 = rewriter.create<chlo::BroadcastAddOp>(
      op->getLoc(), res_float32_tensor_type, res_float32, half, nullptr);
  res_float32 = rewriter.create<mhlo::FloorOp>(op->getLoc(), res_float32);

  // Offset res_int32 according to result_zero_point.
  res_float32 = rewriter.create<chlo::BroadcastAddOp>(
      op->getLoc(), res_float32_tensor_type, res_float32, result_zero_point,
      nullptr);

  // Cast res_float_tensor_type to res_int_tensor_type.
  auto res_int32_tensor_type =
      op.getResult().getType().clone(rewriter.getI32Type());
  Value res_int32 = rewriter.create<mhlo::ConvertOp>(
      op->getLoc(), res_int32_tensor_type, res_float32);

  // Clamp results by [quantization_min, quantization_max].
  Value result_quantization_min = rewriter.create<mhlo::ConstantOp>(
      op->getLoc(), rewriter.getI32IntegerAttr(static_cast<int32_t>(
                        res_element_quant_type.getStorageTypeMin())));
  Value result_quantization_max = rewriter.create<mhlo::ConstantOp>(
      op->getLoc(), rewriter.getI32IntegerAttr(static_cast<int32_t>(
                        res_element_quant_type.getStorageTypeMax())));
  res_int32 = rewriter.create<mhlo::ClampOp>(
      op->getLoc(), res_int32_tensor_type, result_quantization_min, res_int32,
      result_quantization_max);

  // Convert results back to int8.
  auto res_final_tensor_type =
      res_int32_tensor_type.clone(res_element_quant_type.getStorageType());
  rewriter.replaceOpWithNewOp<mhlo::ConvertOp>(op, res_final_tensor_type,
                                               res_int32);

  return success();
}

class ConvertUniformQuantizedDotOp : public OpConversionPattern<mhlo::DotOp> {
 public:
  using OpConversionPattern::OpConversionPattern;

  LogicalResult matchAndRewrite(
      mhlo::DotOp op, mhlo::DotOpAdaptor adaptor,
      ConversionPatternRewriter &rewriter) const override {
    return matchAndRewriteDotLikeOp(op, adaptor, rewriter);
  }
};

class ConvertUniformQuantizedDotGeneralOp
    : public OpConversionPattern<mhlo::DotGeneralOp> {
 public:
  using OpConversionPattern::OpConversionPattern;

  LogicalResult matchAndRewrite(
      mhlo::DotGeneralOp op, mhlo::DotGeneralOpAdaptor adaptor,
      ConversionPatternRewriter &rewriter) const override {
    return matchAndRewriteDotLikeOp(op, adaptor, rewriter);
  }
};

class ConvertUniformQuantizedConvolutionOp
    : public OpConversionPattern<mhlo::ConvolutionOp> {
 public:
  using OpConversionPattern::OpConversionPattern;

  LogicalResult matchAndRewrite(
      mhlo::ConvolutionOp op, mhlo::ConvolutionOpAdaptor adaptor,
      ConversionPatternRewriter &rewriter) const override {
    return matchAndRewriteDotLikeOp(op, adaptor, rewriter);
  }
};

// This pattern lowers a generic MHLO op for uq->int.
// This pattern essentially just performs type change, with no algorithm change.
class ConvertGenericOp : public ConversionPattern {
 public:
  explicit ConvertGenericOp(MLIRContext *ctx)
      : ConversionPattern(MatchAnyOpTypeTag(), 1, ctx) {}

  LogicalResult matchAndRewrite(
      Operation *op, ArrayRef<Value> operands,
      ConversionPatternRewriter &rewriter) const override {
    // This pattern only handle selected ops.
    if (!llvm::isa<mhlo::ConstantOp, mhlo::ConvertOp, mhlo::BroadcastInDimOp,
                   mhlo::MaxOp, mhlo::MinOp>(op)) {
      return failure();
    }

    // Check that all operands and result uq types are the same.
    llvm::SmallVector<Type> uq_types;
    for (auto result_type : op->getResultTypes()) {
      auto type = getElementTypeOrSelf(result_type)
                      .dyn_cast<quant::UniformQuantizedType>();
      if (type) {
        uq_types.push_back(type);
      }
    }
    for (auto operand : op->getOperands()) {
      auto type = getElementTypeOrSelf(operand.getType())
                      .dyn_cast<quant::UniformQuantizedType>();
      if (type) {
        uq_types.push_back(type);
      }
    }
    for (auto type : uq_types) {
      if (type != uq_types.front()) {
        return failure();
      }
    }

    // Determine new result type: use storage type for uq types; use original
    // type otherwise.
    llvm::SmallVector<Type, 4> new_result_types;
    for (auto result_type : op->getResultTypes()) {
      if (getElementTypeOrSelf(result_type)
              .isa<quant::UniformQuantizedType>()) {
        new_result_types.push_back(result_type.cast<TensorType>().clone(
            getElementTypeOrSelf(result_type)
                .cast<quant::UniformQuantizedType>()
                .getStorageType()));
      } else {
        new_result_types.push_back(result_type);
      }
    }

    OperationState state(op->getLoc(), op->getName().getStringRef(), operands,
                         new_result_types, op->getAttrs(), op->getSuccessors());
    Operation *new_op = rewriter.create(state);
    rewriter.replaceOp(op, new_op);
    return success();
  }
};

// Performs conversion of MHLO quant ops to primitive ops.
void ConvertMHLOQuantToInt::runOnOperation() {
  Operation *op = getOperation();
  MLIRContext *context = op->getContext();
  RewritePatternSet patterns(context);

  // Populate MHLO quant ops conversion patterns.
  patterns.add<ConvertUniformQuantizeOp, ConvertUniformDequantizeOp,
               ConvertUniformQuantizedAddOp, ConvertUniformQuantizedDotOp,
               ConvertUniformQuantizedDotGeneralOp,
               ConvertUniformQuantizedConvolutionOp, ConvertGenericOp>(context);

  ConversionTarget target(*op->getContext());
  // An addDynamicallyLegalDialect callback that declares a given operation as
  // legal only if its all operands and results are non-quantized types.
  auto is_legal = [](Operation *op) {
    auto is_not_quant = [](Type type) {
      return !getElementTypeOrSelf(type).isa<quant::UniformQuantizedType>();
    };
    return llvm::all_of(op->getOperandTypes(), is_not_quant) &&
           llvm::all_of(op->getResultTypes(), is_not_quant);
  };
  target.addDynamicallyLegalDialect<mhlo::MhloDialect>(is_legal);
  target.addDynamicallyLegalDialect<chlo::ChloDialect>(is_legal);

  LogicalResult result =
      applyPartialConversion(op, target, std::move(patterns));
  if (failed(result)) {
    signalPassFailure();
  }

  // Legalize CHLO if needed.
  if (!legalize_chlo_) return;
  RewritePatternSet patterns_2(context);

  chlo::populateDecomposeChloPatterns(context, &patterns_2);
  chlo::populateChloBroadcastingPatterns(context, &patterns_2);

  ConversionTarget target_2 =
      mhlo::GetDefaultLegalConversionTargets(*op->getContext(), legalize_chlo_);

  result = applyPartialConversion(op, target_2, std::move(patterns_2));
  if (failed(result)) {
    signalPassFailure();
  }
}

}  // end namespace

std::unique_ptr<OperationPass<func::FuncOp>> createConvertMHLOQuantToIntPass(
    bool legalize_chlo) {
  return std::make_unique<ConvertMHLOQuantToInt>(legalize_chlo);
}

}  // end namespace stablehlo
}  // end namespace mlir
