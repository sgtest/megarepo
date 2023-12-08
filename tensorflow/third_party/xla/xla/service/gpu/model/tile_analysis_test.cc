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

#include <vector>

#include <gmock/gmock.h>
#include "absl/strings/str_cat.h"
#include "absl/strings/string_view.h"
#include "mlir/IR/MLIRContext.h"  // from @llvm-project
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/hlo/ir/hlo_opcode.h"
#include "xla/status_macros.h"
#include "xla/statusor.h"
#include "xla/test_helpers.h"
#include "xla/tests/hlo_test_base.h"
#include "tsl/platform/statusor.h"
#include "tsl/platform/test.h"

namespace xla {
namespace gpu {
namespace {

using ::testing::ElementsAre;
using ::testing::ElementsAreArray;
using ::testing::Eq;
using ::testing::ExplainMatchResult;
using ::testing::HasSubstr;
using ::testing::IsEmpty;
using ::testing::Pair;
using ::testing::PrintToString;
using ::testing::UnorderedElementsAre;

MATCHER_P2(MatchIndexingMap, affine_map_string, input_dims_sizes,
           absl::StrCat(negation ? "equals " : "doesn't equal ", "affine map ",
                        affine_map_string, " with input dim sizes ",
                        PrintToString(input_dims_sizes))) {
  return ExplainMatchResult(HasSubstr(affine_map_string),
                            ToString(arg.affine_map), result_listener) &&
         ExplainMatchResult(ElementsAreArray(input_dims_sizes),
                            arg.input_dims_sizes, result_listener);
}

MATCHER_P2(MatchInstrIndexing, operand_id, indexing_map_matchers, "") {
  return ExplainMatchResult(Eq(operand_id), arg.operand_id, result_listener) &&
         ExplainMatchResult(indexing_map_matchers, arg.indexing_maps,
                            result_listener);
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
          Pair(parameter, ElementsAre(MatchIndexingMap("(d0, d1) -> (d0, d1)",
                                                       std::vector<int>{}))),
          Pair(transpose, ElementsAre(MatchIndexingMap("(d0, d1) -> (d0, d1)",
                                                       std::vector<int>{})))));

  TF_CHECK_OK(FuseProducerConsumerOutputToInputIndexing(
      transpose, &grouped_by_key, &mlir_context_));
  EXPECT_THAT(
      grouped_by_key,
      UnorderedElementsAre(Pair(
          parameter,
          UnorderedElementsAre(
              MatchIndexingMap("(d0, d1) -> (d0, d1)", std::vector<int>{}),
              MatchIndexingMap("(d0, d1) -> (d1, d0)", std::vector<int>{})))));
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
                  Pair(0, ElementsAre(MatchIndexingMap("(d0, d1) -> (d0, d1)",
                                                       std::vector<int>{}))),
                  Pair(1, ElementsAre(MatchIndexingMap("(d0, d1) -> (d0, d1)",
                                                       std::vector<int>{})))));
  TF_ASSERT_OK_AND_ASSIGN(
      auto output_indexing,
      GetInputToOutputIndexingForEntryComputation(ir, /*input_id=*/0));
  EXPECT_THAT(output_indexing.indexing_maps,
              UnorderedElementsAre(
                  Pair(0, ElementsAre(MatchIndexingMap("(d0, d1) -> (d0, d1)",
                                                       std::vector<int>{})))));
  TF_ASSERT_OK_AND_ASSIGN(
      auto output_indexing1,
      GetInputToOutputIndexingForEntryComputation(ir, /*input_id=*/1));
  EXPECT_THAT(output_indexing1.indexing_maps,
              UnorderedElementsAre(
                  Pair(0, ElementsAre(MatchIndexingMap("(d0, d1) -> (d0, d1)",
                                                       std::vector<int>{})))));
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
          0, ElementsAre(MatchIndexingMap("(d0, d1, d2) -> (d0, d1 * 4 + d2)",
                                          std::vector<int>{})))));
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
  EXPECT_THAT(
      input_indexing.indexing_maps,
      UnorderedElementsAre(Pair(0, ElementsAre(MatchIndexingMap(
                                       "(d0, d1, d2, d3) -> (d0, d3, d1, d2)",
                                       std::vector<int>{})))));
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
                              std::vector<int>{})))));
  TF_ASSERT_OK_AND_ASSIGN(auto output_indexing,
                          GetInputToOutputIndexingForEntryComputation(ir));
  EXPECT_THAT(
      output_indexing.indexing_maps,
      UnorderedElementsAre(Pair(
          0, ElementsAre(MatchIndexingMap("(d0, d1, d2) -> (d1 * 3 + d2, d0)",
                                          std::vector<int>{})))));
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
                  Pair(0, ElementsAre(MatchIndexingMap("(d0, d1, d2) -> (d1)",
                                                       std::vector<int>{})))));

  TF_ASSERT_OK_AND_ASSIGN(auto output_indexing,
                          GetInputToOutputIndexingForEntryComputation(ir));
  EXPECT_THAT(output_indexing.indexing_maps,
              UnorderedElementsAre(Pair(0, ElementsAre(MatchIndexingMap(
                                               "(d0)[s0, s1] -> (s0, d0, s1)",
                                               std::vector<int>{10, 30})))));
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
  EXPECT_THAT(
      input_indexing.indexing_maps,
      UnorderedElementsAre(Pair(0, ElementsAre(MatchIndexingMap(
                                       "(d0) -> (d0)", std::vector<int>{}))),
                           Pair(1, ElementsAre(MatchIndexingMap(
                                       "(d0) -> (d0)", std::vector<int>{})))));
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
  EXPECT_TRUE(input_indexing.Simplify({16, 16, 3, 1, 6, 128}));

  EXPECT_THAT(
      input_indexing.indexing_maps,
      UnorderedElementsAre(
          Pair(0,
               ElementsAre(MatchIndexingMap("(d0, d1, d2, d3, d4, d5)[s0] -> "
                                            "(d2 + d3, d0 * 768 + s0, d4, d5)",
                                            std::vector<int>{768}))),
          Pair(1, ElementsAre(MatchIndexingMap(
                      "(d0, d1, d2, d3, d4, d5)[s0] -> (d0 * 768 + s0)",
                      std::vector<int>{768}))),
          Pair(2, ElementsAre(MatchIndexingMap(
                      "(d0, d1, d2, d3, d4, d5) -> (d1)", std::vector<int>{}))),
          Pair(3, ElementsAre(MatchIndexingMap(
                      "(d0, d1, d2, d3, d4, d5)[s0] -> (d1, d0 * 768 + s0)",
                      std::vector<int>{768}))),
          Pair(4, ElementsAre(MatchIndexingMap(
                      "(d0, d1, d2, d3, d4, d5)[s0] -> (d1, d0 * 768 + s0)",
                      std::vector<int>{768}))),
          Pair(5, ElementsAre(MatchIndexingMap(
                      "(d0, d1, d2, d3, d4, d5) -> (d2 + d3, d4, d5)",
                      std::vector<int>{})))));
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
  EXPECT_THAT(
      input_indexing.indexing_maps,
      UnorderedElementsAre(Pair(
          0,
          UnorderedElementsAre(
              MatchIndexingMap("(d0, d1) -> (d1, d0)", std::vector<int>{}),
              MatchIndexingMap("(d0, d1) -> (d0, d1)", std::vector<int>{})))));
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
      ROOT fusion = f32[2] fusion(p0, p1), kind=kLoop, calls=fused_computation
    })"));
  EXPECT_THAT(
      input_indexing.indexing_maps,
      UnorderedElementsAre(
          Pair(0,
               UnorderedElementsAre(
                   MatchIndexingMap("(d0) -> (d0)", std::vector<int>{}),
                   MatchIndexingMap("(d0) -> (d0 + 1)", std::vector<int>{}),
                   MatchIndexingMap("(d0) -> (d0 + 2)", std::vector<int>{}))),
          Pair(1,
               UnorderedElementsAre(
                   MatchIndexingMap("(d0) -> (d0)", std::vector<int>{}),
                   MatchIndexingMap("(d0) -> (d0 + 1)", std::vector<int>{}),
                   MatchIndexingMap("(d0) -> (d0 + 2)", std::vector<int>{})))));
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
  EXPECT_THAT(
      input_indexing.indexing_maps,
      UnorderedElementsAre(Pair(0, ElementsAre(MatchIndexingMap(
                                       "(d0)[s0, s1, s2] -> (s0, s2, d0, s1)",
                                       std::vector<int>{150, 50, 20}))),
                           Pair(1, ElementsAre(MatchIndexingMap(
                                       "(d0) -> ()", std::vector<int>{})))));
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
  EXPECT_THAT(
      input_indexing.indexing_maps,
      UnorderedElementsAre(
          Pair(0, ElementsAre(MatchIndexingMap("(d0, d1)[s0] -> (d0, s0)",
                                               std::vector<int>{20}))),
          Pair(1, ElementsAre(MatchIndexingMap("(d0, d1) -> ()",
                                               std::vector<int>{})))));
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
              UnorderedElementsAre(Pair(0, ElementsAre(MatchIndexingMap(
                                               "(d0, d1, d2) -> (d2, d0, d1)",
                                               std::vector<int>{})))));
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
  EXPECT_THAT(input_indexing.indexing_maps,
              UnorderedElementsAre(
                  Pair(0, ElementsAre(MatchIndexingMap(
                              "(d0)[s0, s1] -> (s0 + 5, d0 * 2, s1 * 3 + 50)",
                              std::vector<int>{16, 128}))),
                  Pair(1, ElementsAre(MatchIndexingMap("(d0) -> ()",
                                                       std::vector<int>{})))));
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
  EXPECT_THAT(input_indexing.indexing_maps,
              ElementsAre(Pair(0, ElementsAre(MatchIndexingMap(
                                      "(d0) -> (d0)", std::vector<int>{})))));
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
  EXPECT_TRUE(input_indexing.Simplify({8, 16}));
  EXPECT_THAT(
      input_indexing.indexing_maps,
      ElementsAre(Pair(0, ElementsAre(MatchIndexingMap("(d0, d1) -> (d0, d1)",
                                                       std::vector<int>{})))));
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
  EXPECT_TRUE(input_indexing.Simplify({10, 10, 10}));
  EXPECT_THAT(input_indexing.indexing_maps,
              ElementsAre(Pair(0, ElementsAre(MatchIndexingMap(
                                      "(d0, d1, d2) -> (d0, d1, d2)",
                                      std::vector<int>{})))));
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
                      std::vector<int>{})))));
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
  EXPECT_FALSE(input_indexing.Simplify({32}));
  EXPECT_THAT(input_indexing.indexing_maps,
              ElementsAre(Pair(0, ElementsAre(MatchIndexingMap(
                                      "(d0) -> (d0 floordiv 8, d0 mod 8)",
                                      std::vector<int>{})))));
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
  EXPECT_FALSE(input_indexing.Simplify({4, 8}));
  EXPECT_THAT(input_indexing.indexing_maps,
              ElementsAre(Pair(
                  0, ElementsAre(MatchIndexingMap("(d0, d1) -> (d0 * 8 + d1)",
                                                  std::vector<int>{})))));
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
  EXPECT_FALSE(input_indexing.Simplify({32, 3, 4}));
  EXPECT_THAT(
      input_indexing.indexing_maps,
      ElementsAre(
          Pair(0, ElementsAre(MatchIndexingMap(
                      "(d0, d1, d2) -> (d0 floordiv 8, d0 mod 8, d1 * 4 + d2)",
                      std::vector<int>{})))));

  TF_ASSERT_OK_AND_ASSIGN(auto output_indexing,
                          GetInputToOutputIndexingForEntryComputation(ir));
  EXPECT_FALSE(output_indexing.Simplify({4, 8, 12}));
  EXPECT_THAT(
      output_indexing.indexing_maps,
      ElementsAre(
          Pair(0, ElementsAre(MatchIndexingMap(
                      "(d0, d1, d2) -> (d0 * 8 + d1, d2 floordiv 4, d2 mod 4)",
                      std::vector<int>{})))));
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
  EXPECT_FALSE(input_indexing.Simplify({4, 4, 8}));
  EXPECT_THAT(input_indexing.indexing_maps,
              ElementsAre(Pair(0, ElementsAre(MatchIndexingMap(
                                      "(d0, d1, d2) -> (d0 * 4 + d1, d2)",
                                      std::vector<int>{})))));
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
  EXPECT_TRUE(input_indexing.Simplify({2, 4, 4}));
  // TODO(b/313840171): Simplify `(d1 * 4 + d2) floordiv 8` to `d1 floordiv 2`.
  EXPECT_THAT(input_indexing.indexing_maps,
              ElementsAre(Pair(
                  0, ElementsAre(MatchIndexingMap(
                         "(d0, d1, d2) -> (d0 * 2 + (d1 * 4 + d2) floordiv 8, "
                         "(d1 * 4 + d2) mod 8)",
                         std::vector<int>{})))));
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
  EXPECT_FALSE(input_indexing.Simplify({4, 8}));
  // TODO(b/313840171): Simplify `(d0 * 8 + d1) floordiv 16` to `d0 floordiv 2`.
  // TODO(b/313840171): Simplify `((d0 * 8 + d1) mod 16) floordiv 4` to
  // `((d0 * 8 + d1) floordiv 4) mod 4` to `(d0 * 2 + d1 floordiv 4) mod 4`.
  EXPECT_THAT(
      input_indexing.indexing_maps,
      ElementsAre(Pair(0, ElementsAre(MatchIndexingMap(
                              "(d0, d1) -> ((d0 * 8 + d1) floordiv 16, "
                              "((d0 * 8 + d1) mod 16) floordiv 4, d1 mod 4)",
                              std::vector<int>{})))));
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
  EXPECT_THAT(input_indexing.indexing_maps,
              UnorderedElementsAre(
                  Pair(0, ElementsAre(MatchIndexingMap(
                              "(d0, d1)[s0, s1] -> (d0, s0, d1, s1)",
                              std::vector<int>{20, 50}))),
                  Pair(1, ElementsAre(MatchIndexingMap("(d0, d1) -> ()",
                                                       std::vector<int>{})))));

  TF_ASSERT_OK_AND_ASSIGN(auto output_indexing,
                          GetInputToOutputIndexingForEntryComputation(ir));
  EXPECT_THAT(
      output_indexing.indexing_maps,
      UnorderedElementsAre(
          Pair(0, ElementsAre(MatchIndexingMap("(d0, d1, d2, d3) -> (d0, d2)",
                                               std::vector<int>{}))),
          Pair(1, ElementsAre(MatchIndexingMap("()[s0, s1] -> (s0, s1)",
                                               std::vector<int>{150, 10})))));
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
  EXPECT_THAT(output_indexing_0.indexing_maps,
              UnorderedElementsAre(
                  Pair(0, ElementsAre(MatchIndexingMap("(d0)[s0] -> (s0, d0)",
                                                       std::vector<int>{256}))),
                  Pair(1, ElementsAre(MatchIndexingMap("(d0)[s0] -> (s0, d0)",
                                                       std::vector<int>{256}))),
                  Pair(2, ElementsAre(MatchIndexingMap("(d0) -> ()",
                                                       std::vector<int>{}))),
                  Pair(3, ElementsAre(MatchIndexingMap("(d0) -> ()",
                                                       std::vector<int>{})))));
  TF_ASSERT_OK_AND_ASSIGN(
      auto output_indexing_1,
      GetOutputToInputIndexingForEntryComputation(ir, /*output_id=*/1));
  EXPECT_THAT(output_indexing_1.indexing_maps,
              UnorderedElementsAre(
                  Pair(0, ElementsAre(MatchIndexingMap("(d0)[s0] -> (s0, d0)",
                                                       std::vector<int>{256}))),
                  Pair(1, ElementsAre(MatchIndexingMap("(d0)[s0] -> (s0, d0)",
                                                       std::vector<int>{256}))),
                  Pair(2, ElementsAre(MatchIndexingMap("(d0) -> ()",
                                                       std::vector<int>{}))),
                  Pair(3, ElementsAre(MatchIndexingMap("(d0) -> ()",
                                                       std::vector<int>{})))));

  TF_ASSERT_OK_AND_ASSIGN(
      auto input_indexing_0,
      GetInputToOutputIndexingForEntryComputation(ir, /*input_id=*/0));
  EXPECT_THAT(input_indexing_0.indexing_maps,
              UnorderedElementsAre(
                  Pair(0, ElementsAre(MatchIndexingMap("(d0, d1) -> (d1)",
                                                       std::vector<int>{}))),
                  Pair(1, ElementsAre(MatchIndexingMap("(d0, d1) -> (d1)",
                                                       std::vector<int>{}))),
                  Pair(2, ElementsAre(MatchIndexingMap("()[s0] -> (s0)",
                                                       std::vector<int>{10}))),
                  Pair(3, ElementsAre(MatchIndexingMap(
                              "()[s0] -> (s0)", std::vector<int>{10})))));

  TF_ASSERT_OK_AND_ASSIGN(
      auto input_indexing_1,
      GetInputToOutputIndexingForEntryComputation(ir, /*input_id=*/1));
  EXPECT_THAT(input_indexing_1.indexing_maps,
              UnorderedElementsAre(
                  Pair(0, ElementsAre(MatchIndexingMap("(d0, d1) -> (d1)",
                                                       std::vector<int>{}))),
                  Pair(1, ElementsAre(MatchIndexingMap("(d0, d1) -> (d1)",
                                                       std::vector<int>{}))),
                  Pair(2, ElementsAre(MatchIndexingMap("()[s0] -> (s0)",
                                                       std::vector<int>{10}))),
                  Pair(3, ElementsAre(MatchIndexingMap(
                              "()[s0] -> (s0)", std::vector<int>{10})))));
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
  EXPECT_FALSE(input_indexing.Simplify({1, 17, 9, 9}));
  EXPECT_THAT(
      input_indexing.indexing_maps,
      ElementsAre(Pair(0, ElementsAre(MatchIndexingMap(
                              "(d0, d1, d2, d3) -> (d0, -d1 + 16, -d2 + 8, d3)",
                              std::vector<int>{})))));

  TF_ASSERT_OK_AND_ASSIGN(auto output_indexing,
                          GetInputToOutputIndexingForEntryComputation(ir));
  EXPECT_FALSE(output_indexing.Simplify({1, 17, 9, 9}));
  EXPECT_THAT(
      output_indexing.indexing_maps,
      ElementsAre(Pair(0, ElementsAre(MatchIndexingMap(
                              "(d0, d1, d2, d3) -> (d0, -d1 + 16, -d2 + 8, d3)",
                              std::vector<int>{})))));
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
      ROOT fusion = f32[10, 11] fusion(p0), kind=kLoop, calls=fused_computation
    }
  )"));
  EXPECT_TRUE(input_indexing.Simplify({10, 11}));
  EXPECT_THAT(
      input_indexing.indexing_maps,
      ElementsAre(Pair(0, ElementsAre(MatchIndexingMap("(d0, d1) -> (d0, d1)",
                                                       std::vector<int>{})))));
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
                              std::vector<int>{})))));
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
              ElementsAre(Pair(0, ElementsAre(MatchIndexingMap(
                                      "(d0, d1, d2, d3) -> (d0, d3, d1, d2)",
                                      std::vector<int>{})))));

  TF_ASSERT_OK_AND_ASSIGN(auto output_indexing,
                          GetInputToOutputIndexingForEntryComputation(ir));
  EXPECT_THAT(output_indexing.indexing_maps,
              ElementsAre(Pair(0, ElementsAre(MatchIndexingMap(
                                      "(d0, d1, d2, d3) -> (d0, d2, d3, d1)",
                                      std::vector<int>{})))));
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
              ElementsAre(Pair(0, ElementsAre(MatchIndexingMap(
                                      "(d0, d1, d2, d3) -> (d0, d3, d1, d2)",
                                      std::vector<int>{})))));
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
      UnorderedElementsAre(Pair(0, ElementsAre(MatchIndexingMap(
                                       "(d0, d1, d2, d3, d4, d5)[s0, s1] -> "
                                       "(d2, d1, s1, d3, s0, d0)",
                                       std::vector<int>{18, 17}))),
                           Pair(1, ElementsAre(MatchIndexingMap(
                                       "(d0, d1, d2, d3, d4, d5)[s0, s1] -> "
                                       "(s1, d0, d4, s0, d5, d1)",
                                       std::vector<int>{18, 17})))));
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

}  // namespace
}  // namespace gpu
}  // namespace xla
