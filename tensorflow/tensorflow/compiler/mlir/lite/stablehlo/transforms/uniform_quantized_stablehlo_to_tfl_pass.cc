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
#include <cstdint>
#include <iterator>
#include <memory>
#include <utility>

#include "absl/algorithm/container.h"
#include "absl/log/check.h"
#include "llvm/ADT/STLExtras.h"
#include "llvm/Support/Debug.h"
#include "mlir/Dialect/Arith/IR/Arith.h"  // from @llvm-project
#include "mlir/Dialect/Func/IR/FuncOps.h"  // from @llvm-project
#include "mlir/Dialect/Quant/QuantOps.h"  // from @llvm-project  // NOLINT: Required to register quantization dialect.
#include "mlir/Dialect/Quant/QuantTypes.h"  // from @llvm-project
#include "mlir/IR/Attributes.h"  // from @llvm-project
#include "mlir/IR/BuiltinAttributes.h"  // from @llvm-project
#include "mlir/IR/BuiltinTypeInterfaces.h"  // from @llvm-project
#include "mlir/IR/BuiltinTypes.h"  // from @llvm-project
#include "mlir/IR/MLIRContext.h"  // from @llvm-project
#include "mlir/IR/PatternMatch.h"  // from @llvm-project
#include "mlir/IR/Types.h"  // from @llvm-project
#include "mlir/IR/Value.h"  // from @llvm-project
#include "mlir/Pass/Pass.h"  // from @llvm-project
#include "mlir/Pass/PassRegistry.h"  // from @llvm-project
#include "mlir/Support/LLVM.h"  // from @llvm-project
#include "mlir/Support/LogicalResult.h"  // from @llvm-project
#include "mlir/Transforms/GreedyPatternRewriteDriver.h"  // from @llvm-project
#include "stablehlo/dialect/Base.h"  // from @stablehlo
#include "stablehlo/dialect/StablehloOps.h"  // from @stablehlo
#include "tensorflow/compiler/mlir/lite/ir/tfl_ops.h"
#include "tensorflow/compiler/mlir/quantization/common/attrs_and_constraints.h"
#include "tensorflow/compiler/mlir/quantization/common/uniform_quantized_types.h"

#define DEBUG_TYPE "uniform-quantized-stablehlo-to-tfl"

namespace mlir {
namespace odml {
namespace {

// TODO: b/311029361: Add e2e test for verifying this legalization once
// StableHLO Quantizer API migration is complete.

using ::mlir::quant::CastI64ArrayToI32;
using ::mlir::quant::CastI64ToI32;
using ::mlir::quant::CreateI32F32UniformQuantizedPerAxisType;
using ::mlir::quant::CreateI32F32UniformQuantizedType;
using ::mlir::quant::CreateI8F32UniformQuantizedPerAxisType;
using ::mlir::quant::CreateI8F32UniformQuantizedType;
using ::mlir::quant::IsI32F32UniformQuantizedType;
using ::mlir::quant::IsI8F32UniformQuantizedPerAxisType;
using ::mlir::quant::IsI8F32UniformQuantizedType;
using ::mlir::quant::IsOpFullyQuantized;
using ::mlir::quant::IsQuantizedTensorType;
using ::mlir::quant::IsSupportedByTfliteQuantizeOrDequantizeOps;
using ::mlir::quant::QuantizedType;
using ::mlir::quant::UniformQuantizedPerAxisType;
using ::mlir::quant::UniformQuantizedType;

#define GEN_PASS_DEF_UNIFORMQUANTIZEDSTABLEHLOTOTFLPASS
#include "tensorflow/compiler/mlir/lite/stablehlo/transforms/passes.h.inc"

class UniformQuantizedStablehloToTflPass
    : public impl::UniformQuantizedStablehloToTflPassBase<
          UniformQuantizedStablehloToTflPass> {
 private:
  void runOnOperation() override;
};

// Bias scales for matmul-like ops should be input scale * filter scale. Here it
// is assumed that the input is per-tensor quantized and filter is per-channel
// quantized.
SmallVector<double> GetBiasScales(const double input_scale,
                                  const ArrayRef<double> filter_scales) {
  SmallVector<double> bias_scales;
  absl::c_transform(filter_scales, std::back_inserter(bias_scales),
                    [input_scale](const double filter_scale) -> double {
                      return filter_scale * input_scale;
                    });
  return bias_scales;
}

// Returns a bias scale for matmul-like ops. Here it is assumed that both input
// and filter are per-tensor quantized.
double GetBiasScale(const double input_scale, const double filter_scale) {
  return filter_scale * input_scale;
}

// Creates a new `tfl.qconst` op for the quantized filter. Transposes the
// filter value from [i, o] -> [o, i]. This is because we assume `[i, o]`
// format for `stablehlo.dot_general` (i.e. contracting dimension == 1)
// whereas `tfl.fully_connected` accepts an OI format.
TFL::QConstOp CreateTflConstOpForFilter(
    stablehlo::ConstantOp filter_constant_op, PatternRewriter& rewriter,
    bool is_per_axis) {
  const auto filter_values = filter_constant_op.getValue()
                                 .cast<DenseIntElementsAttr>()
                                 .getValues<int8_t>();

  ArrayRef<int64_t> filter_shape =
      filter_constant_op.getType().cast<TensorType>().getShape();

  // Reverse the shapes. This makes sense, assuming that the filter tensor has a
  // rank of 2 (no batch dimension).
  SmallVector<int64_t, 2> new_filter_shape(filter_shape.rbegin(),
                                           filter_shape.rend());

  // Construct the value array of transposed filter. Assumes 2D matrix.
  SmallVector<int8_t> new_filter_values(filter_values.size(), /*Value=*/0);
  for (int i = 0; i < filter_shape[0]; ++i) {
    for (int j = 0; j < filter_shape[1]; ++j) {
      const int old_idx = i * filter_shape[1] + j;
      const int new_idx = j * filter_shape[0] + i;
      new_filter_values[new_idx] = filter_values[old_idx];
    }
  }

  auto new_filter_value_attr_type = RankedTensorType::getChecked(
      filter_constant_op.getLoc(), new_filter_shape,
      /*elementType=*/rewriter.getI8Type());

  Type new_filter_quantized_type;

  if (is_per_axis) {
    auto filter_quantized_type = filter_constant_op.getResult()
                                     .getType()
                                     .cast<TensorType>()
                                     .getElementType()
                                     .cast<UniformQuantizedPerAxisType>();
    new_filter_quantized_type = CreateI8F32UniformQuantizedPerAxisType(
        filter_constant_op->getLoc(), *rewriter.getContext(),
        filter_quantized_type.getScales(),
        filter_quantized_type.getZeroPoints(),
        /*quantization_dimension=*/0, /*narrow_range=*/true);
  } else {
    auto filter_quantized_type = filter_constant_op.getResult()
                                     .getType()
                                     .cast<TensorType>()
                                     .getElementType()
                                     .cast<UniformQuantizedType>();
    new_filter_quantized_type = CreateI8F32UniformQuantizedType(
        filter_constant_op->getLoc(), *rewriter.getContext(),
        filter_quantized_type.getScale(), filter_quantized_type.getZeroPoint(),
        /*narrow_range=*/true);
  }

  // Required because the quantized dimension is changed from 3 -> 0.
  auto new_filter_result_type = RankedTensorType::getChecked(
      filter_constant_op.getLoc(), /*shape=*/new_filter_shape,
      /*type=*/new_filter_quantized_type);

  auto new_filter_constant_value_attr =
      DenseIntElementsAttr::get(new_filter_value_attr_type, new_filter_values);
  return rewriter.create<TFL::QConstOp>(
      filter_constant_op.getLoc(),
      /*output=*/TypeAttr::get(new_filter_result_type),
      /*value=*/new_filter_constant_value_attr);
}

// Creates a new `tfl.qconst` op for the bias. The bias values are 0s, because
// this bias a dummy bias (note that bias fusion is not considered for this
// transformation). The quantization scale for the bias is input scale *
// filter scale. `filter_const_op` is used to retrieve the filter scales and
// the size of the bias constant.
// TODO - b/309896242: Support bias fusion legalization.
TFL::QConstOp CreateTflConstOpForDummyBias(const Location loc,
                                           const double input_scale,
                                           TFL::QConstOp filter_const_op,
                                           PatternRewriter& rewriter,
                                           bool is_per_axis, MLIRContext& ctx) {
  const ArrayRef<int64_t> filter_shape =
      filter_const_op.getResult().getType().getShape();

  Type bias_quantized_type;
  if (is_per_axis) {
    const auto filter_quantized_element_type =
        filter_const_op.getResult()
            .getType()
            .getElementType()
            .cast<UniformQuantizedPerAxisType>();

    // The storage type is i32 for bias, which is the precision used for
    // accumulation.
    bias_quantized_type = CreateI32F32UniformQuantizedPerAxisType(
        loc, ctx,
        GetBiasScales(input_scale, filter_quantized_element_type.getScales()),
        filter_quantized_element_type.getZeroPoints(),
        /*quantization_dimension=*/0);
  } else {
    const auto filter_quantized_element_type =
        filter_const_op.getResult()
            .getType()
            .getElementType()
            .cast<UniformQuantizedType>();

    // The storage type is i32 for bias, which is the precision used for
    // accumulation.
    bias_quantized_type = CreateI32F32UniformQuantizedType(
        loc, ctx,
        GetBiasScale(input_scale, filter_quantized_element_type.getScale()),
        filter_quantized_element_type.getZeroPoint());
  }

  SmallVector<int64_t, 1> bias_shape = {filter_shape[0]};
  auto bias_type =
      RankedTensorType::getChecked(loc, bias_shape, bias_quantized_type);

  auto bias_value_type = RankedTensorType::getChecked(
      loc, std::move(bias_shape), rewriter.getI32Type());
  auto bias_value = DenseIntElementsAttr::get(
      bias_value_type, APInt(/*numBits=*/32, /*value=*/0, /*isSigned=*/true));

  return rewriter.create<TFL::QConstOp>(
      loc, /*output=*/TypeAttr::get(bias_type), /*value=*/bias_value);
}

// stablehlo.uniform_quantize -> tfl.quantize
// TODO: b/322428814 - Add StableHLO quantizer integration tests for ODML.
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
    if (!(input_element_type.isa<FloatType>() ||
          IsI32F32UniformQuantizedType(input_element_type))) {
      LLVM_DEBUG(llvm::dbgs() << "Uniform quantize op's input should be a "
                                 "float type or int32. Got: "
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
// TODO: b/294771704 - Support bias quantization.
// TODO: b/322428814 - Add StableHLO quantizer integration tests for ODML.
class RewriteUpstreamQuantizedConvolutionOp
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
    auto new_filter_quantized_type = CreateI8F32UniformQuantizedPerAxisType(
        filter_op->getLoc(), *op.getContext(),
        filter_uniform_quantized_type.getScales(),
        filter_uniform_quantized_type.getZeroPoints(),
        /*quantization_dimension=*/0, /*narrow_range=*/true);

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

    SmallVector<double> bias_scales =
        GetBiasScales(/*input_scale=*/op.getOperand(0)
                          .getType()
                          .cast<TensorType>()
                          .getElementType()
                          .cast<UniformQuantizedType>()
                          .getScale(),
                      /*filter_scales=*/new_filter_quantized_type.getScales());

    // Create a bias filled with zeros. Mimics the behavior of no bias add.
    const int64_t num_output_features = new_filter_result_type.getShape()[0];
    const SmallVector<int64_t, 1> bias_shape = {num_output_features};
    auto bias_quantized_type = CreateI32F32UniformQuantizedPerAxisType(
        op.getLoc(), *op.getContext(), std::move(bias_scales),
        new_filter_quantized_type.getZeroPoints(),
        /*quantization_dimension=*/0);
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
    // TODO: b/294808863 - Use `padding = "SAME"` if the padding attribute
    // matches the semantics.
    Value input_value = op.getOperand(0);
    if (const DenseIntElementsAttr padding_attr = op.getPaddingAttr();
        !IsPaddingValid(padding_attr)) {
      // Add an extra tfl.pad_op if there are explicit padding values. This
      // extra pad op will allow us to always set the `padding` attribute of the
      // newly created tfl.conv_2d op as "VALID".
      TFL::PadOp pad_op =
          CreateTflPadOp(op.getLoc(), padding_attr, input_value, rewriter);
      input_value = pad_op.getResult();
    }

    const auto [stride_h, stride_w] = GetStrides(op);
    const auto [dilation_h_factor, dilation_w_factor] = GetDilationFactors(op);

    auto tfl_conv2d_op = rewriter.create<TFL::Conv2DOp>(
        op.getLoc(), /*output=*/op.getResult().getType(), /*input=*/input_value,
        /*filter=*/new_filter_constant_op, /*bias=*/bias.getResult(),
        /*dilation_h_factor=*/rewriter.getI32IntegerAttr(dilation_h_factor),
        /*dilation_w_factor=*/rewriter.getI32IntegerAttr(dilation_w_factor),
        /*fused_activation_function=*/rewriter.getStringAttr("NONE"),
        /*padding=*/rewriter.getStringAttr("VALID"),
        /*stride_h=*/rewriter.getI32IntegerAttr(stride_h),
        /*stride_w=*/rewriter.getI32IntegerAttr(stride_w));

    rewriter.replaceAllUsesWith(op.getResult(), tfl_conv2d_op.getResult());
    rewriter.eraseOp(op);
  }

 private:
  // Create a `tfl.pad` op to apply explicit padding to the input tensor that
  // correspond to the `padding` attribute from the `stablehlo.convolution` op.
  TFL::PadOp CreateTflPadOp(Location loc,
                            const DenseIntElementsAttr& padding_attr,
                            Value input_value,
                            PatternRewriter& rewriter) const {
    auto padding_values = padding_attr.getValues<int64_t>();
    // [[h_l, h_r], [w_l, w_r]].
    DCHECK_EQ(padding_attr.size(), 4);

    // In StableHLO the padding attribute doesn't include the padding values for
    // input and output feature dimensions (because they are 0 anyways). In
    // TFLite, padding values for input and output feature dimensions should be
    // explicitly set to 0s. Note that TFLite's input tensor is formatted as
    // OHWI. The resulting pad values becomes: [[0, 0], [h_l, h_r], [w_l, w_r],
    // [0, 0]]
    SmallVector<int32_t, 8> tfl_pad_values = {0, 0};  // For output feature dim.
    for (const int64_t padding_value : padding_values) {
      tfl_pad_values.push_back(CastI64ToI32(padding_value).value());
    }
    // For input feature dim.
    tfl_pad_values.push_back(0);
    tfl_pad_values.push_back(0);

    const auto input_tensor_type =
        input_value.getType().cast<RankedTensorType>();
    const int64_t rank = input_tensor_type.getRank();

    SmallVector<int64_t> padded_output_tensor_shape =
        InferPaddedTensorShape(input_tensor_type.getShape(), tfl_pad_values);

    auto padded_output_tensor_type = RankedTensorType::get(
        padded_output_tensor_shape, input_tensor_type.getElementType());

    // The pad values is provided as a const op.
    auto pad_value_const_op = rewriter.create<TFL::ConstOp>(
        loc, /*value=*/DenseIntElementsAttr::get(
            RankedTensorType::get({rank, 2}, rewriter.getIntegerType(32)),
            tfl_pad_values));

    return rewriter.create<TFL::PadOp>(
        loc, /*output=*/padded_output_tensor_type, input_value,
        /*padding=*/pad_value_const_op.getResult());
  }

  // Infers the output tensor's shape after padding `tfl_pad_values` to the
  // `tensor_shape`. `tfl_pad_values` should be formatted as `[[l_0, r_0], [l_1,
  // r_1], ..., [l_n, r_n]]`, where `l_x` and `r_x` are the left and paddings
  // for the x-th dimension, respectively.
  SmallVector<int64_t> InferPaddedTensorShape(
      const ArrayRef<int64_t> tensor_shape,
      const ArrayRef<int32_t> tfl_pad_values) const {
    SmallVector<int64_t> padded_shape(tensor_shape.begin(), tensor_shape.end());
    for (int i = 0; i < padded_shape.size(); ++i) {
      // Left padding + right padding.
      const int32_t padded = tfl_pad_values[i * 2] + tfl_pad_values[i * 2 + 1];
      padded_shape[i] += padded;
    }

    return padded_shape;
  }

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

  // Determines if the padding attribute corresponds to "VALID"
  // (https://www.tensorflow.org/api_docs/python/tf/nn).
  bool IsPaddingValid(const DenseIntElementsAttr& padding_attr) const {
    // If padding_attr is empty, it defaults to splat 0s.
    return !padding_attr || (padding_attr.isSplat() &&
                             padding_attr.getSplatValue<int64_t>() == 0);
  }

  // Returns the stride amount for the height and width, respectively.
  std::pair<int64_t, int64_t> GetStrides(stablehlo::ConvolutionOp op) const {
    const Attribute window_strides_attr = op.getWindowStridesAttr();
    if (!window_strides_attr) {
      return {1, 1};  // Default values.
    }

    const auto window_strides_attr_value =
        hlo::getI64Array(window_strides_attr);
    // It is guaranteed from the spec that it has two values:
    // https://github.com/openxla/stablehlo/blob/main/docs/spec.md#convolution.
    return {window_strides_attr_value[0], window_strides_attr_value[1]};
  }

  // Returns the dilation amount for the height and width, respectively.
  std::pair<int64_t, int64_t> GetDilationFactors(
      stablehlo::ConvolutionOp op) const {
    const Attribute lhs_dilation_attr = op.getLhsDilationAttr();
    if (!lhs_dilation_attr) {
      return {1, 1};  // Default values.
    }

    const auto lhs_dilation_attr_value = hlo::getI64Array(lhs_dilation_attr);
    // It is guaranteed from the spec that it has two values:
    // https://github.com/openxla/stablehlo/blob/main/docs/spec.md#convolution.
    return {lhs_dilation_attr_value[0], lhs_dilation_attr_value[1]};
  }
};

// Rewrites full-integer quantized `stablehlo.dot_general` ->`tfl.batch_matmul`
// when it accepts uniform quantized tensors.
//
// Since transpose and reshape of quantized tensors are not natively supported
// at the moment, the conversion condition is relatively strict, following
// (https://www.tensorflow.org/api_docs/cc/class/tensorflow/ops/batch-mat-mul-v3)
//
// Conditions for the conversion :
//   * size(batching_dimensions) <= 3 (TFLite support restriction)
//   * size(contracting_dimensions) = 1
//   * Input (lhs) and output tensors are per-tensor uniform quantized (i8->f32)
//     tensors (full integer) with shape [..., r_x, c_x] or [..., c_x, r_x].
//   * The rhs tensor is a per-tensor uniform quantized (i8->f32) tensor
//     (constant or activation) with shape [..., r_y, c_y] or [..., c_y, r_y].
//
// TODO: b/293650675 - Relax the conversion condition to support dot_general in
// general.
// TODO: b/322428814 - Add StableHLO quantizer integration tests for ODML.
class RewriteUpstreamQuantizedDotGeneralOpToBatchMatmulOp
    : public OpRewritePattern<stablehlo::DotGeneralOp> {
 public:
  using OpRewritePattern<stablehlo::DotGeneralOp>::OpRewritePattern;

  static LogicalResult MatchLhs(
      Value lhs, stablehlo::DotDimensionNumbersAttr dimension_numbers) {
    auto lhs_type = lhs.getType().cast<TensorType>();
    if (!IsI8F32UniformQuantizedType(lhs_type.getElementType())) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Expected a per-tensor uniform "
                    "quantized (i8->f32) input for dot_general. Got: "
                 << lhs_type << "\n");
      return failure();
    }
    if (!lhs_type.hasRank()) {
      LLVM_DEBUG(llvm::dbgs() << "Expected lhs of dot_general has rank. Got: "
                              << lhs_type << "\n");
      return failure();
    }
    const int lhs_rank = lhs_type.getRank();
    auto lhs_contracting_dim =
        dimension_numbers.getLhsContractingDimensions()[0];
    if ((lhs_contracting_dim != lhs_rank - 1) &&
        (lhs_contracting_dim != lhs_rank - 2)) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Not supported lhs contracting dim for dot_general.\n");
      return failure();
    }
    return success();
  }

  static LogicalResult MatchRhs(
      Value rhs, stablehlo::DotDimensionNumbersAttr dimension_numbers) {
    if (!rhs.getType().cast<TensorType>().hasRank()) {
      LLVM_DEBUG(llvm::dbgs() << "Expected rhs of dot_general has rank. Got: "
                              << rhs.getType() << "\n");
      return failure();
    }
    const int rhs_rank = rhs.getType().cast<TensorType>().getRank();
    auto rhs_contracting_dim =
        dimension_numbers.getRhsContractingDimensions()[0];
    if ((rhs_contracting_dim != rhs_rank - 1) &&
        (rhs_contracting_dim != rhs_rank - 2)) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Not supported rhs contracting dim for dot_general.\n");
      return failure();
    }

    auto rhs_type = rhs.getType().cast<TensorType>();
    if (!IsI8F32UniformQuantizedType(rhs_type.getElementType())) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Expected a per-tensor uniform "
                    "quantized (i8->f32) weight for dot_general. Got: "
                 << rhs_type << "\n");
      return failure();
    }
    return success();
  }

  static LogicalResult MatchOutput(
      Value output, stablehlo::DotDimensionNumbersAttr dimension_numbers) {
    auto output_type = output.getType().cast<TensorType>();
    if (!IsI8F32UniformQuantizedType(output_type.getElementType())) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Expected a per-tensor uniform "
                    "quantized (i8->f32) output for dot_general. Got: "
                 << output_type << "\n");
      return failure();
    }
    return success();
  }

  LogicalResult match(stablehlo::DotGeneralOp op) const override {
    stablehlo::DotDimensionNumbersAttr dimension_numbers =
        op.getDotDimensionNumbers();

    // Check one side is enough since
    // (C1) size(lhs_batching_dimensions) = size(rhs_batching_dimensions).
    if (dimension_numbers.getLhsBatchingDimensions().size() > 3) {
      LLVM_DEBUG(
          llvm::dbgs()
          << "Failed to match batch dimention for quantized dot_general.\n");
      return failure();
    }
    // Check one side is enough since
    // (C2) size(lhs_contracting_dimensions) = size(rhs_contracting_dimensions).
    if (dimension_numbers.getLhsContractingDimensions().size() != 1) {
      LLVM_DEBUG(
          llvm::dbgs()
          << "Failed to match contract dimention for quantized dot_general.\n");
      return failure();
    }

    if (failed(MatchLhs(op.getLhs(), dimension_numbers))) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Failed to match input for quantized dot_general.\n");
      return failure();
    }
    if (failed(MatchRhs(op.getRhs(), dimension_numbers))) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Failed to match weight for quantized dot_general.\n");
      return failure();
    }

    if (failed(MatchOutput(op.getResult(), dimension_numbers))) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Failed to match output for quantized dot_general.\n");
      return failure();
    }

    return success();
  }

  void rewrite(stablehlo::DotGeneralOp op,
               PatternRewriter& rewriter) const override {
    Value rhs_value = op.getRhs();
    Operation* rhs_op = rhs_value.getDefiningOp();

    stablehlo::DotDimensionNumbersAttr dimension_numbers =
        op.getDotDimensionNumbers();
    Value input_value = op.getLhs();
    const int lhs_rank = input_value.getType().cast<TensorType>().getRank();
    auto lhs_contracting_dim =
        dimension_numbers.getLhsContractingDimensions()[0];
    BoolAttr adj_x =
        (lhs_contracting_dim == lhs_rank - 2 ? rewriter.getBoolAttr(true)
                                             : rewriter.getBoolAttr(false));
    auto rhs_contracting_dim =
        dimension_numbers.getRhsContractingDimensions()[0];
    const int rhs_rank = rhs_value.getType().cast<TensorType>().getRank();
    BoolAttr adj_y =
        (rhs_contracting_dim == rhs_rank - 1 ? rewriter.getBoolAttr(true)
                                             : rewriter.getBoolAttr(false));

    // Set to `nullptr` because this attribute only matters when the input is
    // dynamic-range quantized.
    BoolAttr asymmetric_quantize_inputs = nullptr;

    // Create BMM assuming rhs is activation.
    auto tfl_batchmatmul_op = rewriter.create<TFL::BatchMatMulOp>(
        op.getLoc(), /*output=*/op.getResult().getType(), /*input=*/input_value,
        /*filter=*/rhs_value, adj_x, adj_y, asymmetric_quantize_inputs);

    // Update BMM if rhs is a constant.
    auto const_rhs = dyn_cast_or_null<stablehlo::ConstantOp>(rhs_op);
    if (const_rhs) {
      auto rhs_uniform_quantized_type = rhs_value.getType().cast<ShapedType>();
      auto rhs_constant_value_attr =
          cast<DenseIntElementsAttr>(const_rhs.getValue());
      auto rhs_constant_op = rewriter.create<TFL::QConstOp>(
          rhs_op->getLoc(),
          /*output=*/TypeAttr::get(rhs_uniform_quantized_type),
          rhs_constant_value_attr);
      tfl_batchmatmul_op = rewriter.create<TFL::BatchMatMulOp>(
          op.getLoc(), /*output=*/op.getResult().getType(),
          /*input=*/input_value, /*filter=*/rhs_constant_op.getResult(), adj_x,
          adj_y, asymmetric_quantize_inputs);
    }

    rewriter.replaceAllUsesWith(op.getResult(), tfl_batchmatmul_op.getResult());
  }
};

// Rewrites `stablehlo.dot_general` -> `tfl.fully_connected` when it accepts
// uniform quantized tensors with per-axis quantized filter tensor (rhs).
//
// Conditions for the conversion:
//   * Input and output tensors are per-tensor uniform quantized (i8->f32)
//     tensors.
//   * The filter tensor is constant a per-channel uniform quantized (i8->f32)
//     tensor. The quantization dimension should be 1 (the non-contracting
//     dimension).
//   * The input tensor's rank is either 2 or 3. The last dimension of the input
//     tensor should be the contracting dimension, i.e. [..., c_x, r_x].
//   * The filter tensor's rank is 2. The contracting dimension should be the
//     first dimension (dim 0), i.e. [c_y, r_y] where c_y == r_x.
//   * Does not consider activation fusion.
//   * Does not consider bias add fusion.
//
// TODO: b/294983811 - Merge this pattern into
// `RewriteFullIntegerQuantizedDotGeneralOp`.
// TODO: b/295264927 - `stablehlo.dot_general` with per-axis quantized operands
// is not specified in the StableHLO dialect. Update the spec to allow this.
// TODO: b/322428814 - Add StableHLO quantizer integration tests for ODML.
class RewriteUpstreamQuantizedDotGeneralOpToTflFullyConnectedOp
    : public OpRewritePattern<stablehlo::DotGeneralOp> {
  using OpRewritePattern<stablehlo::DotGeneralOp>::OpRewritePattern;

 public:
  LogicalResult match(stablehlo::DotGeneralOp op) const override {
    const stablehlo::DotDimensionNumbersAttr dot_dimension_nums =
        op.getDotDimensionNumbers();
    if (const int num_rhs_contracting_dims =
            dot_dimension_nums.getRhsContractingDimensions().size();
        num_rhs_contracting_dims != 1) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Expected number of contracting dimensions to be 1. Got: "
                 << num_rhs_contracting_dims << ".\n");
      return failure();
    }

    if (failed(MatchInput(op.getOperand(0)))) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Failed to match input for quantized dot_general op.\n");
      return failure();
    }

    if (failed(MatchFilter(op.getOperand(1)))) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Failed to match filter for quantized dot_general op.\n");
      return failure();
    }

    if (failed(MatchOutput(op.getResult()))) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Failed to match output for quantized dot_general op.\n");
      return failure();
    }

    return success();
  }

  void rewrite(stablehlo::DotGeneralOp op,
               PatternRewriter& rewriter) const override {
    // Create the new filter constant - transpose filter value
    // from [i, o] -> [o, i]. This is because we assume `[i, o]` format for
    // `stablehlo.dot_general` (i.e. contracting dimension == 1) whereas
    // `tfl.fully_connected` accepts an OI format.
    auto filter_constant_op =
        cast<stablehlo::ConstantOp>(op.getOperand(1).getDefiningOp());

    TFL::QConstOp new_filter_constant_op =
        CreateTflConstOpForFilter(filter_constant_op, rewriter,
                                  /*is_per_axis=*/true);
    const Value input_value = op.getOperand(0);
    const double input_scale = input_value.getType()
                                   .cast<TensorType>()
                                   .getElementType()
                                   .cast<UniformQuantizedType>()
                                   .getScale();
    TFL::QConstOp bias_constant_op = CreateTflConstOpForDummyBias(
        op.getLoc(), input_scale, new_filter_constant_op, rewriter,
        /*is_per_axis=*/true, *op.getContext());

    const Value result_value = op.getResult();
    // Set to `nullptr` because this attribute only matters when the input is
    // dynamic-range quantized.
    const BoolAttr asymmetric_quantize_inputs = nullptr;
    auto tfl_fully_connected_op = rewriter.create<TFL::FullyConnectedOp>(
        op.getLoc(), /*output=*/result_value.getType(),
        /*input=*/input_value, /*filter=*/new_filter_constant_op.getResult(),
        /*bias=*/bias_constant_op.getResult(),
        /*fused_activation_function=*/rewriter.getStringAttr("NONE"),
        /*weights_format=*/rewriter.getStringAttr("DEFAULT"),
        /*keep_num_dims=*/rewriter.getBoolAttr(false),
        asymmetric_quantize_inputs);

    rewriter.replaceAllUsesWith(result_value,
                                tfl_fully_connected_op.getResult(0));
    rewriter.eraseOp(op);
  }

 private:
  static LogicalResult MatchInput(Value input) {
    auto input_type = input.getType().cast<TensorType>();
    if (!input_type.hasRank() ||
        !(input_type.getRank() == 2 || input_type.getRank() == 3)) {
      LLVM_DEBUG(llvm::dbgs() << "Input expected to have rank of 2 or 3. Got: "
                              << input_type << ".\n");
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
    if (!filter_type.hasRank() || filter_type.getRank() != 2) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Filter tensor expected to have a tensor rank of 2. Got: "
                 << filter_type << ".\n");
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
            .getQuantizedDimension() != 1) {
      LLVM_DEBUG(llvm::dbgs() << "Quantized dimension should be 1. Got: "
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
};

// Rewrites `stablehlo.dot_general` to `tfl.fully_connected` or
// `tfl.batch_matmul` when it accepts uniform quantized tensors.
//
// Conditions for `tfl.fully_connected` conversion:
//   * Input and output tensors are per-tensor uniform quantized (i8->f32)
//     tensors.
//   * The filter tensor is constant a per-tensor uniform quantized (i8->f32)
//     tensor. The quantization dimension should be 1 (the non-contracting
//     dimension).
//   * The input tensor's rank is either 2 or 3. The last dimension of the input
//     tensor should be the contracting dimension, i.e. [..., c_x, r_x].
//   * The filter tensor's rank is 2. The contracting dimension should be the
//     first dimension (dim 0), i.e. [c_y, r_y] where c_y == r_x.
//   * Does not consider activation fusion.
//   * Does not consider bias add fusion.
// TODO: b/580909703 - Include conversion conditions for `tfl.batch_matmul` op.
//
// TODO: b/295264927 - `stablehlo.dot_general` with per-axis quantized operands
// is not specified in the StableHLO dialect. Update the spec to allow this.
// TODO: b/322428814 - Add StableHLO quantizer integration tests for ODML.
class RewriteQuantizedDotGeneralOpToTflFullyConnectedOrBatchMatmulOp
    : public OpRewritePattern<stablehlo::DotGeneralOp> {
  using OpRewritePattern<stablehlo::DotGeneralOp>::OpRewritePattern;

 public:
  LogicalResult match(stablehlo::DotGeneralOp op) const override {
    const stablehlo::DotDimensionNumbersAttr dot_dimension_nums =
        op.getDotDimensionNumbers();
    if (const int num_rhs_contracting_dims =
            dot_dimension_nums.getRhsContractingDimensions().size();
        num_rhs_contracting_dims != 1) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Expected number of contracting dimensions to be 1. Got: "
                 << num_rhs_contracting_dims << ".\n");
      return failure();
    }

    if (failed(MatchInput(op.getOperand(0)))) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Failed to match input for quantized dot_general op.\n");
      return failure();
    }

    if (failed(MatchFilter(op.getOperand(1)))) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Failed to match filter for quantized dot_general op.\n");
      return failure();
    }

    if (failed(MatchOutput(op.getResult()))) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Failed to match output for quantized dot_general op.\n");
      return failure();
    }

    if (failed(MatchUsers(op.getResult()))) {
      LLVM_DEBUG(llvm::dbgs() << "Failed to match subsequent requantize for "
                                 "quantized dot_general op.\n");
      return failure();
    }

    return success();
  }

  void rewrite(stablehlo::DotGeneralOp op,
               PatternRewriter& rewriter) const override {
    // Create the new filter constant - transpose filter value
    // from [i, o] -> [o, i]. This is because we assume `[i, o]` format for
    // `stablehlo.dot_general` (i.e. contracting dimension == 1) whereas
    // `tfl.fully_connected` accepts an OI format.
    auto filter_constant_op =
        cast<stablehlo::ConstantOp>(op.getOperand(1).getDefiningOp());

    TFL::QConstOp new_filter_constant_op = CreateTflConstOpForFilter(
        filter_constant_op, rewriter, /*is_per_axis=*/false);
    const Value input_value = op.getOperand(0);
    const double input_scale = input_value.getType()
                                   .cast<TensorType>()
                                   .getElementType()
                                   .cast<UniformQuantizedType>()
                                   .getScale();
    TFL::QConstOp bias_constant_op = CreateTflConstOpForDummyBias(
        op.getLoc(), input_scale, new_filter_constant_op, rewriter,
        /*is_per_axis=*/false, *op.getContext());

    auto output_op = op.getResult().getDefiningOp();
    Operation* requantize_op = *output_op->getResult(0).getUsers().begin();
    Operation* dequantize_op = *requantize_op->getResult(0).getUsers().begin();

    // Set to `nullptr` because this attribute only matters when the input is
    // dynamic-range quantized.
    const BoolAttr asymmetric_quantize_inputs = nullptr;
    auto tfl_fully_connected_op = rewriter.create<TFL::FullyConnectedOp>(
        op.getLoc(),
        /*output=*/
        requantize_op->getResult(0).getType(),  // result_value.getType(),
        /*input=*/input_value, /*filter=*/new_filter_constant_op.getResult(),
        /*bias=*/bias_constant_op.getResult(),
        /*fused_activation_function=*/rewriter.getStringAttr("NONE"),
        /*weights_format=*/rewriter.getStringAttr("DEFAULT"),
        /*keep_num_dims=*/rewriter.getBoolAttr(false),
        asymmetric_quantize_inputs);

    auto tfl_dequantize_op = rewriter.create<TFL::DequantizeOp>(
        op.getLoc(), dequantize_op->getResult(0).getType(),
        tfl_fully_connected_op->getResult(0));

    rewriter.replaceAllUsesWith(dequantize_op->getResult(0),
                                tfl_dequantize_op->getResult(0));

    rewriter.replaceAllUsesWith(op.getResult(),
                                tfl_fully_connected_op.getResult(0));

    rewriter.eraseOp(op);
  }

 private:
  static LogicalResult MatchInput(Value input) {
    auto input_type = input.getType().cast<TensorType>();
    if (!input_type.hasRank() ||
        !(input_type.getRank() == 2 || input_type.getRank() == 3)) {
      LLVM_DEBUG(llvm::dbgs() << "Input expected to have rank of 2 or 3. Got: "
                              << input_type << ".\n");
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
    if (!filter_type.hasRank() || filter_type.getRank() != 2) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Filter tensor expected to have a tensor rank of 2. Got: "
                 << filter_type << ".\n");
      return failure();
    }

    const Type filter_element_type = filter_type.getElementType();
    if (!IsI8F32UniformQuantizedType(filter_element_type)) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Expected a uniform quantized (i8->f32) type. Got: "
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
    if (!IsI32F32UniformQuantizedType(output_element_type)) {
      LLVM_DEBUG(llvm::dbgs()
                 << "Expected a uniform quantized (i32->f32) type. Got: "
                 << output_element_type << ".\n");
      return failure();
    }
    return success();
  }

  static LogicalResult MatchUsers(Value output) {
    auto output_op = output.getDefiningOp();

    if (!output_op->hasOneUse()) {
      LLVM_DEBUG(llvm::dbgs() << "Expected output to be used only once.\n");
      return failure();
    }
    // TODO: b/309896242 - Add support for fused op case.
    if (Operation* requantize_op = dyn_cast_or_null<TFL::QuantizeOp>(
            *output_op->getResult(0).getUsers().begin())) {
      const Type requantize_element_type = requantize_op->getResult(0)
                                               .getType()
                                               .cast<TensorType>()
                                               .getElementType();
      if (!IsI8F32UniformQuantizedType(requantize_element_type)) {
        LLVM_DEBUG(llvm::dbgs() << "Expected a quantize (i8->f32) type. Got: "
                                << requantize_element_type << ".\n");
        return failure();
      }
      if (!isa<TFL::DequantizeOp>(
              *requantize_op->getResult(0).getUsers().begin())) {
        LLVM_DEBUG(llvm::dbgs() << "Expected a dequantize type.\n");
        return failure();
      }
    } else {
      // Op not followed by a requantization is not supported.
      return failure();
    }
    return success();
  }
};

// Rewrites quantized stablehlo.transpose to tfl.transpose.
// TODO: b/322428814 - Add StableHLO quantizer integration tests for ODML.
class RewriteTransposeOp : public OpRewritePattern<stablehlo::TransposeOp> {
 public:
  using OpRewritePattern<stablehlo::TransposeOp>::OpRewritePattern;

  LogicalResult match(stablehlo::TransposeOp op) const override {
    return success(IsOpFullyQuantized(op));
  }

  void rewrite(stablehlo::TransposeOp op,
               PatternRewriter& rewriter) const override {
    auto operand_type = op.getOperand().getType().cast<TensorType>();
    const int64_t rank = operand_type.getRank();
    ArrayRef<int64_t> shape(rank);
    TensorType permutation_type =
        operand_type.cloneWith(shape, rewriter.getI32Type());
    // Cast permutation attribute from i64 to i32 as they are required to be i32
    // in TFLite.
    SmallVector<int32_t> permutation_i32 =
        CastI64ArrayToI32(op.getPermutation()).value();
    auto permutation_attr =
        DenseIntElementsAttr::get(permutation_type, permutation_i32);
    auto permutation =
        rewriter.create<arith::ConstantOp>(op.getLoc(), permutation_attr);
    rewriter.replaceOpWithNewOp<TFL::TransposeOp>(op, op.getOperand(),
                                                  permutation);
  }
};

// Rewrites quantized stablehlo.reshape to tfl.reshape.
// TODO: b/322428814 - Add StableHLO quantizer integration tests for ODML.
class RewriteReshapeOp : public OpRewritePattern<stablehlo::ReshapeOp> {
 public:
  using OpRewritePattern<stablehlo::ReshapeOp>::OpRewritePattern;

  LogicalResult match(stablehlo::ReshapeOp op) const override {
    return success(IsOpFullyQuantized(op));
  }

  void rewrite(stablehlo::ReshapeOp op,
               PatternRewriter& rewriter) const override {
    auto result_type = op->getResult(0).getType().cast<TensorType>();
    // Cast result shapes from i64 to i32 as they are required to be i32 in
    // TFLite.
    SmallVector<int32_t> shape_i32 =
        CastI64ArrayToI32(result_type.getShape()).value();

    const int64_t shape_length = shape_i32.size();
    ArrayRef<int64_t> shape(shape_length);
    TensorType shape_type = result_type.cloneWith(shape, rewriter.getI32Type());
    auto shape_attr = DenseIntElementsAttr::get(shape_type, shape_i32);
    auto new_shape =
        rewriter.create<arith::ConstantOp>(op.getLoc(), shape_attr);
    rewriter.replaceOpWithNewOp<TFL::ReshapeOp>(op, op.getOperand(), new_shape);
  }
};

// Rewrites quantized stablehlo.select to tfl.select_v2.
// TODO: b/322428814 - Add StableHLO quantizer integration tests for ODML.
class RewriteSelectOp : public OpRewritePattern<stablehlo::SelectOp> {
 public:
  using OpRewritePattern<stablehlo::SelectOp>::OpRewritePattern;

  LogicalResult match(stablehlo::SelectOp op) const override {
    if (!IsQuantizedTensorType(op.getOperand(1).getType())) {
      return failure();
    }
    if (!IsQuantizedTensorType(op.getOperand(2).getType())) {
      return failure();
    }
    if (!IsQuantizedTensorType(op.getResult().getType())) {
      return failure();
    }
    return success();
  }

  void rewrite(stablehlo::SelectOp op,
               PatternRewriter& rewriter) const override {
    Value pred = op.getOperand(0);
    Value on_true = op.getOperand(1);
    Value on_false = op.getOperand(2);
    rewriter.replaceOpWithNewOp<TFL::SelectV2Op>(op, pred, on_true, on_false);
  }
};

// Rewrites quantized stablehlo.concatenate to tfl.concatenation.
// TODO: b/322428814 - Add StableHLO quantizer integration tests for ODML.
class RewriteConcatenateOp : public OpRewritePattern<stablehlo::ConcatenateOp> {
 public:
  using OpRewritePattern<stablehlo::ConcatenateOp>::OpRewritePattern;

  LogicalResult match(stablehlo::ConcatenateOp op) const override {
    return success(IsOpFullyQuantized(op));
  }

  void rewrite(stablehlo::ConcatenateOp op,
               PatternRewriter& rewriter) const override {
    Type output_type = op.getResult().getType();
    uint32_t axis = CastI64ToI32(op.getDimension()).value();
    rewriter.replaceOpWithNewOp<TFL::ConcatenationOp>(
        op, output_type, op.getOperands(), axis,
        /*fused_activation_function=*/rewriter.getStringAttr("NONE"));
  }
};

// Rewrites quantized stablehlo.pad to tfl.padv2.
// tfl.dilate is introduced in between when interior padding exists.
// TODO: b/322428814 - Add StableHLO quantizer integration tests for ODML.
class RewritePadOp : public OpRewritePattern<stablehlo::PadOp> {
 public:
  using OpRewritePattern<stablehlo::PadOp>::OpRewritePattern;

  LogicalResult match(stablehlo::PadOp op) const override {
    return success(IsOpFullyQuantized(op));
  }

  void rewrite(stablehlo::PadOp op, PatternRewriter& rewriter) const override {
    Value input = op.getOperand();
    // If any of the interior padding is non-zero, operand should be dilated
    // first, and then padded.
    if (llvm::any_of(op.getInteriorPadding(),
                     [](int64_t pad) { return pad != 0; })) {
      input = InsertDilateOp(op, rewriter);
    }

    TensorType operand_type = input.getType().cast<TensorType>();
    const int64_t rank = operand_type.getRank();
    // Shape of padding should be [rank, 2].
    SmallVector<int64_t> shape{rank, 2};
    TensorType padding_type =
        operand_type.cloneWith(shape, rewriter.getI32Type());

    ArrayRef<int64_t> padding_low = op.getEdgePaddingLow();
    ArrayRef<int64_t> padding_high = op.getEdgePaddingHigh();
    SmallVector<int32_t> padding_value;
    for (int i = 0; i < rank; ++i) {
      padding_value.push_back(CastI64ToI32(padding_low[i]).value());
      padding_value.push_back(CastI64ToI32(padding_high[i]).value());
    }

    TensorType output_type = op.getResult().getType().cast<TensorType>();
    Value constant_values = op.getPaddingValue();
    auto padding_attr = DenseIntElementsAttr::get(padding_type, padding_value);
    auto padding =
        rewriter.create<arith::ConstantOp>(op.getLoc(), padding_attr);
    rewriter.replaceOpWithNewOp<TFL::PadV2Op>(op, output_type, input, padding,
                                              constant_values);
  }

  Value InsertDilateOp(stablehlo::PadOp op, PatternRewriter& rewriter) const {
    Value input = op.getOperand();
    TensorType operand_type = input.getType().cast<TensorType>();
    const int64_t rank = operand_type.getRank();

    ArrayRef<int64_t> dilate_shape(rank);
    TensorType dilate_type =
        operand_type.cloneWith(dilate_shape, rewriter.getI32Type());
    ArrayRef<int64_t> interior_padding_i64 = op.getInteriorPadding();
    SmallVector<int32_t> interior_padding_i32 =
        CastI64ArrayToI32(interior_padding_i64).value();
    auto dilate_attr =
        DenseIntElementsAttr::get(dilate_type, interior_padding_i32);
    auto dilate = rewriter.create<arith::ConstantOp>(op.getLoc(), dilate_attr);

    // Shape after dilation.
    SmallVector<int64_t> dilated_shape(rank);
    ArrayRef<int64_t> operand_shape = operand_type.getShape();
    for (int i = 0; i < rank; ++i) {
      dilated_shape[i] =
          operand_shape[i] + interior_padding_i64[i] * (operand_shape[i] - 1);
    }
    TensorType output_type = op.getResult().getType().cast<TensorType>();
    Type dilated_output_type = output_type.clone(dilated_shape);
    Value constant_values = op.getPaddingValue();

    return rewriter.create<TFL::DilateOp>(dilate.getLoc(), dilated_output_type,
                                          input, dilate, constant_values);
  }
};

// Rewrites quantized stablehlo.slice to tfl.slice or tfl.strided_slice.
// TODO: b/322428814 - Add StableHLO quantizer integration tests for ODML.
class RewriteSliceOp : public OpRewritePattern<stablehlo::SliceOp> {
 public:
  using OpRewritePattern<stablehlo::SliceOp>::OpRewritePattern;

  LogicalResult match(stablehlo::SliceOp op) const override {
    return success(IsOpFullyQuantized(op));
  }

  void rewrite(stablehlo::SliceOp op,
               PatternRewriter& rewriter) const override {
    auto operand_type = op.getOperand().getType().cast<TensorType>();
    Type output_type = op.getResult().getType();
    const int64_t rank = operand_type.getRank();

    ArrayRef<int64_t> idx_shape(rank);
    TensorType idx_type =
        operand_type.cloneWith(idx_shape, rewriter.getI32Type());

    ArrayRef<int64_t> start_idx_i64 = op.getStartIndices();
    ArrayRef<int64_t> limit_idx_i64 = op.getLimitIndices();

    SmallVector<int32_t> start_idx_i32 =
        CastI64ArrayToI32(start_idx_i64).value();
    auto start_idx_attr = DenseIntElementsAttr::get(idx_type, start_idx_i32);
    auto start_idx =
        rewriter.create<arith::ConstantOp>(op.getLoc(), start_idx_attr);

    SmallVector<int32_t> slice_size_i32(rank);
    for (int i = 0; i < rank; ++i) {
      slice_size_i32[i] =
          CastI64ToI32(limit_idx_i64[i] - start_idx_i64[i]).value();
    }
    auto slice_size_attr = DenseIntElementsAttr::get(idx_type, slice_size_i32);
    auto slice_size =
        rewriter.create<arith::ConstantOp>(op.getLoc(), slice_size_attr);

    ArrayRef<int64_t> strides = op.getStrides();
    // If stride of every dimension is 1, create tfl.slice and return early.
    // Otherwise, create tfl.strided_slice instead.
    if (llvm::all_of(strides, [](int64_t stride) { return stride == 1; })) {
      rewriter.replaceOpWithNewOp<TFL::SliceOp>(
          op, output_type, op.getOperand(), start_idx, slice_size);
      return;
    }

    SmallVector<int32_t> stride_i32 = CastI64ArrayToI32(strides).value();
    auto stride_attr = DenseIntElementsAttr::get(idx_type, stride_i32);
    auto stride = rewriter.create<arith::ConstantOp>(op.getLoc(), stride_attr);
    rewriter.replaceOpWithNewOp<TFL::StridedSliceOp>(
        op, output_type, op.getOperand(), start_idx, slice_size, stride,
        /*begin_mask=*/0, /*end_mask=*/0,
        /*ellipsis_mask=*/0, /*new_axis_mask=*/0, /*shrink_axis_mask=*/0,
        /*offset=*/false);
  }
};

// Rewrites quantized stablehlo.broadcast_in_dim to tfl.broadcast_to.
// tfl.transpose is introduced when broadcast_dimensions is not in ascending
// order. Also, tfl.expand_dims is introduced when input rank is smaller than
// output rank.
// TODO: b/322428814 - Add StableHLO quantizer integration tests for ODML.
class RewriteBroadcastInDimOp
    : public OpRewritePattern<stablehlo::BroadcastInDimOp> {
 public:
  using OpRewritePattern<stablehlo::BroadcastInDimOp>::OpRewritePattern;

  LogicalResult match(stablehlo::BroadcastInDimOp op) const override {
    return success(IsOpFullyQuantized(op));
  }

  void rewrite(stablehlo::BroadcastInDimOp op,
               PatternRewriter& rewriter) const override {
    auto operand_type = op.getOperand().getType().cast<TensorType>();
    auto output_type = op.getResult().getType().cast<TensorType>();
    Value input = op.getOperand();

    // If broadcast_dimensions is not in ascending order, transpose first.
    if (!llvm::is_sorted(op.getBroadcastDimensions())) {
      input = InsertTransposeOp(op, rewriter);
    }

    // If rank of operand is smaller than that of the output, expand dimensions
    // before broadcasting.
    if (operand_type.getRank() < output_type.getRank()) {
      input = InsertExpandDimsOp(op, rewriter, input, output_type.getRank());
    }

    SmallVector<int32_t> broadcast_shape =
        CastI64ArrayToI32(output_type.getShape()).value();
    TensorType broadcast_shape_type =
        output_type.cloneWith({output_type.getRank()}, rewriter.getI32Type());
    auto broadcast_shape_attr =
        DenseIntElementsAttr::get(broadcast_shape_type, broadcast_shape);
    auto shape =
        rewriter.create<arith::ConstantOp>(op.getLoc(), broadcast_shape_attr);

    rewriter.replaceOpWithNewOp<TFL::BroadcastToOp>(op, output_type, input,
                                                    shape);
  }

  Value InsertTransposeOp(stablehlo::BroadcastInDimOp op,
                          PatternRewriter& rewriter) const {
    SmallVector<int64_t> sorted_dims = op.getBroadcastDimensions();
    llvm::sort(sorted_dims);
    auto broadcast_dims = op.getBroadcastDimensions();
    SmallVector<int32_t> permutation(
        llvm::map_range(broadcast_dims, [sorted_dims](int64_t dim) {
          return static_cast<int32_t>(llvm::find(sorted_dims, dim) -
                                      sorted_dims.begin());
        }));
    auto operand_type = op.getOperand().getType().cast<TensorType>();
    TensorType perm_type = operand_type.cloneWith(
        {static_cast<int64_t>(permutation.size())}, rewriter.getI32Type());
    auto perm_attr = DenseIntElementsAttr::get(perm_type, permutation);
    auto perm = rewriter.create<arith::ConstantOp>(op.getLoc(), perm_attr);
    Value input = op.getOperand();

    return rewriter.create<TFL::TransposeOp>(op.getLoc(), input, perm);
  }

  Value InsertExpandDimsOp(stablehlo::BroadcastInDimOp op,
                           PatternRewriter& rewriter, Value input,
                           int64_t output_rank) const {
    auto input_type = input.getType().cast<TensorType>();
    SmallVector<int64_t> input_shape(input_type.getShape());
    SmallVector<int64_t> input_dims = op.getBroadcastDimensions();

    while (input_dims.size() < output_rank) {
      int32_t dim_to_expand = 0;
      for (int32_t i = 0; i < output_rank; ++i) {
        if (!llvm::is_contained(input_dims, i)) {
          dim_to_expand = i;
          break;
        }
      }

      TensorType dim_type = input_type.cloneWith({static_cast<int64_t>(1)},
                                                 rewriter.getI32Type());
      ArrayRef<int32_t> dims(dim_to_expand);
      auto dim_attr = DenseIntElementsAttr::get(dim_type, dims);
      auto dim = rewriter.create<arith::ConstantOp>(op.getLoc(), dim_attr);

      input_shape.insert(input_shape.begin() + dim_to_expand, 1);
      TensorType expanded_type = input_type.clone(input_shape);
      input = rewriter.create<TFL::ExpandDimsOp>(op.getLoc(), expanded_type,
                                                 input, dim);

      // Update expanded dimension in the input dimensions for the next
      // iteration.
      input_dims.push_back(static_cast<int64_t>(dim_to_expand));
    }
    return input;
  }
};

void UniformQuantizedStablehloToTflPass::runOnOperation() {
  func::FuncOp func_op = getOperation();
  MLIRContext& ctx = getContext();

  RewritePatternSet patterns(&ctx);
  patterns.add<RewriteUniformQuantizeOp, RewriteUniformDequantizeOp,
               RewriteUpstreamQuantizedConvolutionOp,
               RewriteUpstreamQuantizedDotGeneralOpToBatchMatmulOp,
               RewriteUpstreamQuantizedDotGeneralOpToTflFullyConnectedOp,
               RewriteQuantizedDotGeneralOpToTflFullyConnectedOrBatchMatmulOp,
               RewriteTransposeOp, RewriteReshapeOp, RewriteSelectOp,
               RewriteConcatenateOp, RewritePadOp, RewriteSliceOp,
               RewriteBroadcastInDimOp>(&ctx);

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
