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
#include <limits>
#include <memory>
#include <string>
#include <utility>

#include "llvm/Support/Debug.h"
#include "mlir/Dialect/Func/IR/FuncOps.h"  // from @llvm-project
#include "mlir/Dialect/Quant/QuantOps.h"  // from @llvm-project  // NOLINT: Required to register quantization dialect.
#include "mlir/IR/BuiltinAttributes.h"  // from @llvm-project
#include "mlir/IR/BuiltinOps.h"  // from @llvm-project
#include "mlir/IR/MLIRContext.h"  // from @llvm-project
#include "mlir/IR/PatternMatch.h"  // from @llvm-project
#include "mlir/Pass/Pass.h"  // from @llvm-project
#include "mlir/Support/LogicalResult.h"  // from @llvm-project
#include "mlir/Transforms/GreedyPatternRewriteDriver.h"  // from @llvm-project
#include "stablehlo/dialect/StablehloOps.h"  // from @stablehlo
#include "tensorflow/compiler/mlir/lite/ir/tfl_ops.h"

#define DEBUG_TYPE "uniform-quantized-stablehlo-to-tfl"

namespace mlir {
namespace odml {
namespace {

using quant::UniformQuantizedPerAxisType;
using quant::UniformQuantizedType;

#define GEN_PASS_DEF_UNIFORMQUANTIZEDSTABLEHLOTOTFLPASS
#include "tensorflow/compiler/mlir/lite/stablehlo/transforms/passes.h.inc"

class UniformQuantizedStablehloToTflPass
    : public impl::UniformQuantizedStablehloToTflPassBase<
          UniformQuantizedStablehloToTflPass> {
 private:
  void runOnOperation() override;
};

// Determines whether the storage type of a quantized type is supported by
// `tfl.quantize` or `tfl.dequantize` ops. ui8, i8 and i16 are supported.
bool IsSupportedByTfliteQuantizeOrDequantizeOps(IntegerType storage_type) {
  if ((storage_type.isSigned() &&
       !(storage_type.getWidth() == 8 || storage_type.getWidth() == 16)) ||
      (!storage_type.isSigned() && storage_type.getWidth() != 8)) {
    LLVM_DEBUG(llvm::dbgs()
               << "Uniform quantize / dequantize op only supports ui8, i8 or "
                  "i16 for the storage type of uniform quantized type. Got: "
               << storage_type << ".\n");
    return false;
  }
  return true;
}

// Returns true iff the storage type of `quantized_type` is 8-bit integer.
bool IsStorageTypeI8(QuantizedType quantized_type) {
  const Type storage_type = quantized_type.getStorageType();
  return storage_type.isInteger(/*width=*/8);
}

// Returns true iff the expressed type of `quantized_type` is f32.
bool IsExpressedTypeF32(QuantizedType quantized_type) {
  const Type expressed_type = quantized_type.getExpressedType();
  return expressed_type.isa<Float32Type>();
}

// Returns true iff `type` is a uniform quantized type whose storage type is
// 8-bit integer and expressed type is f32.
bool IsI8F32UniformQuantizedType(const Type type) {
  auto quantized_type = type.dyn_cast_or_null<UniformQuantizedType>();
  if (!quantized_type) {
    LLVM_DEBUG(llvm::dbgs()
               << "Expected a uniform quantized type. Got: " << type << ".\n");
    return false;
  }

  if (!IsStorageTypeI8(quantized_type)) {
    LLVM_DEBUG(llvm::dbgs() << "Expected an i8 storage type. Got: "
                            << quantized_type << ".\n");
    return false;
  }

  if (!IsExpressedTypeF32(quantized_type)) {
    LLVM_DEBUG(llvm::dbgs() << "Expected an f32 expressed type. Got: "
                            << quantized_type << ".\n");
    return false;
  }

  return true;
}

// Returns true iff `type` is a uniform quantized per-axis (per-channel) type
// whose storage type is 8-bit integer and expressed type is f32.
bool IsI8F32UniformQuantizedPerAxisType(const Type type) {
  auto quantized_per_axis_type =
      type.dyn_cast_or_null<UniformQuantizedPerAxisType>();
  if (!quantized_per_axis_type) {
    LLVM_DEBUG(llvm::dbgs()
               << "Expected a uniform quantized type. Got: " << type << ".\n");
    return false;
  }

  if (!IsStorageTypeI8(quantized_per_axis_type)) {
    LLVM_DEBUG(llvm::dbgs() << "Expected an i8 storage type. Got: "
                            << quantized_per_axis_type << ".\n");
    return false;
  }

  if (!IsExpressedTypeF32(quantized_per_axis_type)) {
    LLVM_DEBUG(llvm::dbgs() << "Expected an f32 expressed type. Got: "
                            << quantized_per_axis_type << ".\n");
    return false;
  }

  return true;
}

// stablehlo.uniform_quantize -> tfl.quantize
class RewriteUniformQuantizeOp
    : public OpRewritePattern<stablehlo::UniformQuantizeOp> {
  using OpRewritePattern<stablehlo::UniformQuantizeOp>::OpRewritePattern;

  // Determines whether the input and output types are compatible with
  // `tfl.quantize`. See the definition for the `QUANTIZE` kernel for the
  // detailed limitations
  // (https://github.com/tensorflow/tensorflow/blob/8f145d579aa0ee7f4187af32dbbf4e12fdabbffe/tensorflow/lite/kernels/quantize.cc#L105).
  LogicalResult match(stablehlo::UniformQuantizeOp op) const override {
    const Type input_element_type =
        op.getOperand().getType().cast<TensorType>().getElementType();
    if (!input_element_type.isa<FloatType>()) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Uniform quantize op's input should be a float type. Got: "
                 << input_element_type << ".\n");
      return failure();
    }

    // Output type of `UniformQuantizeOp` is guaranteed to be a quantized
    // tensor with integer storage type.
    const auto output_storage_type = op.getResult()
                                         .getType()
                                         .cast<TensorType>()
                                         .getElementType()
                                         .cast<QuantizedType>()
                                         .getStorageType()
                                         .cast<IntegerType>();
    if (!IsSupportedByTfliteQuantizeOrDequantizeOps(output_storage_type)) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Failed to match storage type of output quantized type.\n");
      return failure();
    }

    return success();
  }

  void rewrite(stablehlo::UniformQuantizeOp op,
               PatternRewriter& rewriter) const override {
    Type output_type = *op->getResultTypes().begin();
    rewriter.replaceOpWithNewOp<TFL::QuantizeOp>(
        op, output_type, /*input=*/op.getOperand(),
        /*qtype=*/TypeAttr::get(output_type));
  }
};

// stablehlo.uniform_dequantize -> tfl.dequantize
class RewriteUniformDequantizeOp
    : public OpRewritePattern<stablehlo::UniformDequantizeOp> {
  using OpRewritePattern<stablehlo::UniformDequantizeOp>::OpRewritePattern;

  // Determines whether the input and output types are compatible with
  // `tfl.dequantize`. See the definition for the `DEQUANTIZE` kernel for the
  // detailed limitations
  // (https://github.com/tensorflow/tensorflow/blob/8f145d579aa0ee7f4187af32dbbf4e12fdabbffe/tensorflow/lite/kernels/dequantize.cc#L52).
  LogicalResult match(stablehlo::UniformDequantizeOp op) const override {
    const auto input_storage_type = op.getOperand()
                                        .getType()
                                        .cast<TensorType>()
                                        .getElementType()
                                        .cast<QuantizedType>()
                                        .getStorageType()
                                        .cast<IntegerType>();
    if (!IsSupportedByTfliteQuantizeOrDequantizeOps(input_storage_type)) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Failed to match storage type of input quantized type.\n");
      return failure();
    }

    // Output type is guaranteed to be a float tensor for a valid StableHLO.
    const auto output_element_type = op.getResult()
                                         .getType()
                                         .cast<TensorType>()
                                         .getElementType()
                                         .cast<FloatType>();
    if (!output_element_type.isa<Float32Type>()) {
      LLVM_DEBUG(llvm::dbgs() << "Uniform dequantize op's output element type "
                                 "should be f32. Got: "
                              << output_element_type << ".\n");
      return failure();
    }

    return success();
  }

  void rewrite(stablehlo::UniformDequantizeOp op,
               PatternRewriter& rewriter) const override {
    rewriter.replaceOpWithNewOp<TFL::DequantizeOp>(
        op, /*resultTypes=*/op->getResultTypes(), /*input=*/op.getOperand());
  }
};

// Rewrites `stablehlo.convolution` -> `tfl.conv_2d` when it accepts uniform
// quantized tensors.
//
// Conditions for the conversion:
//   * Input and output tensors are per-tensor uniform quantized (i8->f32)
//     tensors.
//   * The filter tensor is constant a per-channel uniform quantized (i8->f32)
//     tensor.
//   * Convolution is a 2D convolution op and both the input's and filter's
//     shape is 4 dimensional.
//   * The filter tensor's format is `[0, 1, i, o]`.
//   * Not a depthwise convolution.
//   * Does not consider bias add fusion.
class RewriteQuantizedConvolutionOp
    : public OpRewritePattern<stablehlo::ConvolutionOp> {
 public:
  using OpRewritePattern<stablehlo::ConvolutionOp>::OpRewritePattern;

  static LogicalResult MatchInput(Value input) {
    auto input_type = input.getType().cast<TensorType>();
    if (input_type.getRank() != 4) {
      LLVM_DEBUG(llvm::dbgs() << "Only 2D convolution op is supported. "
                                 "Expected input rank of 4. Got: "
                              << input_type.getRank() << ".\n");
      return failure();
    }

    if (const auto input_element_type = input_type.getElementType();
        !IsI8F32UniformQuantizedType(input_element_type)) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Expected an i8->f32 uniform quantized type. Got: "
                 << input_element_type << ".\n");
      return failure();
    }

    return success();
  }

  static LogicalResult MatchFilter(Value filter) {
    auto filter_type = filter.getType().cast<TensorType>();
    if (filter_type.getRank() != 4) {
      LLVM_DEBUG(llvm::dbgs() << "Only 2D convolution op is supported. "
                                 "Expected filter rank of 4. Got: "
                              << filter_type.getRank() << ".\n");
      return failure();
    }

    const Type filter_element_type = filter_type.getElementType();
    if (!IsI8F32UniformQuantizedPerAxisType(filter_type.getElementType())) {
      LLVM_DEBUG(
          llvm::dbgs()
          << "Expected a per-channel uniform quantized (i8->f32) type. Got: "
          << filter_element_type << "\n");
      return failure();
    }

    if (filter_element_type.cast<UniformQuantizedPerAxisType>()
            .getQuantizedDimension() != 3) {
      LLVM_DEBUG(llvm::dbgs() << "Quantized dimension should be 3. Got: "
                              << filter_element_type << "\n");
      return failure();
    }

    if (Operation* filter_op = filter.getDefiningOp();
        filter_op == nullptr || !isa<stablehlo::ConstantOp>(filter_op)) {
      LLVM_DEBUG(llvm::dbgs() << "Filter should be a constant.\n");
      return failure();
    }

    return success();
  }

  static LogicalResult MatchOutput(Value output) {
    const Type output_element_type =
        output.getType().cast<TensorType>().getElementType();
    if (!IsI8F32UniformQuantizedType(output_element_type)) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Expected a uniform quantized (i8->f32) type. Got: "
                 << output_element_type << ".\n");
      return failure();
    }

    return success();
  }

  LogicalResult match(stablehlo::ConvolutionOp op) const override {
    stablehlo::ConvDimensionNumbersAttr dimension_numbers =
        op.getDimensionNumbers();

    const int64_t output_dimension =
        dimension_numbers.getKernelOutputFeatureDimension();
    if (output_dimension != 3) {
      LLVM_DEBUG(llvm::dbgs() << "Expected kernel output feature == 3. Got: "
                              << output_dimension << ".\n");
      return failure();
    }

    const int64_t input_dimension =
        dimension_numbers.getKernelInputFeatureDimension();
    if (input_dimension != 2) {
      LLVM_DEBUG(llvm::dbgs() << "Expected kernel input feature == 2. Got: "
                              << input_dimension << ".\n");
      return failure();
    }

    if (failed(MatchInput(op.getOperand(0)))) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Failed to match input for quantized convolution_op.\n");
      return failure();
    }

    if (failed(MatchFilter(op.getOperand(1)))) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Failed to match filter for quantized convolution_op.\n");
      return failure();
    }

    if (failed(MatchOutput(op.getResult()))) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Failed to match output for quantized convolution_op.\n");
      return failure();
    }

    return success();
  }

  void rewrite(stablehlo::ConvolutionOp op,
               PatternRewriter& rewriter) const override {
    Value filter_value = op.getOperand(1);
    Operation* filter_op = filter_value.getDefiningOp();

    auto filter_uniform_quantized_type =
        filter_value.getType()
            .cast<TensorType>()
            .getElementType()
            .cast<UniformQuantizedPerAxisType>();

    // Create a new quantized tensor type for the filter. This is required
    // because the quantized dimension is changed from 3 -> 0. `TFL::Conv2DOp`
    // requires the quantized dimension to be 0 because it accepts a filter
    // tensor of format OHWI
    // (https://github.com/tensorflow/tensorflow/blob/5430e5e238f868ce977df96ba89c9c1d31fbe8fa/tensorflow/compiler/mlir/lite/ir/tfl_ops.td#L933).
    // The quantized dimension should correspond to the output feature
    // dimension.
    auto new_filter_quantized_type = UniformQuantizedPerAxisType::getChecked(
        filter_op->getLoc(), /*flags=*/true,
        /*storageType=*/filter_uniform_quantized_type.getStorageType(),
        filter_uniform_quantized_type.getExpressedType(),
        filter_uniform_quantized_type.getScales(),
        filter_uniform_quantized_type.getZeroPoints(),
        /*quantizedDimension=*/0,
        filter_uniform_quantized_type.getStorageTypeMin(),
        filter_uniform_quantized_type.getStorageTypeMax());

    auto filter_constant_value_attr = cast<DenseIntElementsAttr>(
        cast<stablehlo::ConstantOp>(filter_value.getDefiningOp()).getValue());

    // Using TransposeOp doesn't work because the quantized dimension
    // changes which violates the constraint for the TransposeOp that the
    // input's and output's element type should be the same.
    const DenseIntElementsAttr new_filter_value_attr = TransposeFilterValue(
        filter_op->getLoc(), rewriter, filter_constant_value_attr);

    auto new_filter_result_type = RankedTensorType::getChecked(
        filter_op->getLoc(),
        /*shape=*/new_filter_value_attr.getShapedType().getShape(),
        /*type=*/new_filter_quantized_type);

    auto new_filter_constant_op = rewriter.create<TFL::QConstOp>(
        filter_op->getLoc(), /*output=*/TypeAttr::get(new_filter_result_type),
        new_filter_value_attr);

    // Create a bias filled with zeros. Mimics the behavior of no bias add.
    const int64_t num_output_features = new_filter_result_type.getShape()[0];
    const SmallVector<int64_t, 1> bias_shape = {num_output_features};
    auto bias_quantized_type = UniformQuantizedPerAxisType::getChecked(
        op.getLoc(), /*flags=*/true,
        /*storageType=*/rewriter.getI32Type(),  // i32 for bias
        /*expressedType=*/rewriter.getF32Type(),
        // TODO: b/292886169 - Set this to be s1 * s2.
        /*scales=*/new_filter_quantized_type.getScales(),
        /*zeroPoints=*/new_filter_quantized_type.getZeroPoints(),
        /*quantizedDimension=*/0,
        /*storageTypeMin=*/std::numeric_limits<int32_t>::min(),
        /*storageTypeMax=*/std::numeric_limits<int32_t>::max());
    auto bias_type = RankedTensorType::getChecked(op.getLoc(), bias_shape,
                                                  bias_quantized_type);

    // Create a bias constant. It should have values of 0.
    auto bias_value_type = RankedTensorType::getChecked(op.getLoc(), bias_shape,
                                                        rewriter.getI32Type());
    auto bias_value = DenseIntElementsAttr::get(
        bias_value_type, APInt(/*numBits=*/32, /*value=*/0, /*isSigned=*/true));
    auto bias = rewriter.create<TFL::QConstOp>(
        op.getLoc(), /*output=*/TypeAttr::get(bias_type),
        /*value=*/bias_value);

    // Determine the attributes for the TFL::Conv2DOp.
    const std::string padding = GetPadding(op);
    const auto [stride_h, stride_w] = GetStrides(op);
    const auto [dilation_h_factor, dilation_w_factor] = GetDilationFactors(op);

    Value input_value = op.getOperand(0);
    auto tfl_conv2d_op = rewriter.create<TFL::Conv2DOp>(
        op.getLoc(), /*output=*/op.getResult().getType(),
        /*input=*/input_value,
        /*filter=*/new_filter_constant_op, /*bias=*/bias.getResult(),
        /*dilation_h_factor=*/rewriter.getI32IntegerAttr(dilation_h_factor),
        /*dilation_w_factor=*/rewriter.getI32IntegerAttr(dilation_w_factor),
        /*fused_activation_function=*/rewriter.getStringAttr("NONE"),
        /*padding=*/rewriter.getStringAttr(padding),
        /*stride_h=*/rewriter.getI32IntegerAttr(stride_h),
        /*stride_w=*/rewriter.getI32IntegerAttr(stride_w));

    rewriter.replaceAllUsesWith(op.getResult(), tfl_conv2d_op.getResult());
    rewriter.eraseOp(op);
  }

 private:
  // Transposes the filter tensor to match the filter tensor format for
  // `tfl.conv_2d`. This function performs the following index permutation
  // only: (3, 0, 1, 2). The filter value is assumed to be of `[0, 1, i, o]`
  // format. The `tfl.conv_2d` accepts the filter of `[o, 0, 1, i]`.
  // TODO: b/291598373 - Lift the assumption about the filter tensor's format
  // and generalize the transpose.
  DenseIntElementsAttr TransposeFilterValue(
      Location loc, PatternRewriter& rewriter,
      const DenseIntElementsAttr& filter_value_attr) const {
    ArrayRef<int64_t> filter_shape =
        filter_value_attr.getShapedType().getShape();
    SmallVector<int8_t> filter_constant_values;
    for (const auto filter_val : filter_value_attr.getValues<int8_t>()) {
      filter_constant_values.push_back(filter_val);
    }

    SmallVector<int8_t> new_filter_constant_values(
        filter_constant_values.size(), 0);

    SmallVector<int64_t> new_filter_shape;
    SmallVector<int64_t, 4> transpose_dims = {3, 0, 1, 2};
    for (int i = 0; i < filter_shape.size(); ++i) {
      new_filter_shape.push_back(filter_shape[transpose_dims[i]]);
    }

    auto get_array_idx = [](ArrayRef<int64_t> shape, const int i, const int j,
                            const int k, const int l) -> int64_t {
      return (i * shape[1] * shape[2] * shape[3]) + (j * shape[2] * shape[3]) +
             (k * shape[3]) + l;
    };

    // Transpose the filter value.
    for (int i = 0; i < filter_shape[0]; ++i) {
      for (int j = 0; j < filter_shape[1]; ++j) {
        for (int k = 0; k < filter_shape[2]; ++k) {
          for (int l = 0; l < filter_shape[3]; ++l) {
            // [i][j][k][l] -> [l][i][j][k]
            const int old_idx = get_array_idx(filter_shape, i, j, k, l);
            const int new_idx = get_array_idx(new_filter_shape, l, i, j, k);

            new_filter_constant_values[new_idx] =
                filter_constant_values[old_idx];
          }
        }
      }
    }

    // Create the new filter constant.
    auto new_filter_value_attr_type =
        RankedTensorType::getChecked(loc, new_filter_shape,
                                     /*elementType=*/rewriter.getI8Type());
    auto new_filter_constant_value_attr = DenseIntElementsAttr::get(
        new_filter_value_attr_type, new_filter_constant_values);

    return new_filter_constant_value_attr;
  }

  // Returns the padding attribute used for tfl.conv_2d derived by the padding
  // attribute of `op`.
  // TODO: b/291599812 - Validate the values for "SAME" padding.
  std::string GetPadding(stablehlo::ConvolutionOp op) const {
    const DenseIntElementsAttr padding_attr = op.getPaddingAttr();
    if (!padding_attr) {
      return "VALID";
    }
    if (padding_attr.isSplat() && padding_attr.getSplatValue<int64_t>() == 0) {
      return "VALID";
    }
    return "SAME";
  }

  // Returns the stride amount for the height and width, respectively.
  std::pair<int64_t, int64_t> GetStrides(stablehlo::ConvolutionOp op) const {
    const DenseIntElementsAttr window_strides_attr = op.getWindowStridesAttr();
    if (!window_strides_attr) {
      return {1, 1};  // Default values.
    }

    const auto window_strides_attr_value =
        window_strides_attr.getValues<int64_t>();
    // It is guaranteed from the spec that it has two values:
    // https://github.com/openxla/stablehlo/blob/main/docs/spec.md#convolution.
    return {window_strides_attr_value[0], window_strides_attr_value[1]};
  }

  // Returns the dilation amount for the height and width, respectively.
  std::pair<int64_t, int64_t> GetDilationFactors(
      stablehlo::ConvolutionOp op) const {
    const DenseIntElementsAttr lhs_dilation_attr = op.getLhsDilationAttr();
    if (!lhs_dilation_attr) {
      return {1, 1};  // Default values.
    }

    const auto lhs_dilation_attr_value = lhs_dilation_attr.getValues<int64_t>();
    // It is guaranteed from the spec that it has two values:
    // https://github.com/openxla/stablehlo/blob/main/docs/spec.md#convolution.
    return {lhs_dilation_attr_value[0], lhs_dilation_attr_value[1]};
  }
};

void UniformQuantizedStablehloToTflPass::runOnOperation() {
  func::FuncOp func_op = getOperation();
  MLIRContext& ctx = getContext();

  RewritePatternSet patterns(&ctx);
  patterns.add<RewriteUniformQuantizeOp, RewriteUniformDequantizeOp,
               RewriteQuantizedConvolutionOp>(&ctx);

  if (failed(applyPatternsAndFoldGreedily(func_op, std::move(patterns)))) {
    func_op.emitError() << "Failed to convert stablehlo ops with uniform "
                           "quantized types to tflite ops.";
    signalPassFailure();
  }
}

}  // namespace

std::unique_ptr<OperationPass<func::FuncOp>>
CreateUniformQuantizedStablehloToTflPass() {
  return std::make_unique<UniformQuantizedStablehloToTflPass>();
}

static PassRegistration<UniformQuantizedStablehloToTflPass> pass;

}  // namespace odml
}  // namespace mlir
