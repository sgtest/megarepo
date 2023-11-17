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
#include "tensorflow/compiler/mlir/quantization/stablehlo/uniform_quantized_types.h"

#include <cstdint>
#include <limits>

#include <gmock/gmock.h>
#include <gtest/gtest.h>
#include "mlir/Dialect/Quant/QuantOps.h"  // from @llvm-project
#include "mlir/Dialect/Quant/QuantTypes.h"  // from @llvm-project
#include "mlir/IR/Builders.h"  // from @llvm-project
#include "mlir/IR/Location.h"  // from @llvm-project
#include "mlir/IR/MLIRContext.h"  // from @llvm-project
#include "mlir/Support/LLVM.h"  // from @llvm-project

namespace mlir {
namespace quant {
namespace {

using ::testing::ElementsAreArray;
using ::testing::NotNull;

class CreateI8F32UniformQuantizedTypeTest : public ::testing::Test {
 protected:
  CreateI8F32UniformQuantizedTypeTest() : ctx_() {
    ctx_.loadDialect<quant::QuantizationDialect>();
  }

  MLIRContext ctx_;
};

TEST_F(CreateI8F32UniformQuantizedTypeTest, HasI8StorageType) {
  const UniformQuantizedType quantized_type =
      CreateI8F32UniformQuantizedType(UnknownLoc::get(&ctx_), ctx_,
                                      /*scale=*/1.0, /*zero_point=*/0);

  EXPECT_TRUE(quantized_type.getStorageType().isSignlessInteger(8));
}

TEST_F(CreateI8F32UniformQuantizedTypeTest, HasF32ExpressedType) {
  const UniformQuantizedType quantized_type =
      CreateI8F32UniformQuantizedType(UnknownLoc::get(&ctx_), ctx_,
                                      /*scale=*/1.0, /*zero_point=*/0);

  EXPECT_TRUE(quantized_type.getExpressedType().isF32());
}

TEST_F(CreateI8F32UniformQuantizedTypeTest, IsSigned) {
  const UniformQuantizedType quantized_type =
      CreateI8F32UniformQuantizedType(UnknownLoc::get(&ctx_), ctx_,
                                      /*scale=*/1.0, /*zero_point=*/0);

  EXPECT_TRUE(quantized_type.isSigned());
}

TEST_F(CreateI8F32UniformQuantizedTypeTest, StrageTypeMinMaxEqualToI8MinMax) {
  const UniformQuantizedType quantized_type =
      CreateI8F32UniformQuantizedType(UnknownLoc::get(&ctx_), ctx_,
                                      /*scale=*/1.0, /*zero_point=*/0);

  EXPECT_EQ(quantized_type.getStorageTypeMin(), -128);
  EXPECT_EQ(quantized_type.getStorageTypeMax(), 127);
}

TEST_F(CreateI8F32UniformQuantizedTypeTest, HasScaleAndZeroPointProperlySet) {
  const UniformQuantizedType quantized_type =
      CreateI8F32UniformQuantizedType(UnknownLoc::get(&ctx_), ctx_,
                                      /*scale=*/8.0, /*zero_point=*/99);

  EXPECT_EQ(quantized_type.getScale(), 8.0);
  EXPECT_EQ(quantized_type.getZeroPoint(), 99);
}

class CreateI32F32UniformQuantizedTypeTest : public ::testing::Test {
 protected:
  CreateI32F32UniformQuantizedTypeTest() : ctx_() {
    ctx_.loadDialect<quant::QuantizationDialect>();
  }

  MLIRContext ctx_;
};

TEST_F(CreateI32F32UniformQuantizedTypeTest, HasI32StorageType) {
  const UniformQuantizedType quantized_type =
      CreateI32F32UniformQuantizedType(UnknownLoc::get(&ctx_), ctx_,
                                       /*scale=*/1.0, /*zero_point=*/0);

  EXPECT_TRUE(quantized_type.getStorageType().isSignlessInteger(32));
}

TEST_F(CreateI32F32UniformQuantizedTypeTest, HasF32ExpressedType) {
  const UniformQuantizedType quantized_type =
      CreateI32F32UniformQuantizedType(UnknownLoc::get(&ctx_), ctx_,
                                       /*scale=*/1.0, /*zero_point=*/0);

  EXPECT_TRUE(quantized_type.getExpressedType().isF32());
}

TEST_F(CreateI32F32UniformQuantizedTypeTest, IsSigned) {
  const UniformQuantizedType quantized_type =
      CreateI32F32UniformQuantizedType(UnknownLoc::get(&ctx_), ctx_,
                                       /*scale=*/1.0, /*zero_point=*/0);

  EXPECT_TRUE(quantized_type.isSigned());
}

TEST_F(CreateI32F32UniformQuantizedTypeTest,
       SotrageTypeMinMaxEqualToI32MinMax) {
  const UniformQuantizedType quantized_type =
      CreateI32F32UniformQuantizedType(UnknownLoc::get(&ctx_), ctx_,
                                       /*scale=*/1.0, /*zero_point=*/0);

  EXPECT_EQ(quantized_type.getStorageTypeMin(),
            std::numeric_limits<int32_t>::min());
  EXPECT_EQ(quantized_type.getStorageTypeMax(),
            std::numeric_limits<int32_t>::max());
}

TEST_F(CreateI32F32UniformQuantizedTypeTest, HasScaleAndZeroPointProperlySet) {
  const UniformQuantizedType quantized_type =
      CreateI32F32UniformQuantizedType(UnknownLoc::get(&ctx_), ctx_,
                                       /*scale=*/8.0, /*zero_point=*/1111);

  EXPECT_EQ(quantized_type.getScale(), 8.0);
  EXPECT_EQ(quantized_type.getZeroPoint(), 1111);
}

class CreateI8F32UniformQuantizedPerAxisTypeTest : public ::testing::Test {
 protected:
  CreateI8F32UniformQuantizedPerAxisTypeTest() : ctx_() {
    ctx_.loadDialect<quant::QuantizationDialect>();
  }

  MLIRContext ctx_;
};

TEST_F(CreateI8F32UniformQuantizedPerAxisTypeTest, HasI8StorageType) {
  const UniformQuantizedPerAxisType quantized_type =
      CreateI8F32UniformQuantizedPerAxisType(
          UnknownLoc::get(&ctx_), ctx_,
          /*scales=*/SmallVector<float, 2>{1.0, 1.0},
          /*zero_points=*/SmallVector<int8_t, 2>{0, 0},
          /*quantization_dimension=*/0);

  EXPECT_TRUE(quantized_type.getStorageType().isSignlessInteger(8));
}

TEST_F(CreateI8F32UniformQuantizedPerAxisTypeTest, HasF32ExpressedType) {
  const UniformQuantizedPerAxisType quantized_type =
      CreateI8F32UniformQuantizedPerAxisType(
          UnknownLoc::get(&ctx_), ctx_,
          /*scales=*/SmallVector<float, 2>{1.0, 1.0},
          /*zero_points=*/SmallVector<int8_t, 2>{0, 0},
          /*quantization_dimension=*/0);

  EXPECT_TRUE(quantized_type.getExpressedType().isF32());
}

TEST_F(CreateI8F32UniformQuantizedPerAxisTypeTest, IsSigned) {
  const UniformQuantizedPerAxisType quantized_type =
      CreateI8F32UniformQuantizedPerAxisType(
          UnknownLoc::get(&ctx_), ctx_,
          /*scales=*/SmallVector<float, 2>{1.0, 1.0},
          /*zero_points=*/SmallVector<int8_t, 2>{0, 0},
          /*quantization_dimension=*/0);

  EXPECT_TRUE(quantized_type.isSigned());
}

TEST_F(CreateI8F32UniformQuantizedPerAxisTypeTest,
       StorageTypeMinMaxEqualToI8MinMax) {
  const UniformQuantizedPerAxisType quantized_type =
      CreateI8F32UniformQuantizedPerAxisType(
          UnknownLoc::get(&ctx_), ctx_,
          /*scales=*/SmallVector<float, 2>{1.0, 1.0},
          /*zero_points=*/SmallVector<int8_t, 2>{0, 0},
          /*quantization_dimension=*/0);

  EXPECT_EQ(quantized_type.getStorageTypeMin(), -128);
  EXPECT_EQ(quantized_type.getStorageTypeMax(), 127);
}

TEST_F(CreateI8F32UniformQuantizedPerAxisTypeTest,
       HasQuantizationDimensionProperlySet) {
  const UniformQuantizedPerAxisType quantized_type =
      CreateI8F32UniformQuantizedPerAxisType(
          UnknownLoc::get(&ctx_), ctx_,
          /*scales=*/SmallVector<float, 2>{1.0, 1.0},
          /*zero_points=*/SmallVector<int8_t, 2>{0, 0},
          /*quantization_dimension=*/3);

  EXPECT_EQ(quantized_type.getQuantizedDimension(), 3);
}

TEST_F(CreateI8F32UniformQuantizedPerAxisTypeTest,
       HasScaleAndZeroPointProperlySet) {
  const UniformQuantizedPerAxisType quantized_type =
      CreateI8F32UniformQuantizedPerAxisType(
          UnknownLoc::get(&ctx_), ctx_,
          /*scales=*/SmallVector<float, 2>{8.0, 9.0},
          /*zero_points=*/SmallVector<int8_t, 2>{98, 99},
          /*quantization_dimension=*/0);

  EXPECT_THAT(quantized_type.getScales(), ElementsAreArray({8.0, 9.0}));
  EXPECT_THAT(quantized_type.getZeroPoints(), ElementsAreArray({98, 99}));
}

class IsI8F32UniformQuantizedTypeTest : public ::testing::Test {
 protected:
  IsI8F32UniformQuantizedTypeTest() {
    ctx_.loadDialect<quant::QuantizationDialect>();
  }

  MLIRContext ctx_;
  OpBuilder builder_{&ctx_};
};

TEST_F(IsI8F32UniformQuantizedTypeTest, IsI8F32UniformQuantizedType) {
  const UniformQuantizedType qi8_type = quant::UniformQuantizedType::get(
      /*flags=*/0, builder_.getI8Type(), builder_.getF32Type(), /*scale=*/1.0,
      /*zeroPoint=*/0, /*storageTypeMin=*/0, /*storageTypeMax=*/255);
  EXPECT_TRUE(IsI8F32UniformQuantizedType(qi8_type));
}

TEST_F(IsI8F32UniformQuantizedTypeTest, IsQuantizedType) {
  const UniformQuantizedType qi8_type = quant::UniformQuantizedType::get(
      /*flags=*/0, builder_.getI8Type(), builder_.getF32Type(), /*scale=*/1.0,
      /*zeroPoint=*/0, /*storageTypeMin=*/0, /*storageTypeMax=*/255);
  EXPECT_THAT(qi8_type.dyn_cast_or_null<UniformQuantizedType>(), NotNull());
}

TEST_F(IsI8F32UniformQuantizedTypeTest, IsStorageTypeI8) {
  const UniformQuantizedType qi8_type = quant::UniformQuantizedType::get(
      /*flags=*/0, builder_.getI8Type(), builder_.getF32Type(), /*scale=*/1.0,
      /*zeroPoint=*/0, /*storageTypeMin=*/0, /*storageTypeMax=*/255);
  EXPECT_TRUE(IsStorageTypeI8(qi8_type));
}

TEST_F(IsI8F32UniformQuantizedTypeTest, IsExpressedTypeF32) {
  const UniformQuantizedType qi8_type = quant::UniformQuantizedType::get(
      /*flags=*/0, builder_.getI8Type(), builder_.getF32Type(), /*scale=*/1.0,
      /*zeroPoint=*/0, /*storageTypeMin=*/0, /*storageTypeMax=*/255);
  EXPECT_TRUE(IsExpressedTypeF32(qi8_type));
}

class IsI8F32UniformQuantizedPerAxisTypeTest : public ::testing::Test {
 protected:
  IsI8F32UniformQuantizedPerAxisTypeTest() {
    ctx_.loadDialect<quant::QuantizationDialect>();
  }

  MLIRContext ctx_;
  OpBuilder builder_{&ctx_};
};

TEST_F(IsI8F32UniformQuantizedPerAxisTypeTest,
       IsI8F32UniformQuantizedPerAxisType) {
  const UniformQuantizedPerAxisType qi8_per_axis_type =
      quant::UniformQuantizedPerAxisType::get(
          /*flags=*/0, builder_.getI8Type(), builder_.getF32Type(),
          /*scales=*/{1.0},
          /*zeroPoints=*/{0}, /*quantizedDimension=*/0, /*storageTypeMin=*/0,
          /*storageTypeMax=*/255);
  EXPECT_TRUE(IsI8F32UniformQuantizedPerAxisType(qi8_per_axis_type));
  EXPECT_FALSE(IsI8F32UniformQuantizedType(qi8_per_axis_type));
}

TEST_F(IsI8F32UniformQuantizedTypeTest, IsQuantizedPerAxisType) {
  const UniformQuantizedPerAxisType qi8_per_axis_type =
      quant::UniformQuantizedPerAxisType::get(
          /*flags=*/0, builder_.getI8Type(), builder_.getF32Type(),
          /*scales=*/{1.0},
          /*zeroPoints=*/{0}, /*quantizedDimension=*/0, /*storageTypeMin=*/0,
          /*storageTypeMax=*/255);
  EXPECT_THAT(qi8_per_axis_type.dyn_cast_or_null<UniformQuantizedPerAxisType>(),
              NotNull());
}

TEST_F(IsI8F32UniformQuantizedPerAxisTypeTest, IsStorageTypeI8) {
  const UniformQuantizedPerAxisType qi8_per_axis_type =
      quant::UniformQuantizedPerAxisType::get(
          /*flags=*/0, builder_.getI8Type(), builder_.getF32Type(),
          /*scales=*/{1.0},
          /*zeroPoints=*/{0}, /*quantizedDimension=*/0, /*storageTypeMin=*/0,
          /*storageTypeMax=*/255);
  EXPECT_TRUE(IsStorageTypeI8(qi8_per_axis_type));
}

TEST_F(IsI8F32UniformQuantizedPerAxisTypeTest, IsExpressedTypeF32) {
  const UniformQuantizedPerAxisType qi8_per_axis_type =
      quant::UniformQuantizedPerAxisType::get(
          /*flags=*/0, builder_.getI8Type(), builder_.getF32Type(),
          /*scales=*/{1.0},
          /*zeroPoints=*/{0}, /*quantizedDimension=*/0, /*storageTypeMin=*/0,
          /*storageTypeMax=*/255);
  EXPECT_TRUE(IsExpressedTypeF32(qi8_per_axis_type));
}

class IsI32F32UniformQuantizedTypeTest : public ::testing::Test {
 protected:
  IsI32F32UniformQuantizedTypeTest() {
    ctx_.loadDialect<quant::QuantizationDialect>();
  }

  MLIRContext ctx_;
  OpBuilder builder_{&ctx_};
};

TEST_F(IsI32F32UniformQuantizedTypeTest, IsI32F32UniformQuantizedType) {
  const UniformQuantizedType qi32_type = quant::UniformQuantizedType::get(
      /*flags=*/0, builder_.getI32Type(), builder_.getF32Type(), /*scale=*/1.0,
      /*zeroPoint=*/0, /*storageTypeMin=*/0, /*storageTypeMax=*/255);
  EXPECT_TRUE(IsI32F32UniformQuantizedType(qi32_type));
}

TEST_F(IsI32F32UniformQuantizedTypeTest, IsQuantizedType) {
  const UniformQuantizedType qi32_type = quant::UniformQuantizedType::get(
      /*flags=*/0, builder_.getI8Type(), builder_.getF32Type(), /*scale=*/1.0,
      /*zeroPoint=*/0, /*storageTypeMin=*/0, /*storageTypeMax=*/255);
  EXPECT_THAT(qi32_type.dyn_cast_or_null<UniformQuantizedType>(), NotNull());
}

TEST_F(IsI32F32UniformQuantizedTypeTest, IsStorageTypeI32) {
  const UniformQuantizedType qi32_type = quant::UniformQuantizedType::get(
      /*flags=*/0, builder_.getI32Type(), builder_.getF32Type(), /*scale=*/1.0,
      /*zeroPoint=*/0, /*storageTypeMin=*/0, /*storageTypeMax=*/255);
  EXPECT_TRUE(IsStorageTypeI32(qi32_type));
}

TEST_F(IsI32F32UniformQuantizedTypeTest, IsExpressedTypeF32) {
  const UniformQuantizedType qi32_per_axis_type =
      quant::UniformQuantizedType::get(
          /*flags=*/0, builder_.getI8Type(), builder_.getF32Type(),
          /*scale=*/1.0,
          /*zeroPoint=*/0, /*storageTypeMin=*/0, /*storageTypeMax=*/255);
  EXPECT_TRUE(IsExpressedTypeF32(qi32_per_axis_type));
}

class IsSupportedByTfliteQuantizeOrDequantizeOpsTest : public ::testing::Test {
 protected:
  IsSupportedByTfliteQuantizeOrDequantizeOpsTest() {
    ctx_.loadDialect<quant::QuantizationDialect>();
  }

  MLIRContext ctx_;
  OpBuilder builder_{&ctx_};
};

TEST_F(IsSupportedByTfliteQuantizeOrDequantizeOpsTest, IsI8) {
  auto qi8_type = quant::UniformQuantizedType::get(
      /*flags=*/0, builder_.getIntegerType(8, /*isSigned=*/true),
      builder_.getF32Type(), /*scale=*/1.0,
      /*zeroPoint=*/0, /*storageTypeMin=*/0, /*storageTypeMax=*/255);
  EXPECT_TRUE(IsSupportedByTfliteQuantizeOrDequantizeOps(
      dyn_cast_or_null<IntegerType>(qi8_type.getStorageType())));
}

TEST_F(IsSupportedByTfliteQuantizeOrDequantizeOpsTest, IsI16) {
  auto qi16_type = quant::UniformQuantizedType::get(
      /*flags=*/0, builder_.getIntegerType(16, /*isSigned=*/true),
      builder_.getF32Type(), /*scale=*/1.0,
      /*zeroPoint=*/0, /*storageTypeMin=*/0, /*storageTypeMax=*/255);
  EXPECT_TRUE(IsSupportedByTfliteQuantizeOrDequantizeOps(
      dyn_cast_or_null<IntegerType>(qi16_type.getStorageType())));
}

TEST_F(IsSupportedByTfliteQuantizeOrDequantizeOpsTest, IsUI8) {
  auto qi8_type = quant::UniformQuantizedType::get(
      /*flags=*/0, builder_.getIntegerType(8, /*isSigned=*/false),
      builder_.getF32Type(), /*scale=*/1.0,
      /*zeroPoint=*/0, /*storageTypeMin=*/0, /*storageTypeMax=*/255);
  EXPECT_TRUE(IsSupportedByTfliteQuantizeOrDequantizeOps(
      dyn_cast_or_null<IntegerType>(qi8_type.getStorageType())));
}

}  // namespace
}  // namespace quant
}  // namespace mlir
