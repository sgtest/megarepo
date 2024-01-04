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

#include "xla/service/gpu/model/tile_analysis.h"

#include <cstdint>
#include <optional>
#include <vector>

#include <gmock/gmock.h>
#include <gtest/gtest.h>
#include "absl/strings/str_cat.h"
#include "absl/strings/string_view.h"
#include "mlir/IR/MLIRContext.h"  // from @llvm-project
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/hlo/ir/hlo_opcode.h"
#include "xla/status_macros.h"
#include "xla/statusor.h"
#include "xla/test_helpers.h"
#include "xla/tests/hlo_test_base.h"
#include "tsl/platform/status.h"
#include "tsl/platform/statusor.h"
#include "tsl/platform/test.h"

namespace xla {
namespace gpu {
namespace {

using ::testing::AllOf;
using ::testing::DescribeMatcher;
using ::testing::Each;
using ::testing::ElementsAre;
using ::testing::ElementsAreArray;
using ::testing::Eq;
using ::testing::ExplainMatchResult;
using ::testing::FieldsAre;
using ::testing::HasSubstr;
using ::testing::IsEmpty;
using ::testing::Optional;
using ::testing::Pair;
using ::testing::SizeIs;
using ::testing::StrEq;
using ::testing::UnorderedElementsAre;

MATCHER_P2(MatchRange, lower_bound, upper_bound,
           absl::StrCat(negation ? "equals " : "doesn't equal ", "range [",
                        lower_bound, ", ", upper_bound, "]")) {
  return ExplainMatchResult(FieldsAre(lower_bound, upper_bound), arg,
                            result_listener);
}

MATCHER_P3(MatchIndexingMap, affine_map_string, dim_ranges, symbol_ranges, "") {
  return ExplainMatchResult(HasSubstr(affine_map_string),
                            ToString(arg.affine_map), result_listener) &&
         ExplainMatchResult(dim_ranges, arg.domain.dimension_ranges,
                            result_listener) &&
         ExplainMatchResult(symbol_ranges, arg.domain.symbol_ranges,
                            result_listener);
}

MATCHER_P2(MatchInstrIndexing, operand_id, indexing_map_matchers, "") {
  return ExplainMatchResult(Eq(operand_id), arg.operand_id, result_listener) &&
         ExplainMatchResult(indexing_map_matchers, arg.indexing_maps,
                            result_listener);
}

MATCHER_P4(
    MatchSymbolicTile, affine_map_string, sizes, max_sizes,
    max_strides_and_offsets,
    absl::StrCat(
        negation ? "equals " : "doesn't equal ", "symbolic tile ",
        affine_map_string, " where sizes_ ",
        DescribeMatcher<std::vector<std::optional<int64_t>>>(sizes),
        ", max_sizes_ ", DescribeMatcher<std::vector<int64_t>>(max_sizes),
        " and ", "max_strides_and_offsets_ ",
        DescribeMatcher<std::vector<int64_t>>(max_strides_and_offsets))) {
  return ExplainMatchResult(StrEq(affine_map_string),
                            ToString(arg.affine_map()), result_listener) &&
         ExplainMatchResult(sizes, arg.sizes(), result_listener) &&
         ExplainMatchResult(max_sizes, arg.max_sizes(), result_listener) &&
         ExplainMatchResult(max_strides_and_offsets,
                            arg.max_strides_and_offsets(), result_listener);
}

class TileAnalysisTest : public HloTestBase {
 public:
  StatusOr<HloInstructionIndexing> GetOutputToInputIndexingForEntryComputation(
      absl::string_view hlo_string, int output_id = 0) {
    TF_ASSIGN_OR_RETURN(auto module, ParseAndReturnVerifiedModule(hlo_string));
    HloInstruction* root = module->entry_computation()->root_instruction();

    for (auto* operand : root->operands()) {
      TF_RET_CHECK(operand->opcode() == HloOpcode::kParameter ||
                   operand->opcode() == HloOpcode::kConstant)
          << "If there are multiple instructions, they need to be wrapped in a "
             "fusion.";
    }
    return ComputeOutputToInputIndexing(root, output_id, &mlir_context_);
  }

  StatusOr<HloInstructionIndexing> GetInputToOutputIndexingForEntryComputation(
      absl::string_view hlo_string, int input_id = 0) {
    TF_ASSIGN_OR_RETURN(auto module, ParseAndReturnVerifiedModule(hlo_string));
    HloInstruction* root = module->entry_computation()->root_instruction();

    for (auto* operand : root->operands()) {
      TF_RET_CHECK(operand->opcode() == HloOpcode::kParameter ||
                   operand->opcode() == HloOpcode::kConstant)
          << "If there are multiple instructions, they need to be wrapped in a "
             "fusion.";
    }
    return ComputeInputToOutputIndexing(root, input_id, &mlir_context_);
  }
  mlir::MLIRContext mlir_context_;
};

TEST_F(TileAnalysisTest, FuseProducerConsumerOutputToInputIndexing) {
  TF_ASSERT_OK_AND_ASSIGN(auto module, ParseAndReturnVerifiedModule(R"(
    HloModule m
    ENTRY e {
      p0 = f32[1000, 1000] parameter(0)
      transpose_p0 = f32[1000, 1000]{0, 1} transpose(p0), dimensions={1, 0}
      ROOT a0 = f32[1000, 1000] add(p0, transpose_p0)
    }
  )"));
  const HloInstruction* root = module->entry_computation()->root_instruction();
  const HloInstruction* parameter = root->operand(0);
  const HloInstruction* transpose = root->operand(1);

  TF_ASSERT_OK_AND_ASSIGN(
      auto root_indexing,
      ComputeOutputToInputIndexing(root, /*output_id=*/0, &mlir_context_));

  auto grouped_by_key = GroupIndexingMapsByProducers(root_indexing, root);

  EXPECT_THAT(
      grouped_by_key,
      UnorderedElementsAre(
          Pair(parameter,
               ElementsAre(MatchIndexingMap(
                   "(d0, d1) -> (d0, d1)",
                   ElementsAre(MatchRange(0, 1000), MatchRange(0, 1000)),
                   IsEmpty()))),
          Pair(transpose,
               ElementsAre(MatchIndexingMap(
                   "(d0, d1) -> (d0, d1)",
                   ElementsAre(MatchRange(0, 1000), MatchRange(0, 1000)),
                   IsEmpty())))));

  TF_CHECK_OK(FuseProducerConsumerOutputToInputIndexing(
      transpose, &grouped_by_key, &mlir_context_));
  EXPECT_THAT(
      grouped_by_key,
      UnorderedElementsAre(
          Pair(parameter, UnorderedElementsAre(
                              MatchIndexingMap("(d0, d1) -> (d0, d1)",
                                               ElementsAre(MatchRange(0, 1000),
                                                           MatchRange(0, 1000)),
                                               IsEmpty()),
                              MatchIndexingMap("(d0, d1) -> (d1, d0)",
                                               ElementsAre(MatchRange(0, 1000),
                                                           MatchRange(0, 1000)),
                                               IsEmpty())))));
}

TEST_F(TileAnalysisTest, ElementwiseOp) {
  auto ir = R"(
    HloModule m
    ENTRY e {
      p0 = f32[10, 20] parameter(0)
      p1 = f32[10, 20] parameter(1)
      ROOT add0 = f32[10, 20] add(p0, p1)
    }
  )";
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(ir));
  EXPECT_THAT(input_indexing.indexing_maps,
              UnorderedElementsAre(
                  Pair(0, ElementsAre(MatchIndexingMap(
                              "(d0, d1) -> (d0, d1)",
                              ElementsAre(MatchRange(0, 10), MatchRange(0, 20)),
                              IsEmpty()))),
                  Pair(1, ElementsAre(MatchIndexingMap(
                              "(d0, d1) -> (d0, d1)",
                              ElementsAre(MatchRange(0, 10), MatchRange(0, 20)),
                              IsEmpty())))));
  TF_ASSERT_OK_AND_ASSIGN(
      auto output_indexing,
      GetInputToOutputIndexingForEntryComputation(ir, /*input_id=*/0));
  EXPECT_THAT(output_indexing.indexing_maps,
              UnorderedElementsAre(
                  Pair(0, ElementsAre(MatchIndexingMap(
                              "(d0, d1) -> (d0, d1)",
                              ElementsAre(MatchRange(0, 10), MatchRange(0, 20)),
                              IsEmpty())))));
  TF_ASSERT_OK_AND_ASSIGN(
      auto output_indexing1,
      GetInputToOutputIndexingForEntryComputation(ir, /*input_id=*/1));
  EXPECT_THAT(output_indexing1.indexing_maps,
              UnorderedElementsAre(
                  Pair(0, ElementsAre(MatchIndexingMap(
                              "(d0, d1) -> (d0, d1)",
                              ElementsAre(MatchRange(0, 10), MatchRange(0, 20)),
                              IsEmpty())))));
}

TEST_F(TileAnalysisTest, BitcastIsReshape) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      p0 = f32[4, 32] parameter(0)
      ROOT bitcast = f32[4, 8, 4] bitcast(p0)
    }
  )"));
  EXPECT_THAT(
      input_indexing.indexing_maps,
      UnorderedElementsAre(Pair(
          0,
          ElementsAre(MatchIndexingMap(
              "(d0, d1, d2) -> (d0, d1 * 4 + d2)",
              ElementsAre(MatchRange(0, 4), MatchRange(0, 8), MatchRange(0, 4)),
              IsEmpty())))));
}

TEST_F(TileAnalysisTest, BitcastIsTranspose) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      p0 = f32[3, 12288, 6, 128] parameter(0)
      ROOT bitcast = f32[3, 6, 128, 12288] {2, 1, 3, 0} bitcast(p0)
    }
  )"));
  EXPECT_THAT(input_indexing.indexing_maps,
              UnorderedElementsAre(Pair(
                  0, ElementsAre(MatchIndexingMap(
                         "(d0, d1, d2, d3) -> (d0, d3, d1, d2)",
                         ElementsAre(MatchRange(0, 3), MatchRange(0, 6),
                                     MatchRange(0, 128), MatchRange(0, 12288)),
                         IsEmpty())))));
}

TEST_F(TileAnalysisTest, BitcastIsTransposeReshapeTranspose) {
  auto ir = R"(
    HloModule m
    ENTRY e {
      p0 = f32[16, 17, 3] parameter(0)
      ROOT bitcast = f32[51, 16] {0, 1} bitcast(p0)
    }
  )";
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(ir));
  EXPECT_THAT(input_indexing.indexing_maps,
              UnorderedElementsAre(
                  Pair(0, ElementsAre(MatchIndexingMap(
                              "(d0, d1) -> (d1, d0 floordiv 3, d0 mod 3)",
                              ElementsAre(MatchRange(0, 51), MatchRange(0, 16)),
                              IsEmpty())))));
  TF_ASSERT_OK_AND_ASSIGN(auto output_indexing,
                          GetInputToOutputIndexingForEntryComputation(ir));
  EXPECT_THAT(output_indexing.indexing_maps,
              UnorderedElementsAre(
                  Pair(0, ElementsAre(MatchIndexingMap(
                              "(d0, d1, d2) -> (d1 * 3 + d2, d0)",
                              ElementsAre(MatchRange(0, 16), MatchRange(0, 17),
                                          MatchRange(0, 3)),
                              IsEmpty())))));
}

TEST_F(TileAnalysisTest, BroadcastOp) {
  auto ir = R"(
    HloModule m
    ENTRY e {
      p0 = f32[20] parameter(0)
      ROOT bc0 = f32[10, 20, 30] broadcast(p0), dimensions={1}
    }
  )";
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(ir));
  EXPECT_THAT(input_indexing.indexing_maps,
              UnorderedElementsAre(
                  Pair(0, ElementsAre(MatchIndexingMap(
                              "(d0, d1, d2) -> (d1)",
                              ElementsAre(MatchRange(0, 10), MatchRange(0, 20),
                                          MatchRange(0, 30)),
                              IsEmpty())))));

  TF_ASSERT_OK_AND_ASSIGN(auto output_indexing,
                          GetInputToOutputIndexingForEntryComputation(ir));
  EXPECT_THAT(
      output_indexing.indexing_maps,
      UnorderedElementsAre(Pair(
          0, ElementsAre(MatchIndexingMap(
                 "(d0)[s0, s1] -> (s0, d0, s1)", ElementsAre(MatchRange(0, 20)),
                 ElementsAre(MatchRange(0, 10), MatchRange(0, 30)))))));
}

TEST_F(TileAnalysisTest, ConstantOp) {
  auto ir = R"(
    HloModule m
    ENTRY e {
      ROOT c1 = bf16[17, 22] constant(1)
    }
  )";
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(ir));
  EXPECT_THAT(input_indexing.indexing_maps, IsEmpty());
}

TEST_F(TileAnalysisTest, FusionOpWithSingleBinaryOp) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    f {
      p0 = f32[100] parameter(0)
      p1 = f32[100] parameter(1)
      ROOT a0 = f32[100] add(p0, p1)
    }
    ENTRY e {
      p0 = f32[100] parameter(0)
      p1 = f32[100] parameter(1)
      ROOT fusion = f32[100] fusion(p0, p1), kind=kLoop, calls=f
    }
  )"));
  EXPECT_THAT(input_indexing.indexing_maps,
              UnorderedElementsAre(
                  Pair(0, ElementsAre(MatchIndexingMap(
                              "(d0) -> (d0)", ElementsAre(MatchRange(0, 100)),
                              IsEmpty()))),
                  Pair(1, ElementsAre(MatchIndexingMap(
                              "(d0) -> (d0)", ElementsAre(MatchRange(0, 100)),
                              IsEmpty())))));
}

TEST_F(TileAnalysisTest, FusionOpWithDot) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    f {
      p0 = s8[3,12288,6,128]{3,2,1,0} parameter(0)
      bitcast1 = s8[3,6,128,12288]{2,1,3,0} bitcast(p0)
      copy1 = s8[3,6,128,12288]{3,2,1,0} copy(bitcast1)
      bitcast2 = s8[2304,12288]{1,0} bitcast(copy1)
      convert1 = bf16[2304,12288]{1,0} convert(bitcast2)
      bitcast3 = bf16[2304,16,768]{2,1,0} bitcast(convert1)
      p3 = bf16[16,12288]{1,0} parameter(3)
      convert2 = f32[16,12288]{1,0} convert(p3)
      p4 = bf16[16,12288]{1,0} parameter(4)
      convert3 = f32[16,12288]{1,0} convert(p4)
      add1 = f32[16,12288]{1,0} add(convert2, convert3)
      p2 = bf16[16]{0} parameter(2)
      convert15 = f32[16]{0} convert(p2)
      rsqrt = f32[16]{0} rsqrt(convert15)
      convert4 = bf16[16]{0} convert(rsqrt)
      bcast1 = bf16[16,12288]{1,0} broadcast(convert4), dimensions={0}
      convert5 = f32[16,12288]{1,0} convert(bcast1)
      multiply1 = f32[16,12288]{1,0} multiply(add1, convert5)
      p1 = bf16[12288]{0} parameter(1)
      convert6 = f32[12288]{0} convert(p1)
      c1 = bf16[] constant(1)
      bcast2 = bf16[12288]{0} broadcast(c1), dimensions={}
      convert7 = f32[12288]{0} convert(bcast2)
      add2 = f32[12288]{0} add(convert6, convert7)
      convert8 = bf16[12288]{0} convert(add2)
      bcast3 = bf16[16,12288]{1,0} broadcast(convert8), dimensions={1}
      convert9 = f32[16,12288]{1,0} convert(bcast3)
      multiply2 = f32[16,12288]{1,0} multiply(multiply1, convert9)
      convert10 = bf16[16,12288]{1,0} convert(multiply2)
      bcast4 = bf16[16,16,768]{2,1,0} bitcast(convert10)
      dot = bf16[16,2304,16]{2,1,0} dot(bitcast3, bcast4),
        lhs_batch_dims={1}, lhs_contracting_dims={2},
        rhs_batch_dims={1}, rhs_contracting_dims={2}
      bcast5 = bf16[16,3,6,128,16]{4,3,2,1,0} bitcast(dot)
      copy2 = bf16[16,3,6,128,16]{3,2,4,1,0} copy(bcast5)
      convert13 = f32[16,3,6,128,16]{3,2,4,1,0} convert(copy2)
      p5 = bf16[3,6,128]{2,1,0} parameter(5)
      bcast6 = bf16[3,6,128,16]{2,1,3,0} broadcast(p5), dimensions={0,1,2}
      convert11 = f32[3,6,128,16]{2,1,3,0} convert(bcast6)
      bcast7 = f32[16,3,6,128,16]{3,2,4,1,0} broadcast(convert11),
        dimensions={1,2,3,4}
      multiply3 = f32[16,3,6,128,16]{3,2,4,1,0} multiply(convert13, bcast7)
      convert12 = bf16[16,3,6,128,16]{3,2,4,1,0} convert(multiply3)
      ROOT bcast8 = bf16[16,16,3,1,6,128]{5,4,1,3,2,0} bitcast(convert12)
    }
    ENTRY e {
      p0 = s8[3,12288,6,128]{3,2,1,0} parameter(0)
      p1 = bf16[12288]{0} parameter(1)
      p2 = bf16[16]{0} parameter(2)
      p3 = bf16[16,12288]{1,0} parameter(3)
      p4 = bf16[16,12288]{1,0} parameter(4)
      p5 = bf16[3,6,128]{2,1,0} parameter(5)
      ROOT fusion = bf16[16,16,3,1,6,128]{5,4,1,3,2,0}
        fusion(p0, p1, p2, p3, p4, p5), kind=kLoop, calls=f
    }
  )"));

  EXPECT_THAT(
      input_indexing.indexing_maps,
      UnorderedElementsAre(
          Pair(0, ElementsAre(MatchIndexingMap(
                      "(d0, d1, d2, d3, d4, d5)[s0] -> "
                      "(d2 + d3, d0 * 768 + s0, d4, d5)",
                      ElementsAre(MatchRange(0, 16), MatchRange(0, 16),
                                  MatchRange(0, 3), MatchRange(0, 1),
                                  MatchRange(0, 6), MatchRange(0, 128)),
                      ElementsAre(MatchRange(0, 768))))),
          Pair(1, ElementsAre(MatchIndexingMap(
                      "(d0, d1, d2, d3, d4, d5)[s0] -> (d0 * 768 + s0)",
                      ElementsAre(MatchRange(0, 16), MatchRange(0, 16),
                                  MatchRange(0, 3), MatchRange(0, 1),
                                  MatchRange(0, 6), MatchRange(0, 128)),
                      ElementsAre(MatchRange(0, 768))))),
          Pair(2, ElementsAre(MatchIndexingMap(
                      "(d0, d1, d2, d3, d4, d5) -> (d1)",
                      ElementsAre(MatchRange(0, 16), MatchRange(0, 16),
                                  MatchRange(0, 3), MatchRange(0, 1),
                                  MatchRange(0, 6), MatchRange(0, 128)),
                      IsEmpty()))),
          Pair(3, ElementsAre(MatchIndexingMap(
                      "(d0, d1, d2, d3, d4, d5)[s0] -> (d1, d0 * 768 + s0)",
                      ElementsAre(MatchRange(0, 16), MatchRange(0, 16),
                                  MatchRange(0, 3), MatchRange(0, 1),
                                  MatchRange(0, 6), MatchRange(0, 128)),
                      ElementsAre(MatchRange(0, 768))))),
          Pair(4, ElementsAre(MatchIndexingMap(
                      "(d0, d1, d2, d3, d4, d5)[s0] -> (d1, d0 * 768 + s0)",
                      ElementsAre(MatchRange(0, 16), MatchRange(0, 16),
                                  MatchRange(0, 3), MatchRange(0, 1),
                                  MatchRange(0, 6), MatchRange(0, 128)),
                      ElementsAre(MatchRange(0, 768))))),
          Pair(5, ElementsAre(MatchIndexingMap(
                      "(d0, d1, d2, d3, d4, d5) -> (d2 + d3, d4, d5)",
                      ElementsAre(MatchRange(0, 16), MatchRange(0, 16),
                                  MatchRange(0, 3), MatchRange(0, 1),
                                  MatchRange(0, 6), MatchRange(0, 128)),
                      IsEmpty())))));
}

TEST_F(TileAnalysisTest, FusionOpWithSoftmax) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    add_computation {
      p0 = f32[] parameter(0)
      p1 = f32[] parameter(1)
      ROOT add = f32[] add(p0, p1)
    }
    max_computation {
      p0 = f32[] parameter(0)
      p1 = f32[] parameter(1)
      ROOT max = f32[] maximum(p0, p1)
    }
    softmax {
      p0 = f32[2,65,125]{2,1,0} parameter(0)
      bitcast0 = f32[65,2,125]{2,1,0} bitcast(p0)
      constant_neg_inf_1 = f32[] constant(-inf)
      reduce0 = f32[2,65]{1,0} reduce(p0, constant_neg_inf_1),
        dimensions={2}, to_apply=max_computation
      bitcast1 = f32[130]{0} bitcast(reduce0)
      bcast1 = f32[130,125]{1,0} broadcast(bitcast1), dimensions={0}
      bitcast2 = f32[65,2,125]{2,1,0} bitcast(bcast1)
      subtract0 = f32[65,2,125]{2,1,0} subtract(bitcast0, bitcast2)
      exponential0 = f32[65,2,125]{2,1,0} exponential(subtract0)
      bitcast3 = f32[65,2,125]{2,1,0} bitcast(p0)
      reduce1 = f32[2,65]{1,0} reduce(p0, constant_neg_inf_1),
        dimensions={2}, to_apply=max_computation
      bitcast4 = f32[130]{0} bitcast(reduce1)
      bcast2 = f32[130,125]{1,0} broadcast(bitcast4), dimensions={0}
      bitcast5 = f32[65,2,125]{2,1,0} bitcast(bcast2)
      subtract1 = f32[65,2,125]{2,1,0} subtract(bitcast3, bitcast5)
      exponential1 = f32[65,2,125]{2,1,0} exponential(subtract1)
      constant_zero_1 = f32[] constant(0)
      reduce2 = f32[65,2]{1,0} reduce(exponential1, constant_zero_1),
        dimensions={2}, to_apply=add_computation
      bitcast6 = f32[130]{0} bitcast(reduce2)
      bcast3 = f32[130,125]{1,0} broadcast(bitcast6), dimensions={0}
      bitcast7 = f32[65,2,125]{2,1,0} bitcast(bcast3)
      divide = f32[65,2,125]{2,1,0} divide(exponential0, bitcast7)
      ROOT bitcast8 = f32[2,65,125]{2,1,0} bitcast(divide)
    }
    ENTRY e {
      p0 = f32[2,65,125]{2,1,0} parameter(0)
      ROOT fusion = f32[2,65,125]{2,1,0}
        fusion(p0), kind=kLoop, calls=softmax
    }
  )"));
  EXPECT_THAT(
      input_indexing.indexing_maps,
      UnorderedElementsAre(Pair(
          0,
          UnorderedElementsAre(
              MatchIndexingMap("(d0, d1, d2) -> (d0, d1, d2)",
                               ElementsAre(MatchRange(0, 2), MatchRange(0, 65),
                                           MatchRange(0, 125)),
                               IsEmpty()),
              MatchIndexingMap("(d0, d1, d2)[s0] -> (d0, d1, s0)",
                               ElementsAre(MatchRange(0, 2), MatchRange(0, 65),
                                           MatchRange(0, 125)),
                               ElementsAre(MatchRange(0, 125)))))));
}

TEST_F(TileAnalysisTest, FusionOpTensorPlusTransposedTensor) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    f {
      p0 = f32[1000, 1000] parameter(0)
      transpose_p0 = f32[1000, 1000]{0, 1} transpose(p0), dimensions={1, 0}
      ROOT a0 = f32[1000, 1000] add(p0, transpose_p0)
    }
    ENTRY e {
      p0 = f32[1000,1000] parameter(0)
      ROOT fusion = f32[1000,1000] fusion(p0), kind=kLoop, calls=f
    }
  )"));
  EXPECT_THAT(input_indexing.indexing_maps,
              UnorderedElementsAre(
                  Pair(0, UnorderedElementsAre(
                              MatchIndexingMap("(d0, d1) -> (d1, d0)",
                                               ElementsAre(MatchRange(0, 1000),
                                                           MatchRange(0, 1000)),
                                               IsEmpty()),
                              MatchIndexingMap("(d0, d1) -> (d0, d1)",
                                               ElementsAre(MatchRange(0, 1000),
                                                           MatchRange(0, 1000)),
                                               IsEmpty())))));
}

TEST_F(TileAnalysisTest, FusionExponentialDuplication) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule test_module

    fused_computation {
      p0 = f32[4] parameter(0)
      p1 = f32[4] parameter(1)
      add0 = f32[4] add(p0, p1)
      slice1.0 = f32[3] slice(add0), slice={[0:3]}
      slice1.1 = f32[3] slice(add0), slice={[1:4]}
      add1 = f32[3]{0} add(slice1.0, slice1.1)
      slice2.0 = f32[2] slice(add1), slice={[0:2]}
      slice2.1 = f32[2] slice(add1), slice={[1:3]}
      ROOT add2 = f32[2] add(slice2.0, slice2.1)
    }

    ENTRY entry_computation {
      p0 = f32[4] parameter(0)
      p1 = f32[4] parameter(1)
      ROOT fusion = f32[2] fusion(p0, p1), kind=kLoop,
      calls=fused_computation
    })"));
  EXPECT_THAT(
      input_indexing.indexing_maps,
      UnorderedElementsAre(
          Pair(0,
               UnorderedElementsAre(
                   MatchIndexingMap("(d0) -> (d0)",
                                    ElementsAre(MatchRange(0, 2)), IsEmpty()),
                   MatchIndexingMap("(d0) -> (d0 + 1)",
                                    ElementsAre(MatchRange(0, 2)), IsEmpty()),
                   MatchIndexingMap("(d0) -> (d0 + 2)",
                                    ElementsAre(MatchRange(0, 2)), IsEmpty()))),
          Pair(1,
               UnorderedElementsAre(
                   MatchIndexingMap("(d0) -> (d0)",
                                    ElementsAre(MatchRange(0, 2)), IsEmpty()),
                   MatchIndexingMap("(d0) -> (d0 + 1)",
                                    ElementsAre(MatchRange(0, 2)), IsEmpty()),
                   MatchIndexingMap("(d0) -> (d0 + 2)",
                                    ElementsAre(MatchRange(0, 2)),
                                    IsEmpty())))));
}

TEST_F(TileAnalysisTest, FusionOpWithReduceOfReduce) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    max {
      p0 = f32[] parameter(0)
      p1 = f32[] parameter(1)
      ROOT max = f32[] maximum(p0, p1)
    }
    f {
      p0 = f32[150, 20, 10, 50] parameter(0)
      p0_init = f32[] parameter(1)
      reduce_1 = f32[20, 10] reduce(p0, p0_init),
        dimensions={0, 3}, to_apply=max
      ROOT reduce_2 = f32[10] reduce(reduce_1, p0_init),
        dimensions={0}, to_apply=max
    }
    ENTRY e {
      p0 = f32[150, 20, 10, 50] parameter(0)
      p0_init = f32[] constant(-inf)
      ROOT fusion = f32[10] fusion(p0, p0_init), kind=kLoop, calls=f
    }
  )"));
  EXPECT_THAT(input_indexing.indexing_maps,
              UnorderedElementsAre(
                  Pair(0, ElementsAre(MatchIndexingMap(
                              "(d0)[s0, s1, s2] -> (s0, s2, d0, s1)",
                              ElementsAre(MatchRange(0, 10)),
                              ElementsAre(MatchRange(0, 150), MatchRange(0, 50),
                                          MatchRange(0, 20))))),
                  Pair(1, ElementsAre(MatchIndexingMap(
                              "(d0) -> ()", ElementsAre(MatchRange(0, 10)),
                              IsEmpty())))));
}

TEST_F(TileAnalysisTest, FusionOpWithReduceOfBroadcast) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    max {
      p0 = f32[] parameter(0)
      p1 = f32[] parameter(1)
      ROOT max = f32[] maximum(p0, p1)
    }
    f {
      p0 = f32[15, 20] parameter(0)
      p0_init = f32[] parameter(1)
      p0_bcast = f32[15, 32, 20, 64] broadcast(p0), dimensions={0, 2}

      ROOT reduce_2 = f32[15, 64] reduce(p0_bcast, p0_init),
        dimensions={1, 2}, to_apply=max
    }
    ENTRY e {
      p0 = f32[15, 20] parameter(0)
      p0_init = f32[] constant(-inf)
      ROOT fusion = f32[15, 64] fusion(p0, p0_init), kind=kLoop, calls=f
    }
  )"));
  EXPECT_THAT(input_indexing.indexing_maps,
              UnorderedElementsAre(
                  Pair(0, ElementsAre(MatchIndexingMap(
                              "(d0, d1)[s0] -> (d0, s0)",
                              ElementsAre(MatchRange(0, 15), MatchRange(0, 64)),
                              ElementsAre(MatchRange(0, 20))))),
                  Pair(1, ElementsAre(MatchIndexingMap(
                              "(d0, d1) -> ()",
                              ElementsAre(MatchRange(0, 15), MatchRange(0, 64)),
                              IsEmpty())))));
}

TEST_F(TileAnalysisTest, FusionOpWithTransposeOfTranspose) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    f {
      p0 = f32[20, 10, 50] parameter(0)

      lhs_transpose_1 = f32[10, 20, 50]
             transpose(p0), dimensions={1, 0, 2}
      lhs_e = f32[10, 20, 50] exponential(lhs_transpose_1)
      lhs_transpose_2 = f32[10, 50, 20]
             transpose(lhs_e), dimensions={0, 2, 1}

      rhs_transpose_1 = f32[50, 10, 20]
             transpose(p0), dimensions={2, 1, 0}
      rhs_log = f32[50, 10, 20] exponential(rhs_transpose_1)
      rhs_transpose_2 = f32[10, 50, 20]
             transpose(rhs_log), dimensions={1, 0, 2}

      ROOT add = f32[10, 50, 20] add(lhs_transpose_2, rhs_transpose_2)
    }
    ENTRY e {
      p0 = f32[20, 10, 50] parameter(0)
      ROOT fusion = f32[10, 50, 20] fusion(p0), kind=kLoop, calls=f
    }
  )"));
  EXPECT_THAT(input_indexing.indexing_maps,
              UnorderedElementsAre(
                  Pair(0, ElementsAre(MatchIndexingMap(
                              "(d0, d1, d2) -> (d2, d0, d1)",
                              ElementsAre(MatchRange(0, 10), MatchRange(0, 50),
                                          MatchRange(0, 20)),
                              IsEmpty())))));
}

TEST_F(TileAnalysisTest, FusionOpWithReducedSlice) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    max {
      p0 = f32[] parameter(0)
      p1 = f32[] parameter(1)
      ROOT max = f32[] maximum(p0, p1)
    }
    f {
      p0 = f32[150, 64, 1024] parameter(0)
      p0_init = f32[] parameter(1)
      p0_slice = f32[16, 32, 128] slice(f32[150, 64, 1024] p0),
                slice={[5:21:1], [0:64:2], [50:434:3]}
      ROOT reduce = f32[32] reduce(p0_slice, p0_init),
        dimensions={0, 2}, to_apply=max
    }
    ENTRY e {
      p0 = f32[150, 64, 1024] parameter(0)
      p0_init = f32[] constant(-inf)
      ROOT fusion = f32[32] fusion(p0, p0_init), kind=kLoop, calls=f
    }
  )"));
  EXPECT_THAT(
      input_indexing.indexing_maps,
      UnorderedElementsAre(
          Pair(0, ElementsAre(MatchIndexingMap(
                      "(d0)[s0, s1] -> (s0 + 5, d0 * 2, s1 * 3 + 50)",
                      ElementsAre(MatchRange(0, 32)),
                      ElementsAre(MatchRange(0, 16), MatchRange(0, 128))))),
          Pair(1,
               ElementsAre(MatchIndexingMap(
                   "(d0) -> ()", ElementsAre(MatchRange(0, 32)), IsEmpty())))));
}

TEST_F(TileAnalysisTest, FusionOpWithReshape_CollapseOfExpand) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    f {
      p0 = f32[128] parameter(0)
      expand = f32[8, 16] reshape(p0)
      ROOT collapse = f32[128] reshape(expand)
    }
    ENTRY e {
      p0 = f32[128] parameter(0)
      ROOT fusion = f32[128] fusion(p0), kind=kLoop, calls=f
    }
  )"));
  EXPECT_THAT(
      input_indexing.indexing_maps,
      ElementsAre(Pair(0, ElementsAre(MatchIndexingMap(
                              "(d0) -> (d0)", ElementsAre(MatchRange(0, 128)),
                              IsEmpty())))));
}

TEST_F(TileAnalysisTest, FusionOpWithReshape_ExpandOfCollapse) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    f {
      p0 = f32[8, 16] parameter(0)
      collapse = f32[128] reshape(p0)
      ROOT expand = f32[8, 16] reshape(collapse)
    }
    ENTRY e {
      p0 = f32[8, 16] parameter(0)
      ROOT fusion = f32[8, 16] fusion(p0), kind=kLoop, calls=f
    }
  )"));
  EXPECT_THAT(
      input_indexing.indexing_maps,
      ElementsAre(Pair(0, ElementsAre(MatchIndexingMap(
                              "(d0, d1) -> (d0, d1)",
                              ElementsAre(MatchRange(0, 8), MatchRange(0, 16)),
                              IsEmpty())))));
}

TEST_F(TileAnalysisTest, FusionOpWithReshape_ChainedGenericReshapes) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    f {
      p0 = f32[10, 10, 10] parameter(0)
      reshape1 = f32[50, 20] reshape(p0)
      ROOT reshape2 = f32[10, 10, 10] reshape(reshape1)
    }
    ENTRY e {
      p0 = f32[10, 10, 10] parameter(0)
      ROOT fusion = f32[10, 10, 10] fusion(p0), kind=kLoop, calls=f
    }
  )"));
  EXPECT_THAT(
      input_indexing.indexing_maps,
      ElementsAre(Pair(0, ElementsAre(MatchIndexingMap(
                              "(d0, d1, d2) -> (d0, d1, d2)",
                              ElementsAre(MatchRange(0, 10), MatchRange(0, 10),
                                          MatchRange(0, 10)),
                              IsEmpty())))));
}

TEST_F(TileAnalysisTest, FusionOpWithSliceOfSlice) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    f {
      p0 = f32[150, 64, 1024] parameter(0)
      p0_slice_1 = f32[16, 32, 128] slice(f32[150, 64, 1024] p0),
                slice={[5:21:1], [0:64:2], [50:434:3]}
      ROOT p0_slice_2 = f32[7, 9, 24] slice(f32[16, 32, 128] p0_slice_1),
                slice={[3:16:2], [4:30:3], [5:100:4]}
    }
    ENTRY e {
      p0 = f32[150, 64, 1024] parameter(0)
      ROOT fusion = f32[7, 9, 24] fusion(p0), kind=kLoop, calls=f
    }
  )"));
  EXPECT_THAT(
      input_indexing.indexing_maps,
      ElementsAre(
          Pair(0, ElementsAre(MatchIndexingMap(
                      "(d0, d1, d2) -> (d0 * 2 + 8, d1 * 6 + 8, d2 * 12 + 65)",
                      ElementsAre(MatchRange(0, 7), MatchRange(0, 9),
                                  MatchRange(0, 24)),
                      IsEmpty())))));
}

TEST_F(TileAnalysisTest, IotaOp) {
  auto ir = R"(
    HloModule m
    ENTRY e {
      ROOT iota = s32[5,5,111,42] iota(), iota_dimension=0
    }
  )";
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(ir));
  EXPECT_THAT(input_indexing.indexing_maps, IsEmpty());
}

TEST_F(TileAnalysisTest, ReshapeOpCollapseShape) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      p0 = f32[4,8] parameter(0)
      ROOT reshape = f32[32] reshape(p0)
    }
  )"));
  EXPECT_THAT(
      input_indexing.indexing_maps,
      ElementsAre(Pair(0, ElementsAre(MatchIndexingMap(
                              "(d0) -> (d0 floordiv 8, d0 mod 8)",
                              ElementsAre(MatchRange(0, 32)), IsEmpty())))));
}

TEST_F(TileAnalysisTest, ReshapeOpExpandShape) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      p0 = f32[32] parameter(0)
      ROOT reshape = f32[4, 8] reshape(p0)
    }
  )"));
  EXPECT_THAT(
      input_indexing.indexing_maps,
      ElementsAre(Pair(0, ElementsAre(MatchIndexingMap(
                              "(d0, d1) -> (d0 * 8 + d1)",
                              ElementsAre(MatchRange(0, 4), MatchRange(0, 8)),
                              IsEmpty())))));
}

TEST_F(TileAnalysisTest, ReshapeOpExpandAndCollapseShape) {
  auto ir = R"(
    HloModule m
    ENTRY e {
      p0 = f32[4, 8, 12] parameter(0)
      ROOT reshape = f32[32, 3, 4] reshape(p0)
    }
  )";
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(ir));
  EXPECT_THAT(
      input_indexing.indexing_maps,
      ElementsAre(
          Pair(0, ElementsAre(MatchIndexingMap(
                      "(d0, d1, d2) -> (d0 floordiv 8, d0 mod 8, d1 * 4 + d2)",
                      ElementsAre(MatchRange(0, 32), MatchRange(0, 3),
                                  MatchRange(0, 4)),
                      IsEmpty())))));

  TF_ASSERT_OK_AND_ASSIGN(auto output_indexing,
                          GetInputToOutputIndexingForEntryComputation(ir));
  EXPECT_THAT(
      output_indexing.indexing_maps,
      ElementsAre(
          Pair(0, ElementsAre(MatchIndexingMap(
                      "(d0, d1, d2) -> (d0 * 8 + d1, d2 floordiv 4, d2 mod 4)",
                      ElementsAre(MatchRange(0, 4), MatchRange(0, 8),
                                  MatchRange(0, 12)),
                      IsEmpty())))));
}

TEST_F(TileAnalysisTest, ReshapeOpExpandSubshapeOnly) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      p0 = f32[16, 8] parameter(0)
      ROOT reshape = f32[4, 4, 8] reshape(p0)
    }
  )"));
  EXPECT_THAT(
      input_indexing.indexing_maps,
      ElementsAre(Pair(0, ElementsAre(MatchIndexingMap(
                              "(d0, d1, d2) -> (d0 * 4 + d1, d2)",
                              ElementsAre(MatchRange(0, 4), MatchRange(0, 4),
                                          MatchRange(0, 8)),
                              IsEmpty())))));
}

TEST_F(TileAnalysisTest, ReshapeOpGenericReshape2DTO3D) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      p0 = f32[4,8] parameter(0)
      ROOT reshape = f32[2, 4, 4] reshape(p0)
    }
  )"));
  EXPECT_THAT(
      input_indexing.indexing_maps,
      ElementsAre(Pair(
          0,
          ElementsAre(MatchIndexingMap(
              "(d0, d1, d2) -> (d0 * 2 + (d1 * 4 + d2) floordiv 8, "
              "(d1 * 4 + d2) mod 8)",
              ElementsAre(MatchRange(0, 2), MatchRange(0, 4), MatchRange(0, 4)),
              IsEmpty())))));
}

TEST_F(TileAnalysisTest, ReshapeOpGenericReshape3DTO2D) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      p0 = f32[2, 4, 4] parameter(0)
      ROOT reshape = f32[4, 8] reshape(p0)
    }
  )"));
  EXPECT_THAT(
      input_indexing.indexing_maps,
      ElementsAre(Pair(0, ElementsAre(MatchIndexingMap(
                              "(d0, d1) -> ((d0 * 8 + d1) floordiv 16, "
                              "((d0 * 8 + d1) mod 16) floordiv 4, d1 mod 4)",
                              ElementsAre(MatchRange(0, 4), MatchRange(0, 8)),
                              IsEmpty())))));
}

TEST_F(TileAnalysisTest, ReduceOp) {
  auto ir = R"(
    HloModule m
    max {
      p0 = f32[] parameter(0)
      p1 = f32[] parameter(1)
      ROOT max = f32[] maximum(p0, p1)
    }
    ENTRY e {
      p0 = f32[150, 20, 10, 50] parameter(0)
      p0_init = f32[] constant(-inf)
      ROOT reduce = f32[150, 10] reduce(p0, p0_init),
        dimensions={3, 1}, to_apply=max
    }
  )";
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(ir));
  EXPECT_THAT(
      input_indexing.indexing_maps,
      UnorderedElementsAre(
          Pair(0, ElementsAre(MatchIndexingMap(
                      "(d0, d1)[s0, s1] -> (d0, s0, d1, s1)",
                      ElementsAre(MatchRange(0, 150), MatchRange(0, 10)),
                      ElementsAre(MatchRange(0, 20), MatchRange(0, 50))))),
          Pair(1, ElementsAre(MatchIndexingMap(
                      "(d0, d1) -> ()",
                      ElementsAre(MatchRange(0, 150), MatchRange(0, 10)),
                      IsEmpty())))));

  TF_ASSERT_OK_AND_ASSIGN(auto output_indexing,
                          GetInputToOutputIndexingForEntryComputation(ir));
  EXPECT_THAT(
      output_indexing.indexing_maps,
      UnorderedElementsAre(
          Pair(0, ElementsAre(MatchIndexingMap(
                      "(d0, d1, d2, d3) -> (d0, d2)",
                      ElementsAre(MatchRange(0, 150), MatchRange(0, 20),
                                  MatchRange(0, 10), MatchRange(0, 50)),
                      IsEmpty()))),
          Pair(1, ElementsAre(MatchIndexingMap(
                      "()[s0, s1] -> (s0, s1)", IsEmpty(),
                      ElementsAre(MatchRange(0, 150), MatchRange(0, 10)))))));
}

TEST_F(TileAnalysisTest, VariadicReduceOp) {
  absl::string_view ir = R"(
    HloModule m
    min {
      tmp_0 = f32[] parameter(0)
      tmp_1 = f32[] parameter(2)
      tmp_2 = s32[] parameter(1)
      tmp_3 = s32[] parameter(3)
      cmp = pred[] compare(tmp_0, tmp_1), direction=GE
      select1 = f32[] select(cmp, tmp_0, tmp_1)
      select2 = s32[] select(cmp, tmp_2, tmp_3)
      ROOT tmp_4 = (f32[], s32[]) tuple(select1, select2)
    }
    ENTRY e {
      p0 = f32[256,10] parameter(0)
      p0_init = f32[] constant(-inf)
      p1 = s32[256,10] parameter(1)
      p1_init = s32[] constant(0)
      ROOT reduce = (f32[10], s32[10]) reduce(p0, p1, p0_init, p1_init),
        dimensions={0}, to_apply=min
    }
  )";
  TF_ASSERT_OK_AND_ASSIGN(
      auto output_indexing_0,
      GetOutputToInputIndexingForEntryComputation(ir, /*output_id=*/0));
  EXPECT_THAT(
      output_indexing_0.indexing_maps,
      UnorderedElementsAre(
          Pair(0, ElementsAre(MatchIndexingMap(
                      "(d0)[s0] -> (s0, d0)", ElementsAre(MatchRange(0, 10)),
                      ElementsAre(MatchRange(0, 256))))),
          Pair(1, ElementsAre(MatchIndexingMap(
                      "(d0)[s0] -> (s0, d0)", ElementsAre(MatchRange(0, 10)),

                      ElementsAre(MatchRange(0, 256))))),
          Pair(2,
               ElementsAre(MatchIndexingMap(
                   "(d0) -> ()", ElementsAre(MatchRange(0, 10)), IsEmpty()))),
          Pair(3,
               ElementsAre(MatchIndexingMap(
                   "(d0) -> ()", ElementsAre(MatchRange(0, 10)), IsEmpty())))));
  TF_ASSERT_OK_AND_ASSIGN(
      auto output_indexing_1,
      GetOutputToInputIndexingForEntryComputation(ir, /*output_id=*/1));
  EXPECT_THAT(
      output_indexing_1.indexing_maps,
      UnorderedElementsAre(
          Pair(0, ElementsAre(MatchIndexingMap(
                      "(d0)[s0] -> (s0, d0)", ElementsAre(MatchRange(0, 10)),
                      ElementsAre(MatchRange(0, 256))))),
          Pair(1, ElementsAre(MatchIndexingMap(
                      "(d0)[s0] -> (s0, d0)", ElementsAre(MatchRange(0, 10)),

                      ElementsAre(MatchRange(0, 256))))),
          Pair(2,
               ElementsAre(MatchIndexingMap(
                   "(d0) -> ()", ElementsAre(MatchRange(0, 10)), IsEmpty()))),
          Pair(3,
               ElementsAre(MatchIndexingMap(
                   "(d0) -> ()", ElementsAre(MatchRange(0, 10)), IsEmpty())))));

  TF_ASSERT_OK_AND_ASSIGN(
      auto input_indexing_0,
      GetInputToOutputIndexingForEntryComputation(ir, /*input_id=*/0));
  EXPECT_THAT(
      input_indexing_0.indexing_maps,
      UnorderedElementsAre(
          Pair(0, ElementsAre(MatchIndexingMap(
                      "(d0, d1) -> (d1)",
                      ElementsAre(MatchRange(0, 256), MatchRange(0, 10)),
                      IsEmpty()))),
          Pair(1, ElementsAre(MatchIndexingMap(
                      "(d0, d1) -> (d1)",
                      ElementsAre(MatchRange(0, 256), MatchRange(0, 10)),
                      IsEmpty()))),
          Pair(2,
               ElementsAre(MatchIndexingMap("()[s0] -> (s0)", IsEmpty(),
                                            ElementsAre(MatchRange(0, 10))))),
          Pair(3,
               ElementsAre(MatchIndexingMap("()[s0] -> (s0)", IsEmpty(),
                                            ElementsAre(MatchRange(0, 10)))))));

  TF_ASSERT_OK_AND_ASSIGN(
      auto input_indexing_1,
      GetInputToOutputIndexingForEntryComputation(ir, /*input_id=*/1));
  EXPECT_THAT(
      input_indexing_1.indexing_maps,
      UnorderedElementsAre(
          Pair(0, ElementsAre(MatchIndexingMap(
                      "(d0, d1) -> (d1)",
                      ElementsAre(MatchRange(0, 256), MatchRange(0, 10)),
                      IsEmpty()))),
          Pair(1, ElementsAre(MatchIndexingMap(
                      "(d0, d1) -> (d1)",
                      ElementsAre(MatchRange(0, 256), MatchRange(0, 10)),
                      IsEmpty()))),
          Pair(2,
               ElementsAre(MatchIndexingMap("()[s0] -> (s0)", IsEmpty(),
                                            ElementsAre(MatchRange(0, 10))))),
          Pair(3,
               ElementsAre(MatchIndexingMap("()[s0] -> (s0)", IsEmpty(),
                                            ElementsAre(MatchRange(0, 10)))))));
}

TEST_F(TileAnalysisTest, ReverseOp) {
  auto ir = R"(
    HloModule m
    ENTRY e {
      p0 = f32[1, 17, 9, 9] parameter(0)
      ROOT reverse = f32[1, 17, 9, 9] reverse(p0), dimensions={1, 2}
    }
  )";
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(ir));
  EXPECT_THAT(
      input_indexing.indexing_maps,
      ElementsAre(Pair(0, ElementsAre(MatchIndexingMap(
                              "(d0, d1, d2, d3) -> (d0, -d1 + 16, -d2 + 8, d3)",
                              ElementsAre(MatchRange(0, 1), MatchRange(0, 17),
                                          MatchRange(0, 9), MatchRange(0, 9)),
                              IsEmpty())))));

  TF_ASSERT_OK_AND_ASSIGN(auto output_indexing,
                          GetInputToOutputIndexingForEntryComputation(ir));
  EXPECT_THAT(
      output_indexing.indexing_maps,
      ElementsAre(Pair(0, ElementsAre(MatchIndexingMap(
                              "(d0, d1, d2, d3) -> (d0, -d1 + 16, -d2 + 8, d3)",
                              ElementsAre(MatchRange(0, 1), MatchRange(0, 17),
                                          MatchRange(0, 9), MatchRange(0, 9)),
                              IsEmpty())))));
}

TEST_F(TileAnalysisTest, ReverseReshape) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    fused_computation {
      p0 = f32[10, 11] parameter(0)
      reverse.0 = f32[10, 11] reverse(p0), dimensions={0, 1}
      reshape.0 = f32[110] reshape(reverse.0)
      reverse.1 = f32[110] reverse(reshape.0), dimensions={0}
      ROOT reshape.1 = f32[10, 11] reshape(reverse.1)
    }
    ENTRY e {
      p0 = f32[10, 11] parameter(0)
      ROOT fusion = f32[10, 11] fusion(p0), kind=kLoop,
      calls=fused_computation
    }
  )"));
  EXPECT_THAT(
      input_indexing.indexing_maps,
      ElementsAre(Pair(0, ElementsAre(MatchIndexingMap(
                              "(d0, d1) -> (d0, d1)",
                              ElementsAre(MatchRange(0, 10), MatchRange(0, 11)),
                              IsEmpty())))));
}

TEST_F(TileAnalysisTest, SliceOp) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      p0 = f32[10, 20, 50] parameter(0)
      ROOT slice = f32[5, 3, 25] slice(f32[10, 20, 50] p0),
          slice={[5:10:1], [3:20:7], [0:50:2]}
    }
  )"));
  EXPECT_THAT(
      input_indexing.indexing_maps,
      ElementsAre(Pair(0, ElementsAre(MatchIndexingMap(
                              "(d0, d1, d2) -> (d0 + 5, d1 * 7 + 3, d2 * 2)",
                              ElementsAre(MatchRange(0, 5), MatchRange(0, 3),
                                          MatchRange(0, 25)),
                              IsEmpty())))));
}

TEST_F(TileAnalysisTest, TransposeOp) {
  auto ir = R"(
    HloModule m
    ENTRY e {
      p0 = f32[3, 12288, 6, 128] parameter(0)
      ROOT transpose = f32[3, 6, 128, 12288]
        transpose(p0), dimensions={0, 2, 3, 1}
    }
  )";
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(ir));
  EXPECT_THAT(input_indexing.indexing_maps,
              ElementsAre(Pair(
                  0, ElementsAre(MatchIndexingMap(
                         "(d0, d1, d2, d3) -> (d0, d3, d1, d2)",
                         ElementsAre(MatchRange(0, 3), MatchRange(0, 6),
                                     MatchRange(0, 128), MatchRange(0, 12288)),
                         IsEmpty())))));

  TF_ASSERT_OK_AND_ASSIGN(auto output_indexing,
                          GetInputToOutputIndexingForEntryComputation(ir));
  EXPECT_THAT(output_indexing.indexing_maps,
              ElementsAre(Pair(
                  0, ElementsAre(MatchIndexingMap(
                         "(d0, d1, d2, d3) -> (d0, d2, d3, d1)",
                         ElementsAre(MatchRange(0, 3), MatchRange(0, 12288),
                                     MatchRange(0, 6), MatchRange(0, 128)),
                         IsEmpty())))));
}

TEST_F(TileAnalysisTest, TransposeOp4D) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      p0 = f32[3, 12288, 6, 128] parameter(0)
      ROOT bitcast = f32[3, 6, 128, 12288] {2, 1, 3, 0} bitcast(p0)
    }
  )"));
  EXPECT_THAT(input_indexing.indexing_maps,
              ElementsAre(Pair(
                  0, ElementsAre(MatchIndexingMap(
                         "(d0, d1, d2, d3) -> (d0, d3, d1, d2)",
                         ElementsAre(MatchRange(0, 3), MatchRange(0, 6),
                                     MatchRange(0, 128), MatchRange(0, 12288)),
                         IsEmpty())))));
}

TEST_F(TileAnalysisTest, DotOp) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      p0 = f32[4, 38, 17, 11, 18, 10] parameter(0)
      p1 = f32[17, 10, 16, 18, 22, 38] parameter(1)
      ROOT dot = f32[10, 38, 4, 11, 16, 22] dot(p0, p1),
        lhs_batch_dims={5,1}, rhs_batch_dims={1,5},
        lhs_contracting_dims={4,2}, rhs_contracting_dims={3,0}
    }
  )"));
  EXPECT_THAT(
      input_indexing.indexing_maps,
      UnorderedElementsAre(
          Pair(0, ElementsAre(MatchIndexingMap(
                      "(d0, d1, d2, d3, d4, d5)[s0, s1] -> "
                      "(d2, d1, s1, d3, s0, d0)",
                      ElementsAre(MatchRange(0, 10), MatchRange(0, 38),
                                  MatchRange(0, 4), MatchRange(0, 11),
                                  MatchRange(0, 16), MatchRange(0, 22)),
                      ElementsAre(MatchRange(0, 18), MatchRange(0, 17))))),
          Pair(1, ElementsAre(MatchIndexingMap(
                      "(d0, d1, d2, d3, d4, d5)[s0, s1] -> "
                      "(s1, d0, d4, s0, d5, d1)",
                      ElementsAre(MatchRange(0, 10), MatchRange(0, 38),
                                  MatchRange(0, 4), MatchRange(0, 11),
                                  MatchRange(0, 16), MatchRange(0, 22)),
                      ElementsAre(MatchRange(0, 18), MatchRange(0, 17)))))));
}

TEST_F(TileAnalysisTest, UnsupportedOps) {
  ASSERT_IS_NOT_OK(GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      p0 = f32[1, 17, 9, 9] parameter(0)
      p1 = f32[5, 17, 9, 9] parameter(1)
      ROOT concat = f32[6, 17, 9, 9] concatenate(p0, p1)
    }
  )"));
  ASSERT_IS_NOT_OK(GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      input = s32[1,1,25,1] parameter(0)
      update = s32[1,1,2,1] parameter(1)
      start_indices = s32[4] parameter(2)
      ROOT dyn-update = s32[1,1,25,1] dynamic-update-slice(
        s32[1,1,25,1] input, s32[1,1,2,1] update, s32[4] start_indices)
    }
  )"));
}

using SymbolicTileTest = TileAnalysisTest;

TEST_F(SymbolicTileTest, SymbolicTileConstructionIsCorrect) {
  std::vector<int64_t> shape = {182, 17, 2};
  SymbolicTile tile(shape, &mlir_context_);

  EXPECT_THAT(ToString(tile.affine_map()),
              StrEq("(d0, d1, d2, d3, d4, d5)[s0, s1, s2] -> "
                    "(d0 * s0 + d1, d2 * s1 + d3, d4 * s2 + d5)"));
  EXPECT_THAT(tile.sizes(), AllOf(Each(std::nullopt), SizeIs(shape.size())));
  EXPECT_THAT(tile.max_sizes(), ElementsAreArray(shape));
}

TEST_F(SymbolicTileTest,
       CanPropagateTileFromDotOutputToInputsWithoutSpecializedTileSizes) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      p0 = f32[11, 17, 19] parameter(0)
      p1 = f32[11, 19, 23] parameter(1)
      ROOT dot = f32[11, 17, 23] dot(p0, p1),
        lhs_batch_dims={0}, rhs_batch_dims={0},
        lhs_contracting_dims={2}, rhs_contracting_dims={1}
    }
  )"));

  SymbolicTile output_tile(/*target_shape=*/{11, 17, 23}, &mlir_context_);

  EXPECT_THAT(
      output_tile.TryPropagateTileThroughIndexingMap(
          *input_indexing.indexing_maps[0].begin()),
      Optional(MatchSymbolicTile(
          "(d0, d1, d2, d3, d4, d5)[s0, s1, s2, s3] -> "
          "(d0 * s1 + d1, d2 * s2 + d3, s0)",
          ElementsAre(19, std::nullopt, std::nullopt, std::nullopt),
          ElementsAre(19, 11, 17, 23), ElementsAre(11, 11, 17, 17, 23, 23))));

  EXPECT_THAT(
      output_tile.TryPropagateTileThroughIndexingMap(
          *input_indexing.indexing_maps[1].begin()),
      Optional(MatchSymbolicTile(
          "(d0, d1, d2, d3, d4, d5)[s0, s1, s2, s3] -> "
          "(d0 * s1 + d1, s0, d4 * s3 + d5)",
          ElementsAre(19, std::nullopt, std::nullopt, std::nullopt),
          ElementsAre(19, 11, 17, 23), ElementsAre(11, 11, 17, 17, 23, 23))));
}

TEST_F(SymbolicTileTest, CanPropagateTileThroughTrivialReshape) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      p0 = f32[11, 17, 19] parameter(0)
      ROOT reshape = f32[1, 11, 17, 19] reshape(p0)
    }
  )"));

  std::vector<int64_t> target_shape({1, 11, 17, 19});
  SymbolicTile output_tile(target_shape, &mlir_context_);

  std::optional<SymbolicTile> operand_tile =
      output_tile.TryPropagateTileThroughIndexingMap(
          *input_indexing.indexing_maps[0].begin());

  std::optional<int64_t> undef = std::nullopt;

  // Note: the affine map here could be simplified further since s0 can take on
  // a single value (0). The fact that it is not is a current limitation of
  // 'IndexingMapSimplifier`. When that simplification logic becomes more
  // advanced, this test may thus require editing.
  EXPECT_THAT(operand_tile,
              Optional(MatchSymbolicTile(
                  "(d0, d1, d2, d3, d4, d5, d6, d7)[s0, s1, s2, s3] -> "
                  "((d0 * s0 + d1) * 11 + d2 * s1 + d3, d4 * s2 + d5, d6 * s3 "
                  "+ d7)",  // NOLINT
                  AllOf(Each(undef), SizeIs(target_shape.size())),
                  ElementsAreArray(target_shape),
                  ElementsAre(1, 1, 11, 11, 17, 17, 19, 19))));
}

TEST_F(SymbolicTileTest,
       FailsToPropagateTileThroughReshapeWithoutSpecializedTileSizes) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      p0 = f32[12, 4, 19] parameter(0)
      ROOT reshape = f32[4, 12, 19] reshape(p0)
    }
  )"));

  std::vector<int64_t> target_shape({4, 12, 19});
  SymbolicTile output_tile(target_shape, &mlir_context_);

  EXPECT_EQ(output_tile.TryPropagateTileThroughIndexingMap(
                *input_indexing.indexing_maps[0].begin()),
            std::nullopt);
}

TEST_F(SymbolicTileTest,
       CanPropagateTileThroughElementwiseOpWithoutSpecializedTileSizes) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      p0 = f32[150] parameter(0)
      p1 = f32[150] parameter(1)
      ROOT add = f32[150] add(p0, p1)
    }
  )"));

  SymbolicTile output_tile(/*target_shape=*/{150}, &mlir_context_);

  EXPECT_THAT(output_tile.TryPropagateTileThroughIndexingMap(
                  *input_indexing.indexing_maps[0].begin()),
              Optional(MatchSymbolicTile(
                  "(d0, d1)[s0] -> (d0 * s0 + d1)", ElementsAre(std::nullopt),
                  ElementsAre(150), ElementsAre(150, 150))));
}

TEST_F(SymbolicTileTest,
       CanPropagateTileFromBroadcastOutputToInputWithoutSpecializedTileSizes) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      p0 = f32[150] parameter(0)
      ROOT broadcast = f32[157,150] broadcast(p0), dimensions={1}
    }
  )"));

  SymbolicTile output_tile(/*target_shape=*/{157, 150}, &mlir_context_);

  EXPECT_THAT(output_tile.TryPropagateTileThroughIndexingMap(
                  *input_indexing.indexing_maps[0].begin()),
              Optional(MatchSymbolicTile(
                  "(d0, d1, d2, d3)[s0, s1] -> (d2 * s1 + d3)",
                  ElementsAre(std::nullopt, std::nullopt),
                  ElementsAre(157, 150), ElementsAre(157, 157, 150, 150))));
}

TEST_F(SymbolicTileTest,
       CanPropagateTileFromReduceOutputToInputWithoutSpecializedTileSizes) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    max {
      p0 = f32[] parameter(0)
      p1 = f32[] parameter(1)
      ROOT max = f32[] maximum(p0, p1)
    }

    ENTRY e {
      p0 = f32[125,150] parameter(0)
      c0 = f32[] constant(-inf)
      ROOT reduce = f32[150] reduce(p0, c0), dimensions={0}, to_apply=max
    }
  )"));

  SymbolicTile output_tile(/*target_shape=*/{150}, &mlir_context_);

  EXPECT_THAT(output_tile.TryPropagateTileThroughIndexingMap(
                  *input_indexing.indexing_maps[0].begin()),
              Optional(MatchSymbolicTile(
                  "(d0, d1)[s0, s1] -> (s0, d0 * s1 + d1)",
                  ElementsAre(125, std::nullopt), ElementsAre(125, 150),
                  ElementsAre(150, 150))));
}

TEST_F(SymbolicTileTest,
       CanPropagateTileThroughReverseWithoutSpecializedTileSizes) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      p0 = f32[179] parameter(0)
      ROOT reverse = f32[179] reverse(p0), dimensions={0}
    }
  )"));

  SymbolicTile output_tile(/*target_shape=*/{179}, &mlir_context_);

  EXPECT_THAT(
      output_tile.TryPropagateTileThroughIndexingMap(
          *input_indexing.indexing_maps[0].begin()),
      Optional(MatchSymbolicTile("(d0, d1)[s0] -> (-(d0 * s0 + d1) + 178)",
                                 ElementsAre(std::nullopt), ElementsAre(179),
                                 ElementsAre(179, 179))));
}

TEST_F(SymbolicTileTest,
       CanPropagateTileFromSliceOutputToInputWithoutSpecializedTileSizes) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      p0 = f32[120,142] parameter(0)
      ROOT slice = f32[10,21] slice(p0), slice={[40:60:2], [20:104:4]}
    }
  )"));

  SymbolicTile output_tile(/*target_shape=*/{10, 21}, &mlir_context_);

  EXPECT_THAT(output_tile.TryPropagateTileThroughIndexingMap(
                  *input_indexing.indexing_maps[0].begin()),
              Optional(MatchSymbolicTile(
                  "(d0, d1, d2, d3)[s0, s1] -> "
                  "((d0 * s0 + d1) * 2 + 40, (d2 * s1 + d3) * 4 + 20)",
                  ElementsAre(std::nullopt, std::nullopt), ElementsAre(10, 21),
                  ElementsAre(10, 10, 21, 21))));
}

TEST_F(SymbolicTileTest,
       CanPropagateTileThroughTransposeWithoutSpecializedTileSizes) {
  TF_ASSERT_OK_AND_ASSIGN(auto input_indexing,
                          GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      p0 = f32[21,10] parameter(0)
      ROOT transpose = f32[10,21] transpose(p0), dimensions={1,0}
    }
  )"));

  SymbolicTile output_tile(/*target_shape=*/{10, 21}, &mlir_context_);

  EXPECT_THAT(output_tile.TryPropagateTileThroughIndexingMap(
                  *input_indexing.indexing_maps[0].begin()),
              Optional(MatchSymbolicTile(
                  "(d0, d1, d2, d3)[s0, s1] -> (d2 * s1 + d3, d0 * s0 + d1)",
                  ElementsAre(std::nullopt, std::nullopt), ElementsAre(10, 21),
                  ElementsAre(10, 10, 21, 21))));
}

}  // namespace
}  // namespace gpu
}  // namespace xla
