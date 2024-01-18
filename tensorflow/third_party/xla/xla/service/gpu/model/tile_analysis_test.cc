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

#include "xla/service/gpu/model/tile_analysis.h"

#include <optional>

#include <gmock/gmock.h>
#include <gtest/gtest.h>
#include "absl/strings/str_cat.h"
#include "absl/strings/string_view.h"
#include "mlir/IR/MLIRContext.h"  // from @llvm-project
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/hlo/ir/hlo_opcode.h"
#include "xla/service/gpu/model/indexing_analysis.h"
#include "xla/tests/hlo_test_base.h"
#include "tsl/platform/test.h"

namespace xla {
namespace gpu {
namespace {

using ::testing::ExplainMatchResult;
using ::testing::Optional;
using ::testing::StrEq;

MATCHER_P3(MatchSymbolicTile, offset_map_string, size_map_string,
           stride_map_string,
           absl::StrCat(negation
                            ? "equals "
                            : "doesn't equal symbolic tile with offset_map_ ",
                        offset_map_string, " and size_map_ ", size_map_string,
                        " and stride_map_ ", stride_map_string)) {
  return ExplainMatchResult(StrEq(offset_map_string),
                            ToString(arg.offset_map()), result_listener) &&
         ExplainMatchResult(StrEq(size_map_string), ToString(arg.size_map()),
                            result_listener) &&
         ExplainMatchResult(StrEq(stride_map_string),
                            ToString(arg.stride_map()), result_listener);
}

class SymbolicTileTest : public HloTestBase {
 public:
  HloInstructionIndexing GetOutputToInputIndexingForEntryComputation(
      absl::string_view hlo_string, int output_id = 0) {
    auto module = ParseAndReturnVerifiedModule(hlo_string);
    EXPECT_TRUE(module.ok());

    HloInstruction* root =
        module.value()->entry_computation()->root_instruction();

    // If there are multiple instructions, they need to be wrapped in a fusion.
    for (auto* operand : root->operands()) {
      if (operand->opcode() != HloOpcode::kParameter &&
          operand->opcode() != HloOpcode::kConstant) {
        return {};
      }
    }
    return ComputeOutputToInputIndexing(root, output_id, &mlir_context_);
  }
  mlir::MLIRContext mlir_context_;
};


TEST_F(SymbolicTileTest,
       CanPropagateTileFromDotOutputToInputsWithoutSpecializedTileSizes) {
  auto input_indexing = GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      p0 = f32[11, 17, 19] parameter(0)
      p1 = f32[11, 19, 23] parameter(1)
      ROOT dot = f32[11, 17, 23] dot(p0, p1),
        lhs_batch_dims={0}, rhs_batch_dims={0},
        lhs_contracting_dims={2}, rhs_contracting_dims={1}
    }
  )");

  EXPECT_THAT(
      SymbolicTile::FromIndexingMap(**input_indexing.indexing_maps[0].begin()),
      Optional(MatchSymbolicTile(
          "()[s0, s1, s2, s3, s4, s5, s6, s7, s8] -> (s0, s3, 0)",
          "()[s0, s1, s2, s3, s4, s5, s6, s7, s8] -> (s1, s4, 19)",
          "()[s0, s1, s2, s3, s4, s5, s6, s7, s8] -> (s2, s5, 1)")));

  EXPECT_THAT(
      SymbolicTile::FromIndexingMap(**input_indexing.indexing_maps[1].begin()),
      Optional(MatchSymbolicTile(
          "()[s0, s1, s2, s3, s4, s5, s6, s7, s8] -> (s0, 0, s6)",
          "()[s0, s1, s2, s3, s4, s5, s6, s7, s8] -> (s1, 19, s7)",
          "()[s0, s1, s2, s3, s4, s5, s6, s7, s8] -> (s2, 1, s8)")));
}

TEST_F(SymbolicTileTest, CanPropagateTileThroughTrivialReshape) {
  auto input_indexing = GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      p0 = f32[11, 17, 19] parameter(0)
      ROOT reshape = f32[1, 11, 17, 19] reshape(p0)
    }
  )");

  EXPECT_THAT(
      SymbolicTile::FromIndexingMap(**input_indexing.indexing_maps[0].begin()),
      Optional(MatchSymbolicTile(
          "()[s0, s1, s2, s3, s4, s5, s6, s7, s8, s9, s10, s11] "
          "-> (s3, s6, s9)",
          "()[s0, s1, s2, s3, s4, s5, s6, s7, s8, s9, s10, s11] "
          "-> (s4, s7, s10)",
          "()[s0, s1, s2, s3, s4, s5, s6, s7, s8, s9, s10, s11] "
          "-> (s5, s8, s11)")));
}

TEST_F(SymbolicTileTest,
       FailsToPropagateTileThroughReshapeWithoutSpecializedTileSizes) {
  auto input_indexing = GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      p0 = f32[12, 4, 19] parameter(0)
      ROOT reshape = f32[4, 12, 19] reshape(p0)
    }
  )");

  EXPECT_EQ(
      SymbolicTile::FromIndexingMap(**input_indexing.indexing_maps[0].begin()),
      std::nullopt);
}

TEST_F(SymbolicTileTest,
       CanPropagateTileThroughElementwiseOpWithoutSpecializedTileSizes) {
  auto input_indexing = GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      p0 = f32[150] parameter(0)
      p1 = f32[150] parameter(1)
      ROOT add = f32[150] add(p0, p1)
    }
  )");

  EXPECT_THAT(
      SymbolicTile::FromIndexingMap(**input_indexing.indexing_maps[0].begin()),
      Optional(MatchSymbolicTile("()[s0, s1, s2] -> (s0)",
                                 "()[s0, s1, s2] -> (s1)",
                                 "()[s0, s1, s2] -> (s2)")));
}

TEST_F(SymbolicTileTest,
       CanPropagateTileFromBroadcastOutputToInputWithoutSpecializedTileSizes) {
  auto input_indexing = GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      p0 = f32[150] parameter(0)
      ROOT broadcast = f32[157,150] broadcast(p0), dimensions={1}
    }
  )");

  EXPECT_THAT(
      SymbolicTile::FromIndexingMap(**input_indexing.indexing_maps[0].begin()),
      Optional(MatchSymbolicTile("()[s0, s1, s2, s3, s4, s5] -> (s3)",
                                 "()[s0, s1, s2, s3, s4, s5] -> (s4)",
                                 "()[s0, s1, s2, s3, s4, s5] -> (s5)")));
}

TEST_F(SymbolicTileTest,
       CanPropagateTileFromReduceOutputToInputWithoutSpecializedTileSizes) {
  auto input_indexing = GetOutputToInputIndexingForEntryComputation(R"(
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
  )");

  EXPECT_THAT(
      SymbolicTile::FromIndexingMap(**input_indexing.indexing_maps[0].begin()),
      Optional(MatchSymbolicTile("()[s0, s1, s2] -> (0, s0)",
                                 "()[s0, s1, s2] -> (125, s1)",
                                 "()[s0, s1, s2] -> (1, s2)")));
}

TEST_F(SymbolicTileTest,
       CanPropagateTileThroughReverseWithoutSpecializedTileSizes) {
  auto input_indexing = GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      p0 = f32[179] parameter(0)
      ROOT reverse = f32[179] reverse(p0), dimensions={0}
    }
  )");

  EXPECT_THAT(
      SymbolicTile::FromIndexingMap(**input_indexing.indexing_maps[0].begin()),
      Optional(MatchSymbolicTile("()[s0, s1, s2] -> (-s0 - s2 * s1 + 178)",
                                 "()[s0, s1, s2] -> (s1)",
                                 "()[s0, s1, s2] -> (s2)")));
}

TEST_F(SymbolicTileTest,
       CanPropagateTileFromSliceOutputToInputWithoutSpecializedTileSizes) {
  auto input_indexing = GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      p0 = f32[120,142] parameter(0)
      ROOT slice = f32[10,21] slice(p0), slice={[40:60:2], [20:104:4]}
    }
  )");

  EXPECT_THAT(
      SymbolicTile::FromIndexingMap(**input_indexing.indexing_maps[0].begin()),
      Optional(MatchSymbolicTile(
          "()[s0, s1, s2, s3, s4, s5] -> (s0 * 2 + 40, s3 * 4 + 20)",
          "()[s0, s1, s2, s3, s4, s5] -> (s1, s4)",
          "()[s0, s1, s2, s3, s4, s5] -> (s2 * 2, s5 * 4)")));
}

TEST_F(SymbolicTileTest,
       CanPropagateTileThroughTransposeWithoutSpecializedTileSizes) {
  auto input_indexing = GetOutputToInputIndexingForEntryComputation(R"(
    HloModule m
    ENTRY e {
      p0 = f32[21,10] parameter(0)
      ROOT transpose = f32[10,21] transpose(p0), dimensions={1,0}
    }
  )");

  EXPECT_THAT(
      SymbolicTile::FromIndexingMap(**input_indexing.indexing_maps[0].begin()),
      Optional(MatchSymbolicTile("()[s0, s1, s2, s3, s4, s5] -> (s3, s0)",
                                 "()[s0, s1, s2, s3, s4, s5] -> (s4, s1)",
                                 "()[s0, s1, s2, s3, s4, s5] -> (s5, s2)")));
}

}  // namespace
}  // namespace gpu
}  // namespace xla
