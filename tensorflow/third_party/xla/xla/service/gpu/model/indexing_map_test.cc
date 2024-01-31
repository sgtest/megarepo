/* Copyright 2023 The OpenXLA Authors.

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

#include "xla/service/gpu/model/indexing_map.h"

#include <gmock/gmock.h>
#include <gtest/gtest.h>
#include "mlir/IR/AffineExpr.h"  // from @llvm-project
#include "mlir/IR/AffineMap.h"  // from @llvm-project
#include "mlir/IR/MLIRContext.h"  // from @llvm-project
#include "xla/service/gpu/model/affine_map_printer.h"
#include "xla/service/gpu/model/indexing_test_utils.h"
#include "xla/tests/hlo_test_base.h"
#include "tsl/platform/test.h"

namespace xla {
namespace gpu {
namespace {

class IndexingMapTest : public HloTestBase {
 public:
  mlir::MLIRContext mlir_context_;
  AffineMapPrinter printer_;
};

TEST_F(IndexingMapTest, Composition_Permutation) {
  IndexingMap producer = IndexingMap::FromTensorSizes(
      ParseAffineMap("(d0, d1)[s0, s1] -> (d1, d0, s1, s0)", &mlir_context_),
      {4, 4}, {2, 2});

  IndexingMap consumer = IndexingMap::FromTensorSizes(
      ParseAffineMap("(d0)[s0] -> (d0, s0)", &mlir_context_), {4}, {4});

  auto composed = ComposeIndexingMaps(producer, consumer);
  EXPECT_THAT(composed, MatchIndexingMap(R"(
                          (d0)[s0, s1, s2] -> (s2, d0, s1, s0)
                          domain:
                          d0 in [0, 3]
                          s0 in [0, 1]
                          s1 in [0, 1]
                          s2 in [0, 3]
                        )"));
}

TEST_F(IndexingMapTest, Composition_RestrictedRange) {
  IndexingMap producer = IndexingMap::FromTensorSizes(
      ParseAffineMap("(d0, d1)[s0, s1] -> (d1, d0, s1, s0)", &mlir_context_),
      {5, 6}, {7, 2});

  IndexingMap consumer = IndexingMap::FromTensorSizes(
      ParseAffineMap("(d0)[s0] -> (d0, s0)", &mlir_context_), {10}, {8});

  auto composed = ComposeIndexingMaps(producer, consumer);
  EXPECT_THAT(composed, MatchIndexingMap(R"(
                          (d0)[s0, s1, s2] -> (s2, d0, s1, s0)
                          domain:
                          d0 in [0, 4]
                          s0 in [0, 5]
                          s1 in [0, 1]
                          s2 in [0, 7]
                        )"));
}

TEST_F(IndexingMapTest, ConstraintRangeSimplification_Sum) {
  IndexingMap indexing_map = IndexingMap::FromTensorSizes(
      ParseAffineMap("(d0) -> (d0)", &mlir_context_), {100}, {});

  indexing_map.AddConstraint(ParseAffineExpr("(d0 mod 8) + 5", &mlir_context_),
                             Range{50, 54});

  EXPECT_THAT(indexing_map.ToString(), MatchIndexingString(R"(
                          (d0) -> (d0)
                          domain:
                          d0 in [0, 99]
                          d0 mod 8 in [45, 49]
                        )"));
}

TEST_F(IndexingMapTest,
       ConstraintRangeSimplification_FloorDivPositiveDivisorPositiveBounds) {
  IndexingMap indexing_map = IndexingMap::FromTensorSizes(
      ParseAffineMap("(d0) -> (d0)", &mlir_context_), {100}, {});

  indexing_map.AddConstraint(ParseAffineExpr("d0 floordiv 8", &mlir_context_),
                             Range{5, 11});
  EXPECT_THAT(indexing_map.ToString(), MatchIndexingString(R"(
                          (d0) -> (d0)
                          domain:
                          d0 in [40, 95]
                        )"));
}

TEST_F(IndexingMapTest,
       ConstraintRangeSimplification_FloorDivPositiveDivisorNegativeBounds) {
  IndexingMap indexing_map =
      IndexingMap(ParseAffineMap("(d0)[s0] -> (d0)", &mlir_context_),
                  {Range{0, 99}}, {Range{-99, 99}});

  indexing_map.AddConstraint(ParseAffineExpr("s0 floordiv 3", &mlir_context_),
                             Range{-11, -5});
  EXPECT_THAT(indexing_map.ToString(), MatchIndexingString(R"(
                          (d0)[s0] -> (d0)
                          domain:
                          d0 in [0, 99]
                          s0 in [-33, -13]
                        )"));
}

TEST_F(IndexingMapTest,
       ConstraintRangeSimplification_FloorDivNegativeDivisorNegativeBounds) {
  IndexingMap indexing_map =
      IndexingMap(ParseAffineMap("(d0)[s0] -> (d0)", &mlir_context_),
                  {Range{0, 99}}, {Range{-99, 99}});

  indexing_map.AddConstraint(ParseAffineExpr("s0 floordiv -3", &mlir_context_),
                             Range{-11, -5});
  EXPECT_THAT(indexing_map.ToString(), MatchIndexingString(R"(
                          (d0)[s0] -> (d0)
                          domain:
                          d0 in [0, 99]
                          s0 in [15, 35]
                        )"));
}

TEST_F(IndexingMapTest,
       ConstraintRangeSimplification_MulPositiveMultiplierPositiveBounds) {
  IndexingMap indexing_map = IndexingMap::FromTensorSizes(
      ParseAffineMap("(d0) -> (d0)", &mlir_context_), {100}, {});

  indexing_map.AddConstraint(ParseAffineExpr("d0 * 8", &mlir_context_),
                             Range{14, 33});
  EXPECT_THAT(indexing_map.ToString(), MatchIndexingString(R"(
                          (d0) -> (d0)
                          domain:
                          d0 in [2, 4]
                        )"));
}

TEST_F(IndexingMapTest,
       ConstraintRangeSimplification_MulPositiveMultiplierNegativeBounds) {
  IndexingMap indexing_map =
      IndexingMap(ParseAffineMap("(d0)[s0] -> (d0)", &mlir_context_),
                  {Range{0, 99}}, {Range{-99, 99}});

  indexing_map.AddConstraint(ParseAffineExpr("s0 * 3", &mlir_context_),
                             Range{-11, -5});
  EXPECT_THAT(indexing_map.ToString(), MatchIndexingString(R"(
                          (d0)[s0] -> (d0)
                          domain:
                          d0 in [0, 99]
                          s0 in [-3, -2]
                        )"));
}

TEST_F(IndexingMapTest,
       ConstraintRangeSimplification_MulNegativeMultiplierNegativeBounds) {
  IndexingMap indexing_map =
      IndexingMap(ParseAffineMap("(d0)[s0] -> (d0)", &mlir_context_),
                  {Range{0, 99}}, {Range{-99, 99}});

  indexing_map.AddConstraint(ParseAffineExpr("s0 * -3", &mlir_context_),
                             Range{-11, -5});
  EXPECT_THAT(indexing_map.ToString(), MatchIndexingString(R"(
                          (d0)[s0] -> (d0)
                          domain:
                          d0 in [0, 99]
                          s0 in [2, 3]
                        )"));
}

TEST_F(IndexingMapTest, AffineMapSimplification_ConstantDims) {
  IndexingMap indexing_map = IndexingMap(
      ParseAffineMap("(d0) -> (d0)", &mlir_context_), {Range{5, 5}}, {});
  indexing_map.Simplify();
  EXPECT_THAT(indexing_map.ToString(printer_), MatchIndexingString(R"(
                                                  (d0) -> (5)
                                                  domain:
                                                  d0 in [5, 5]
                                                )"));
}

TEST_F(IndexingMapTest,
       AffineMapSimplification_DivsAndModsIfSmallerThanDivisor) {
  auto serialized_map = "(d0, d1) -> (d0 + d1 floordiv 16, d1 mod 16)";
  IndexingMap indexing_map = IndexingMap::FromTensorSizes(
      ParseAffineMap(serialized_map, &mlir_context_), {8, 16}, {});
  indexing_map.Simplify();
  EXPECT_THAT(indexing_map.ToString(printer_), MatchIndexingString(R"(
                                                  (d0, d1) -> (d0, d1)
                                                  domain:
                                                  d0 in [0, 7]
                                                  d1 in [0, 15]
                                                )"));
}

TEST_F(IndexingMapTest, AffineMapSimplification_DivsAndModsWithMultipliers) {
  auto serialized_map =
      "(d0, d1, d2) -> ((d0 * 100 + d1 * 10 + d2) floordiv 100, "
      "((d0 * 100 + d1 * 10 + d2) mod 100) floordiv 10, "
      "d2 mod 10)";

  IndexingMap indexing_map = IndexingMap::FromTensorSizes(
      ParseAffineMap(serialized_map, &mlir_context_), {9, 9, 9}, {});
  indexing_map.Simplify();

  EXPECT_THAT(indexing_map.ToString(printer_), MatchIndexingString(R"(
                                                  (d0, d1, d2) -> (d0, d1, d2)
                                                  domain:
                                                  d0 in [0, 8]
                                                  d1 in [0, 8]
                                                  d2 in [0, 8]
                                                )"));
}

TEST_F(IndexingMapTest,
       AffineMapSimplification_DivsAndModsWithDivisibleMultipliers) {
  auto serialized_map =
      "(d0, d1, d2) -> ((d0 * 16 + d1 * 4 + d2) floordiv 8, "
      "                 (d0 * 16 + d1 * 4 + d2) mod 8)";

  IndexingMap indexing_map = IndexingMap::FromTensorSizes(
      ParseAffineMap(serialized_map, &mlir_context_), {10, 10, 10}, {});
  indexing_map.Simplify();
  EXPECT_THAT(indexing_map.ToString(printer_), MatchIndexingString(R"(
    (d0, d1, d2) -> (d0 * 2 + (d1 * 4 + d2) floordiv 8, (d1 * 4 + d2) mod 8)
    domain:
    d0 in [0, 9]
    d1 in [0, 9]
    d2 in [0, 9]
  )"));
}

TEST_F(IndexingMapTest, AffineMapSimplification_DivsAndModsWithReverse) {
  auto serialized_map =
      "(d0, d1) -> (-((d0 * -11 - d1 + 109) floordiv 11) + 9, "
      "d0 * 11 + d1 + ((d0 * -11 - d1 + 109) floordiv 11) * 11 - 99)";
  IndexingMap indexing_map = IndexingMap::FromTensorSizes(
      ParseAffineMap(serialized_map, &mlir_context_), {8, 9}, {});
  indexing_map.Simplify();
  EXPECT_THAT(indexing_map.ToString(printer_), MatchIndexingString(R"(
                                                 (d0, d1) -> (d0, d1)
                                                 domain:
                                                 d0 in [0, 7]
                                                 d1 in [0, 8]
                                               )"));
}

TEST_F(IndexingMapTest, RangeEvaluatorTest) {
  RangeEvaluator range_evaluator(
      {Range{0, 9}, Range{-10, -1}, Range{-1, 2}, Range{0, 0}}, {},
      &mlir_context_);
  mlir::AffineExpr d0, d1, d2, d3;
  bindDims(&mlir_context_, d0, d1, d2, d3);

  // d0 is always positive.
  EXPECT_TRUE(range_evaluator.IsAlwaysPositiveOrZero(d0));
  EXPECT_FALSE(range_evaluator.IsAlwaysNegativeOrZero(d0));

  // d1 is always negative.
  EXPECT_FALSE(range_evaluator.IsAlwaysPositiveOrZero(d1));
  EXPECT_TRUE(range_evaluator.IsAlwaysNegativeOrZero(d1));

  // d2 is sometimes positive and sometimes negative.
  EXPECT_FALSE(range_evaluator.IsAlwaysPositiveOrZero(d2));
  EXPECT_FALSE(range_evaluator.IsAlwaysNegativeOrZero(d2));

  // d3 is always 0.
  EXPECT_TRUE(range_evaluator.IsAlwaysPositiveOrZero(d3));
  EXPECT_TRUE(range_evaluator.IsAlwaysNegativeOrZero(d3));
}

// TODO(b/313840171): Simplify `(d1 * 4 + d2) floordiv 8` to `d1 floordiv 2`.

// TODO(b/313840171): Simplify `(d0 * 8 + d1) floordiv 16` to `d0 floordiv 2`.

// TODO(b/313840171): Simplify `((d0 * 8 + d1) mod 16) floordiv 4` to
// `((d0 * 8 + d1) floordiv 4) mod 4` to `(d0 * 2 + d1 floordiv 4) mod 4`.

}  // namespace
}  // namespace gpu
}  // namespace xla
