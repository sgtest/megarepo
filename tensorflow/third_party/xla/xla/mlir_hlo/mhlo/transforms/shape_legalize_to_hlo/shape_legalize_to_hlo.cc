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

#include <cstddef>
#include <cstdint>
#include <memory>
#include <utility>

#include "llvm/ADT/SmallVector.h"
#include "mhlo/IR/hlo_ops.h"
#include "mhlo/transforms/passes.h"
#include "mlir/Dialect/Arith/IR/Arith.h"
#include "mlir/Dialect/Func/IR/FuncOps.h"
#include "mlir/Dialect/Shape/IR/Shape.h"
#include "mlir/Dialect/Tensor/IR/Tensor.h"
#include "mlir/IR/Builders.h"
#include "mlir/IR/BuiltinAttributes.h"
#include "mlir/IR/BuiltinOps.h"
#include "mlir/IR/BuiltinTypes.h"
#include "mlir/IR/DialectRegistry.h"
#include "mlir/IR/Location.h"
#include "mlir/IR/PatternMatch.h"
#include "mlir/IR/TypeUtilities.h"
#include "mlir/IR/Value.h"
#include "mlir/IR/ValueRange.h"
#include "mlir/Pass/Pass.h"
#include "mlir/Support/LLVM.h"
#include "mlir/Support/LogicalResult.h"
#include "mlir/Support/TypeID.h"
#include "mlir/Transforms/DialectConversion.h"

namespace mlir {
namespace mhlo {

#define GEN_PASS_DEF_SHAPELEGALIZETOHLOPASS
#include "mhlo/transforms/mhlo_passes.h.inc"

namespace {

bool hasI32Style(Value value) {
  auto type = value.getType().dyn_cast<ShapedType>();
  return type && type.getElementType().isInteger(32);
}

// Cast from index-based shape representation used in the Shape dialect to the
// i32-based representation used in HLO:
//   * index => tensor<i32>.
//   * tensor<Nxindex> => tensor<Nxi32>.
//   * All i32-based types from above => themselves.
// There is no convenient op that can express this, so we're using
// unrealized_conversion_cast (with the idea that all these casts will
// annihilate at the end of the pass).
Value castToI32(PatternRewriter& rewriter, Location loc, Value value) {
  Type resultType;
  if (value.getType().isIndex())
    resultType = RankedTensorType::get({}, rewriter.getI32Type());
  if (auto valueType = value.getType().dyn_cast<ShapedType>()) {
    if (!valueType.hasStaticShape()) return {};
    if (valueType.getElementType().isInteger(32)) return value;
    if (valueType.getElementType().isIndex())
      resultType =
          RankedTensorType::get(valueType.getShape(), rewriter.getI32Type());
  }
  if (!resultType) return {};
  auto cast =
      rewriter.create<UnrealizedConversionCastOp>(loc, resultType, value);
  return cast.getResult(0);
}

bool hasIndexStyle(Value value) {
  if (value.getType().isIndex()) return true;
  auto type = value.getType().dyn_cast<ShapedType>();
  return type && type.getElementType().isIndex();
}

// Cast from the i32-based shape representation used in HLO to the index-based
// representation used in the Shape dialect:
//   * tensor<i32> => index.
//   * tensor<Nxi32> => tensor<Nxindex>.
//   * All index-based types from above => themselves.
// There is no convenient op that can express this, so we're using
// unrealized_conversion_cast (with the idea that all these casts will
// annihilate at the end of the pass).
Value castToIndex(PatternRewriter& rewriter, Location loc, Value value) {
  Type resultType;
  if (value.getType().isIndex()) return value;
  if (auto valueType = value.getType().dyn_cast<ShapedType>()) {
    if (!valueType.hasStaticShape()) return {};
    if (valueType.getElementType().isInteger(32)) {
      if (valueType.getRank() == 0) {
        resultType = rewriter.getIndexType();
      } else {
        resultType = RankedTensorType::get(valueType.getShape(),
                                           rewriter.getIndexType());
      }
    }
    if (valueType.getElementType().isIndex()) return value;
  }
  if (!resultType) return {};
  auto cast =
      rewriter.create<UnrealizedConversionCastOp>(loc, resultType, value);
  return cast.getResult(0);
}

void insertShapeAssertionCustomCall(OpBuilder builder, Location loc,
                                    Value assert) {
  auto customCall =
      builder.create<mhlo::CustomCallOp>(loc, TypeRange{}, ValueRange{assert});
  customCall.setCallTargetName("shape_assertion");
  customCall.setHasSideEffect(true);
  customCall->setAttr("error_message",
                      builder.getStringAttr("Shape assertion failed"));
}

struct ConvertComputeReshapeShapeOpPattern
    : public OpRewritePattern<ComputeReshapeShapeOp> {
  using OpRewritePattern::OpRewritePattern;
  LogicalResult matchAndRewrite(ComputeReshapeShapeOp op,
                                PatternRewriter& rewriter) const override {
    // Cast num_elements from index to tensor<i32>.
    // Cast dynamic_shape from tensor<Nxindex> to tensor<Nxi32> if needed.
    // (mhlo.compute_reshape_shape supports both index- and integer-based
    // dynamic_shape operands).
    // This cannot error out given how the operation is currently defined.
    auto numElementsI32 = castToI32(rewriter, op.getLoc(), op.getNumElements());
    auto dynamicShapeI32x1 =
        castToI32(rewriter, op.getLoc(), op.getDynamicShape());
    if (!numElementsI32 || !dynamicShapeI32x1)
      return rewriter.notifyMatchFailure(op, "cast to i32 failed");
    auto rank = dynamicShapeI32x1.getType().cast<ShapedType>().getNumElements();

    // Obtain individual input dimension sizes and also compute the product of
    // all these dimension sizes.
    auto i32Type = RankedTensorType::get({}, rewriter.getI32Type());
    Value dynamicNumElementsI32 = rewriter.create<ConstantOp>(
        op.getLoc(), DenseIntElementsAttr::get<int32_t>(i32Type, -1));
    SmallVector<Value> dynamicSizesI32;
    for (auto i = 0; i < rank; ++i) {
      auto dynamicSizeI32x1 = rewriter.create<SliceOp>(
          op.getLoc(), dynamicShapeI32x1, rewriter.getI64TensorAttr(i),
          rewriter.getI64TensorAttr(i + 1), rewriter.getI64TensorAttr(1));
      auto dynamicSizeI32 =
          rewriter.create<ReshapeOp>(op.getLoc(), i32Type, dynamicSizeI32x1);
      dynamicSizesI32.push_back(dynamicSizeI32);
      dynamicNumElementsI32 = rewriter.create<MulOp>(
          op.getLoc(), dynamicNumElementsI32, dynamicSizeI32);
    }

    // Compute the dimension size that corresponds to -1 in dynamic_shape.
    // If such a dimension doesn't exist, then this value doesn't matter.
    auto computedSizeI32 = rewriter.create<DivOp>(op.getLoc(), numElementsI32,
                                                  dynamicNumElementsI32);

    // Compute individual output dimension sizes, replacing a potential -1
    // with the value computed above.
    auto i32x1Type = RankedTensorType::get({1}, rewriter.getI32Type());
    Value minusOneI32 = rewriter.create<ConstantOp>(
        op.getLoc(), DenseIntElementsAttr::get<int32_t>(i32Type, -1));
    SmallVector<Value> resultSizesI32x1;
    for (auto i = 0; i < rank; ++i) {
      auto eqMinusOne =
          rewriter.create<CompareOp>(op.getLoc(), dynamicSizesI32[i],
                                     minusOneI32, ComparisonDirection::EQ);
      auto resultSizeI32 = rewriter.create<SelectOp>(
          op.getLoc(), eqMinusOne, computedSizeI32, dynamicSizesI32[i]);
      auto resultSizeI32x1 =
          rewriter.create<ReshapeOp>(op.getLoc(), i32x1Type, resultSizeI32);
      resultSizesI32x1.push_back(resultSizeI32x1);
    }
    auto resultI32 =
        rewriter.create<mhlo::ConcatenateOp>(op.getLoc(), resultSizesI32x1,
                                             /*dimension=*/0);

    // Cast the result to tensor<Nxindex> if needed.
    // (mhlo.compute_reshape_shape supports both index- and integer-based
    // results).
    // This cannot error out given how the operation is currently defined.
    auto resultIndex = hasI32Style(op.getResult())
                           ? resultI32
                           : castToIndex(rewriter, op.getLoc(), resultI32);
    if (!resultIndex || resultIndex.getType() != op.getResult().getType())
      return rewriter.notifyMatchFailure(op, "cast to index failed");
    rewriter.replaceOp(op, resultIndex);
    return success();
  }
};

struct ConvertNumElementsOpPattern
    : public OpRewritePattern<shape::NumElementsOp> {
  using OpRewritePattern::OpRewritePattern;
  LogicalResult matchAndRewrite(shape::NumElementsOp op,
                                PatternRewriter& rewriter) const override {
    // Cast shape from tensor<Nxindex> to tensor<Nxi32>.
    // This will error out if shape is !shape.shape.
    auto shapeI32 = castToI32(rewriter, op.getLoc(), op.getShape());
    if (!shapeI32) return rewriter.notifyMatchFailure(op, "cast to i32 failed");
    auto rank = shapeI32.getType().cast<ShapedType>().getNumElements();

    // Compute the product of the individual dimension sizes.
    // Using this representation instead of mhlo::ReduceOp because it is more
    // amenable to optimizations. (Reduce can be folded only if the entire
    // shape is static, but individual multiplications can be folded if
    // individual dimensions are static).
    auto resultI32Type = RankedTensorType::get({}, rewriter.getI32Type());
    Value resultI32 = rewriter.create<ConstantOp>(
        op.getLoc(), DenseIntElementsAttr::get<int32_t>(resultI32Type, 1));
    for (auto i = 0; i < rank; ++i) {
      auto sizeI32x1 = rewriter.create<SliceOp>(
          op.getLoc(), shapeI32, rewriter.getI64TensorAttr(i),
          rewriter.getI64TensorAttr(i + 1), rewriter.getI64TensorAttr(1));
      auto sizeI32 =
          rewriter.create<ReshapeOp>(op.getLoc(), resultI32Type, sizeI32x1);
      resultI32 = rewriter.create<MulOp>(op.getLoc(), resultI32, sizeI32);
    }

    // Cast result from tensor<i32> to index.
    // This will error out if the result is !shape.size.
    auto resultIndex = castToIndex(rewriter, op.getLoc(), resultI32);
    if (!resultIndex || resultIndex.getType() != op.getResult().getType())
      return rewriter.notifyMatchFailure(op, "cast to index failed");
    rewriter.replaceOp(op, resultIndex);
    return success();
  }
};

struct ConvertShapeOfOpPattern : public OpRewritePattern<shape::ShapeOfOp> {
  using OpRewritePattern::OpRewritePattern;
  LogicalResult matchAndRewrite(shape::ShapeOfOp op,
                                PatternRewriter& rewriter) const override {
    auto operandType = op.getArg().getType().dyn_cast<RankedTensorType>();
    if (!operandType)
      return rewriter.notifyMatchFailure(op, "expected ranked operand");

    // Produce an MHLO equivalent of this shape::ShapeOfOp.
    // This is a very laborious representation because MHLO is currently lacking
    // convenient tools to express this.
    SmallVector<Value> sizesI32x1;
    for (auto i = 0; i < operandType.getRank(); ++i) {
      auto sizeI32 =
          rewriter.create<GetDimensionSizeOp>(op.getLoc(), op.getArg(), i);
      auto sizeI32x1 = rewriter.create<ReshapeOp>(
          op.getLoc(), RankedTensorType::get({1}, rewriter.getI32Type()),
          sizeI32);
      sizesI32x1.push_back(sizeI32x1);
    }
    auto shapeI32 =
        rewriter.create<mhlo::ConcatenateOp>(op.getLoc(), sizesI32x1,
                                             /*dimension=*/0);

    // Cast result from tensor<Nxi32> to tensor<Nxindex>.
    // This will error out if the result is !shape.shape.
    auto shapeIndex = castToIndex(rewriter, op.getLoc(), shapeI32);
    if (!shapeIndex || shapeIndex.getType() != op.getResult().getType())
      return rewriter.notifyMatchFailure(op, "cast to index failed");
    rewriter.replaceOp(op, shapeIndex);
    return success();
  }
};

struct ConvertShapeBroadcastOpPattern
    : public OpRewritePattern<shape::BroadcastOp> {
  using OpRewritePattern::OpRewritePattern;
  LogicalResult matchAndRewrite(shape::BroadcastOp op,
                                PatternRewriter& rewriter) const override {
    // Only support broadcasting for two 1D tensors with same size.
    if (op.getShapes().size() != 2) return failure();
    auto shape1 = castToI32(rewriter, op.getLoc(), op.getShapes().front());
    auto shape2 = castToI32(rewriter, op.getLoc(), op.getShapes().back());
    if (!shape1 || !shape2) return failure();
    auto tensorType1 = shape1.getType().dyn_cast<RankedTensorType>();
    auto tensorType2 = shape2.getType().dyn_cast<RankedTensorType>();
    if (!tensorType1 || !tensorType2 ||
        tensorType1.getDimSize(0) != tensorType2.getDimSize(0))
      return failure();

    // By definition, broadcasted dims are:
    //   result[i] = lhs[i] if lhs[i] == rhs[i]
    //             = lhs[i] if rhs[i] == 1
    //             = rhs[i] if lhs[i] == 1
    //
    // We assume that there is shape.cstr_broadcastable check done elsewhere to
    // make sure the shapes are broadcastable, then we can calculate broadcast
    // result simply using MaxOp. In case the shapes are not broadcastable, the
    // result extent tensor is undefined according to spec. So this
    // implementation is technically correct.
    auto broadcasted =
        rewriter.create<mhlo::MaxOp>(op->getLoc(), shape1, shape2);

    auto broadcastedIndex = castToIndex(rewriter, op.getLoc(), broadcasted);
    if (!broadcastedIndex ||
        broadcastedIndex.getType() != op.getResult().getType())
      return rewriter.notifyMatchFailure(op, "cast to index failed");
    rewriter.replaceOp(op, broadcastedIndex);
    return success();
  }
};

struct ConvertTensorDimPattern : public OpRewritePattern<tensor::DimOp> {
  using OpRewritePattern::OpRewritePattern;
  LogicalResult matchAndRewrite(tensor::DimOp op,
                                PatternRewriter& rewriter) const override {
    // We only support getting static index.
    auto constIndex =
        dyn_cast_or_null<arith::ConstantIndexOp>(op.getIndex().getDefiningOp());
    if (!constIndex) {
      return failure();
    }

    auto dim = rewriter.create<mhlo::GetDimensionSizeOp>(
        op->getLoc(), op.getSource(), constIndex.value());
    auto dimIndex = castToIndex(rewriter, op.getLoc(), dim);
    rewriter.replaceOp(op, dimIndex);
    return success();
  }
};

struct ConvertTensorFromElementsPattern
    : public OpRewritePattern<tensor::FromElementsOp> {
  using OpRewritePattern::OpRewritePattern;
  LogicalResult matchAndRewrite(tensor::FromElementsOp op,
                                PatternRewriter& rewriter) const override {
    // We only handle 1D tensor with index types. tensor.from_elements spec
    // allows the same element type only for all input/output.
    auto tensorType =
        op.getResult().getType().dyn_cast_or_null<RankedTensorType>();
    if (!tensorType || tensorType.getRank() != 1) {
      return failure();
    }
    if (!hasIndexStyle(op.getResult())) return failure();

    SmallVector<Value> elementI32x1;
    for (size_t i = 0; i < op.getElements().size(); ++i) {
      if (auto constIndex = dyn_cast_or_null<arith::ConstantIndexOp>(
              op.getElements()[i].getDefiningOp())) {
        elementI32x1.push_back(rewriter.create<ConstantOp>(
            op.getLoc(), DenseIntElementsAttr::get<int32_t>(
                             RankedTensorType::get({1}, rewriter.getI32Type()),
                             static_cast<int32_t>(constIndex.value()))));
      } else {
        elementI32x1.push_back(rewriter.create<ReshapeOp>(
            op.getLoc(), RankedTensorType::get({1}, rewriter.getI32Type()),
            castToI32(rewriter, op->getLoc(), op.getElements()[i])));
      }
    }
    Value tensorI32 =
        rewriter.create<mhlo::ConcatenateOp>(op.getLoc(), elementI32x1,
                                             /*dimension=*/0);

    tensorI32 = hasI32Style(op.getResult())
                    ? tensorI32
                    : castToIndex(rewriter, op.getLoc(), tensorI32);
    if (!tensorI32 || tensorI32.getType() != op.getResult().getType())
      return rewriter.notifyMatchFailure(op, "cast to index failed");
    rewriter.replaceOp(op, tensorI32);
    return success();
  }
};

struct ConvertCstrBroadcastableOp
    : public OpRewritePattern<shape::CstrBroadcastableOp> {
  using OpRewritePattern::OpRewritePattern;
  LogicalResult matchAndRewrite(shape::CstrBroadcastableOp op,
                                PatternRewriter& rewriter) const override {
    // Only support broadcasting for two 1D tensors with same size.
    if (op.getShapes().size() != 2) return failure();
    auto shape1 = castToI32(rewriter, op.getLoc(), op.getShapes().front());
    auto shape2 = castToI32(rewriter, op.getLoc(), op.getShapes().back());
    if (!shape1 || !shape2) return failure();
    auto tensorType1 = shape1.getType().dyn_cast<RankedTensorType>();
    auto tensorType2 = shape2.getType().dyn_cast<RankedTensorType>();
    if (!tensorType1 || !tensorType2 ||
        tensorType1.getDimSize(0) != tensorType2.getDimSize(0))
      return failure();

    // Compute if each dim is broadcastable. A dim is broadcastable iff
    // dimSize1 == dimSize2 or dimSize1 == 1 or dimSize2 == 1
    auto allOne = rewriter.create<mhlo::ConstantOp>(
        op.getLoc(), DenseIntElementsAttr::get<int32_t>(
                         RankedTensorType::get({tensorType1.getDimSize(0)},
                                               rewriter.getI32Type()),
                         static_cast<int32_t>(1)));
    Value dimSize1Is1 = rewriter.create<mhlo::CompareOp>(
        op.getLoc(), shape1, allOne, ComparisonDirection::EQ);
    Value dimSize2Is1 = rewriter.create<mhlo::CompareOp>(
        op.getLoc(), shape2, allOne, ComparisonDirection::EQ);
    Value eitherDimSizeIs1 =
        rewriter.create<mhlo::OrOp>(op.getLoc(), dimSize1Is1, dimSize2Is1);
    Value dimSizeEq = rewriter.create<mhlo::CompareOp>(
        op.getLoc(), shape1, shape2, ComparisonDirection::EQ);
    Value dimBroadcastable =
        rewriter.create<mhlo::OrOp>(op.getLoc(), eitherDimSizeIs1, dimSizeEq);

    // Iterate over each dim to check that all dims are broadcastable.
    auto boolType = RankedTensorType::get({1}, rewriter.getI1Type());
    Value allBroadcastable = rewriter.create<ConstantOp>(
        op.getLoc(), DenseIntElementsAttr::get<bool>(boolType, true));
    for (auto i = 0; i < tensorType1.getDimSize(0); ++i) {
      Value broadcastable = rewriter.create<SliceOp>(
          op.getLoc(), dimBroadcastable, rewriter.getI64TensorAttr(i),
          rewriter.getI64TensorAttr(i + 1), rewriter.getI64TensorAttr(1));
      allBroadcastable =
          rewriter.create<AndOp>(op.getLoc(), allBroadcastable, broadcastable);
    }
    Value allBroadcastableScalar = rewriter.create<ReshapeOp>(
        op.getLoc(), RankedTensorType::get({}, rewriter.getI1Type()),
        allBroadcastable);

    // Add CustomCallOp and replace Cstr op with const witness, which is useful
    // for canonicalizer to remove the shape.assuming region.
    insertShapeAssertionCustomCall(rewriter, op->getLoc(),
                                   allBroadcastableScalar);
    rewriter.replaceOpWithNewOp<shape::ConstWitnessOp>(op.getOperation(), true);
    return success();
  }
};

// As defined in tensorflow/compiler/xla/mlir_hlo/mhlo/IR/hlo_ops.td, the
// dynamic shape is reshapable if it has only 1 dynamic dimension and the number
// of element can divide the product of the static dimension sizes.
struct ConvertCstrReshapableOp
    : public OpRewritePattern<mhlo::CstrReshapableOp> {
  using OpRewritePattern::OpRewritePattern;
  LogicalResult matchAndRewrite(mhlo::CstrReshapableOp op,
                                PatternRewriter& rewriter) const override {
    Value numElements;
    if (auto constIndex = dyn_cast_or_null<arith::ConstantIndexOp>(
            op.getNumElements().getDefiningOp())) {
      numElements = rewriter.create<ConstantOp>(
          op.getLoc(), DenseIntElementsAttr::get<int32_t>(
                           RankedTensorType::get({}, rewriter.getI32Type()),
                           static_cast<int32_t>(constIndex.value())));
    } else {
      numElements = castToI32(rewriter, op->getLoc(), op.getNumElements());
    }
    Value dyanmicShape =
        castToI32(rewriter, op->getLoc(), op.getDynamicShape());
    if (!dyanmicShape || !numElements) return failure();
    auto dyanmicShapeType =
        dyanmicShape.getType().dyn_cast_or_null<RankedTensorType>();
    if (!dyanmicShapeType || dyanmicShapeType.getRank() != 1) return failure();

    auto i32Type = RankedTensorType::get({}, rewriter.getI32Type());
    Value minusOne = rewriter.create<ConstantOp>(
        op.getLoc(), DenseIntElementsAttr::get<int32_t>(i32Type, -1));
    // There must only be 1 dynamic dimension, enforced later in this pattern.
    // Init as -1 so that it will cancel with the dynamic dim when calculating
    // product of static dim sizes.
    Value productStaticDimSizes = minusOne;
    Value one = rewriter.create<ConstantOp>(
        op.getLoc(), DenseIntElementsAttr::get<int32_t>(i32Type, 1));
    Value zero = rewriter.create<ConstantOp>(
        op.getLoc(), DenseIntElementsAttr::get<int32_t>(i32Type, 0));
    Value numDyanmicDim = zero;
    for (auto i = 0; i < dyanmicShapeType.getDimSize(0); ++i) {
      // Calculate the product of static dimension sizes.
      Value dimSize = rewriter.create<SliceOp>(
          op.getLoc(), dyanmicShape, rewriter.getI64TensorAttr(i),
          rewriter.getI64TensorAttr(i + 1), rewriter.getI64TensorAttr(1));
      dimSize = rewriter.create<ReshapeOp>(op.getLoc(), i32Type, dimSize);
      productStaticDimSizes =
          rewriter.create<MulOp>(op.getLoc(), productStaticDimSizes, dimSize);
      // Count number of -1 dims, aka dynamic dimensions.
      Value eqMinusOne = rewriter.create<CompareOp>(
          op.getLoc(), dimSize, minusOne, ComparisonDirection::EQ);
      eqMinusOne =
          rewriter.create<SelectOp>(op.getLoc(), eqMinusOne, one, zero);
      numDyanmicDim =
          rewriter.create<AddOp>(op.getLoc(), numDyanmicDim, eqMinusOne);
    }

    // 1. Check there is 1 dynamic dim.
    Value exactlyOneDynamicDim = rewriter.create<CompareOp>(
        op.getLoc(), numDyanmicDim, one, ComparisonDirection::EQ);

    // 2. Check number of elements can be divided by product of static dim
    // sizes.
    Value rem =
        rewriter.create<RemOp>(op.getLoc(), numElements, productStaticDimSizes);
    Value reshapable = rewriter.create<CompareOp>(op.getLoc(), rem, zero,
                                                  ComparisonDirection::EQ);

    // Check both conditions are true.
    reshapable =
        rewriter.create<AndOp>(op.getLoc(), reshapable, exactlyOneDynamicDim);

    // Add CustomCallOp and replace Cstr op with const witness, which is
    // useful for canonicalizer to remove the shape.assuming region.
    insertShapeAssertionCustomCall(rewriter, op->getLoc(), reshapable);
    rewriter.replaceOpWithNewOp<shape::ConstWitnessOp>(op.getOperation(), true);
    return success();
  }
};

template <typename OpType>
struct CastOperandsPattern : public OpRewritePattern<OpType> {
  using OpRewritePattern<OpType>::OpRewritePattern;
  LogicalResult matchAndRewrite(OpType op,
                                PatternRewriter& rewriter) const override {
    if (!llvm::any_of(op->getOperands(), hasIndexStyle))
      return rewriter.notifyMatchFailure(op, "no operands need a cast to i32");

    // If op has operands of type tensor<Nxindex>, cast them to tensor<Nxi32>.
    // If producers of these operands have been transformed into casts from
    // tensor<Nxi32> to tensor<Nxindex>, then these casts will annihilate with
    // each other upon canonicalization.
    SmallVector<Value> operandsI32;
    for (auto operand : op->getOperands()) {
      if (hasIndexStyle(operand)) {
        operandsI32.push_back(castToI32(rewriter, op.getLoc(), operand));
      } else {
        operandsI32.push_back(operand);
      }
    }

    rewriter.replaceOpWithNewOp<OpType>(op, op->getResultTypes(), operandsI32,
                                        op->getAttrs());
    return success();
  }
};

// TODO(b/264240901): Comprehensively support shape computations to the extent
// needed to support bounded dynamism in MHLO export.
struct ShapeLegalizeToHloPass
    : public impl::ShapeLegalizeToHloPassBase<ShapeLegalizeToHloPass> {
  explicit ShapeLegalizeToHloPass(bool legalizeConstraints)
      : impl::ShapeLegalizeToHloPassBase<
            ShapeLegalizeToHloPass>::ShapeLegalizeToHloPassBase() {
    this->legalize_constraints_ = legalizeConstraints;
  }

  void runOnOperation() override {
    // In order to make dynamic MHLO programs compatible with HLO,
    // we need to get rid of all non-MHLO ops as well as the two shape-related
    // MHLO ops: mhlo.compute_reshape_shape and mhlo.cstr_reshapable.
    //
    // As an example, a cursory inspection of the TF/XLA bridge, which provides
    // one data point of an MHLO producer that can generate dynamic MHLO
    // programs, reveals the following non-MHLO ops:
    //   * shape.broadcast
    //   * shape.concat
    //   * shape.cstr_broadcastable
    //   * shape.cstr_eq
    //   * shape.dim
    //   * shape.split_at
    //   * shape.to_extent_tensor
    //   * shape.assuming
    //   * shape.assuming_yield
    //   * tensor.dim
    //   * tensor.extract
    //   * tensor.from_elements
    //
    // Most of these ops are convertible to MHLO, although the representation is
    // going to be pretty laborious for many of them. Luckily, canonicalization
    // is able to remove unnecessary cruft. At the moment, this pass is a
    // work in progress, so not all of these ops are supported.
    //
    // When legalize_constraints_ is set true, cstr* ops are also legalized.
    // A shape_assertion custom_call is used to check the constraint. And the
    // shape.assuming region will consume a shape.const_witness that evaluate to
    // true, so that it can be removed later in a canonicalizer pass.
    ConversionTarget target(getContext());
    target.addIllegalDialect<shape::ShapeDialect>();
    target.addIllegalDialect<tensor::TensorDialect>();
    target.addIllegalOp<mhlo::ComputeReshapeShapeOp>();
    target.addIllegalOp<mhlo::CstrReshapableOp>();
    target.addDynamicallyLegalDialect<mhlo::MhloDialect>([](Operation* op) {
      return !llvm::any_of(op->getOperands(), hasIndexStyle);
    });
    target.addLegalOp<tensor::CastOp>();
    target.addLegalOp<UnrealizedConversionCastOp>();
    if (this->legalize_constraints_) {
      target.addLegalOp<shape::ConstWitnessOp, shape::AssumingOp,
                        shape::AssumingYieldOp>();
    }

    // The patterns do what one might expect, converting between MLIR-style
    // and HLO-style shape computations.
    //
    // The only complication is that MLIR style uses index/tensor<Nxindex>
    // whereas HLO style uses tensor<i32>/vararg of tensor<i32>. We bridge
    // this gap by producing unrealized_conversion_cast ops, which we expect
    // to ultimately annihilate with each other upon canonicalization if
    // everything went right.
    RewritePatternSet patterns(&getContext());
    patterns.add<ConvertComputeReshapeShapeOpPattern>(&getContext());
    patterns.add<ConvertNumElementsOpPattern>(&getContext());
    patterns.add<ConvertShapeOfOpPattern>(&getContext());
    patterns.add<ConvertShapeBroadcastOpPattern>(&getContext());
    patterns.add<CastOperandsPattern<DynamicBroadcastInDimOp>>(&getContext());
    patterns.add<CastOperandsPattern<DynamicReshapeOp>>(&getContext());
    patterns.add<ConvertTensorDimPattern>(&getContext());
    patterns.add<ConvertTensorFromElementsPattern>(&getContext());
    if (this->legalize_constraints_) {
      patterns.add<ConvertCstrBroadcastableOp>(&getContext());
      patterns.add<ConvertCstrReshapableOp>(&getContext());
    }
    if (failed(applyPartialConversion(getOperation(), target,
                                      std::move(patterns))))
      return signalPassFailure();
  }
};

}  // namespace

std::unique_ptr<mlir::OperationPass<func::FuncOp>> createShapeLegalizeToHloPass(
    bool legalizeConstraints) {
  return std::make_unique<ShapeLegalizeToHloPass>(legalizeConstraints);
}

}  // namespace mhlo
}  // namespace mlir
