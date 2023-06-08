/* Copyright 2020 The TensorFlow Authors. All Rights Reserved.

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

#include "tensorflow/compiler/xla/service/gpu/ir_emission_utils.h"

#include "mlir/Dialect/Func/IR/FuncOps.h"  // from @llvm-project
#include "mlir/IR/Operation.h"  // from @llvm-project
#include "mlir/Parser/Parser.h"  // from @llvm-project
#include "tensorflow/compiler/xla/mlir_hlo/lhlo/IR/lhlo_ops.h"
#include "tensorflow/compiler/xla/tests/hlo_test_base.h"
#include "tensorflow/tsl/platform/test.h"

namespace xla {
namespace gpu {

class IrEmissionUtilsTest : public HloTestBase {};

TEST_F(IrEmissionUtilsTest, TestOperandPartitionNoAlias) {
  mlir::DialectRegistry registry;
  registry.insert<mlir::lmhlo::LmhloDialect>();
  registry.insert<mlir::func::FuncDialect>();
  mlir::MLIRContext context(registry);

  auto module = mlir::parseSourceString<mlir::ModuleOp>(R"(
    func.func @foo(%arg0 : memref<f32>, %arg1 : memref<f32>, %arg2 : memref<f32>) {
      "lmhlo.add" (%arg0, %arg1, %arg2) : (memref<f32>, memref<f32>, memref<f32>) -> ()
      "lmhlo.terminator" () : () -> ()
    }
  )",
                                                        &context);
  mlir::func::FuncOp func =
      mlir::cast<mlir::func::FuncOp>(module->lookupSymbol("foo"));
  mlir::Operation* op = &func.getBody().front().front();
  EXPECT_EQ(2, PartitionLmhloOperandsAndOutputs(op));
}

TEST_F(IrEmissionUtilsTest, TestOperandPartitionWithAlias0) {
  mlir::DialectRegistry registry;
  registry.insert<mlir::lmhlo::LmhloDialect>();
  registry.insert<mlir::func::FuncDialect>();
  mlir::MLIRContext context(registry);

  auto module = mlir::parseSourceString<mlir::ModuleOp>(R"(
    func.func @foo(%arg0 : memref<f32>, %arg1 : memref<f32>, %arg2 : memref<f32>) {
      "lmhlo.add" (%arg0, %arg1, %arg0) : (memref<f32>, memref<f32>, memref<f32>) -> ()
      "lmhlo.terminator" () : () -> ()
    }
  )",
                                                        &context);
  mlir::func::FuncOp func =
      mlir::cast<mlir::func::FuncOp>(module->lookupSymbol("foo"));
  mlir::Operation* op = &func.getBody().front().front();
  EXPECT_EQ(2, PartitionLmhloOperandsAndOutputs(op));
}

TEST_F(IrEmissionUtilsTest, TestOperandPartitionWithAlias1) {
  mlir::DialectRegistry registry;
  registry.insert<mlir::lmhlo::LmhloDialect>();
  registry.insert<mlir::func::FuncDialect>();
  mlir::MLIRContext context(registry);

  auto module = mlir::parseSourceString<mlir::ModuleOp>(R"(
    func.func @foo(%arg0 : memref<f32>, %arg1 : memref<f32>, %arg2 : memref<f32>) {
      "lmhlo.add" (%arg0, %arg1, %arg1) : (memref<f32>, memref<f32>, memref<f32>) -> ()
      "lmhlo.terminator" () : () -> ()
    }
  )",
                                                        &context);
  mlir::func::FuncOp func =
      mlir::cast<mlir::func::FuncOp>(module->lookupSymbol("foo"));
  mlir::Operation* op = &func.getBody().front().front();
  EXPECT_EQ(2, PartitionLmhloOperandsAndOutputs(op));
}

TEST_F(IrEmissionUtilsTest, FindTiledLogicalTranspose) {
  const char* hlo = R"(
HloModule module

ENTRY entry {
  p = f32[32,48,64]{2,1,0} parameter(0)
  ROOT t = f32[64,32,48]{2,1,0} transpose(p), dimensions={2,0,1}
}
)";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo));

  HloInstruction* tr = module->entry_computation()->root_instruction();

  EXPECT_EQ(*FindTiledLogicalTranspose(*tr),
            TransposeDescription(Vector3{1, 64, 1536}, Vector3{0, 2, 1}));
}

TEST_F(IrEmissionUtilsTest, FindAnyTiledTranspose) {
  const char* hlo = R"(
HloModule module

ENTRY entry {
  p = f32[32,48,64]{2,1,0} parameter(0)
  ROOT t = f32[64,48,32]{2,1,0} transpose(p), dimensions={2,1,0}
}
)";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo));

  HloInstruction* tr = module->entry_computation()->root_instruction();
  EXPECT_EQ(FindAnyTiledTranspose(*tr),
            std::make_optional(
                TransposeDescription(Vector3{64, 48, 32}, Vector3{2, 1, 0})));
}

TEST_F(IrEmissionUtilsTest, FindAnyTiledTransposeWithIntermediateUnaryOp) {
  const char* hlo = R"(
HloModule module

ENTRY entry {
  p = f32[32,48,64]{2,1,0} parameter(0)
  t = f32[64,48,32]{2,1,0} transpose(p), dimensions={2,1,0}
  ROOT n = f32[64,48,32]{2,1,0} negate(t)
}
)";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo));

  HloInstruction* r = module->entry_computation()->root_instruction();
  EXPECT_EQ(FindAnyTiledTranspose(*r),
            std::make_optional(
                TransposeDescription(Vector3{64, 48, 32}, Vector3{2, 1, 0})));
  EXPECT_EQ(&FindNonTrivialHero(*r), r->operand(0));
}

TEST_F(IrEmissionUtilsTest, FindAnyTiledTransposeWithIntermediateUnaryOpS8) {
  const char* hlo = R"(
HloModule module

ENTRY entry {
  p = f32[32,48,64]{2,1,0} parameter(0)
  t = f32[64,48,32]{2,1,0} transpose(p), dimensions={2,1,0}
  ROOT c = s8[64,48,32]{2,1,0} convert(t)
}
)";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo));

  HloInstruction* r = module->entry_computation()->root_instruction();
  // TODO(b/284431534): Update this test when the shared memory transpose
  // emitter is fast for S8 output.
  EXPECT_FALSE(FindAnyTiledTranspose(*r).has_value());
  EXPECT_EQ(&FindNonTrivialHero(*r), r->operand(0));
}

TEST_F(IrEmissionUtilsTest, FindAnyTiledTransposeWithIntermediateBinaryOp) {
  const char* hlo = R"(
HloModule module

ENTRY entry {
  p = f32[32,48,64]{2,1,0} parameter(0)
  p2 = f32[64,48,32]{2,1,0} parameter(1)
  t = f32[64,48,32]{2,1,0} transpose(p), dimensions={2,1,0}
  ROOT add = f32[64,48,32]{2,1,0} add(t, p2)
}
)";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo));

  HloInstruction* r = module->entry_computation()->root_instruction();
  EXPECT_EQ(FindAnyTiledTranspose(*r),
            std::make_optional(
                TransposeDescription(Vector3{64, 48, 32}, Vector3{2, 1, 0})));
  EXPECT_EQ(&FindNonTrivialHero(*r), r->operand(0));
}

TEST_F(IrEmissionUtilsTest, FindAnyTiledTransposeWithTwoIntermediateBinaryOps) {
  const char* hlo = R"(
HloModule module

ENTRY entry {
  p = f32[32,48,64]{2,1,0} parameter(0)
  p2 = f32[64,48,32]{2,1,0} parameter(1)
  p3 = f32[64,48,32]{2,1,0} parameter(2)
  t = f32[64,48,32]{2,1,0} transpose(p), dimensions={2,1,0}
  mul = f32[64,48,32]{2,1,0} multiply(t, p3)
  ROOT add = f32[64,48,32]{2,1,0} add(mul, p3)
}
)";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo));

  HloInstruction* r = module->entry_computation()->root_instruction();
  EXPECT_EQ(FindAnyTiledTranspose(*r),
            std::make_optional(
                TransposeDescription(Vector3{64, 48, 32}, Vector3{2, 1, 0})));
  EXPECT_EQ(&FindNonTrivialHero(*r), r->operand(0)->operand(0));
}

TEST_F(IrEmissionUtilsTest,
       FindAnyTiledTransposeWithIntermediateBinaryOpTwoTransposes) {
  const char* hlo = R"(
HloModule module

ENTRY entry {
  p = f32[32,48,64]{2,1,0} parameter(0)
  p2 = f32[48,32,64]{2,1,0} parameter(1)
  t = f32[64,48,32]{2,1,0} transpose(p), dimensions={2,1,0}
  t2 = f32[64,48,32]{2,1,0} transpose(p2), dimensions={2,0,1}
  ROOT add = f32[64,48,32]{2,1,0} add(t, t2)
}
)";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo));

  HloInstruction* r = module->entry_computation()->root_instruction();
  EXPECT_FALSE(FindAnyTiledTranspose(*r).has_value());
  EXPECT_EQ(&FindNonTrivialHero(*r), r);
}

TEST_F(IrEmissionUtilsTest, FindTiledTransposeOneSwapDimIsSmall) {
  const char* hlo = R"(
HloModule module

ENTRY entry {
  p = f32[100,11,12,8]{3,2,1,0} parameter(0)
  ROOT c = f32[100,11,12,8]{1,0,2,3} copy(p)
}
)";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo));

  HloInstruction* copy = module->entry_computation()->root_instruction();
  EXPECT_EQ(FindTiledTranspose(*copy),
            std::make_optional(
                TransposeDescription{Vector3{8, 12, 1100}, Vector3{2, 1, 0}}));
}

TEST_F(IrEmissionUtilsTest, FindTiledLogicalTransposeOneSwapDimIsSmall) {
  const char* hlo = R"(
HloModule module

ENTRY entry {
  p = f32[100,11,12,8]{3,2,1,0} parameter(0)
  ROOT t = f32[8,12,100,11]{3,2,1,0} transpose(p), dimensions={3,2,0,1}
}
)";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo));

  HloInstruction* tr = module->entry_computation()->root_instruction();
  EXPECT_EQ(FindTiledLogicalTranspose(*tr),
            std::make_optional(
                TransposeDescription{Vector3{8, 12, 1100}, Vector3{2, 1, 0}}));
}

TEST_F(IrEmissionUtilsTest, FindTiledTransposeOtherSwapDimIsSmall) {
  const char* hlo = R"(
HloModule module

ENTRY entry {
  p = f32[8,12,100,11]{3,2,1,0} parameter(0)
  ROOT c = f32[8,12,100,11]{0,1,3,2} copy(p)
}
)";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo));

  HloInstruction* copy = module->entry_computation()->root_instruction();

  EXPECT_EQ(FindTiledTranspose(*copy),
            std::make_optional(
                TransposeDescription{Vector3{1100, 12, 8}, Vector3{2, 1, 0}}));
}

TEST_F(IrEmissionUtilsTest, FindTiledLogicalTransposeOtherSwapDimIsSmall) {
  const char* hlo = R"(
HloModule module

ENTRY entry {
  p = f32[8,12,100,11]{3,2,1,0} parameter(0)
  ROOT t = f32[100,11,12,8]{3,2,1,0} transpose(p), dimensions={2,3,1,0}
}
)";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo));

  HloInstruction* tr = module->entry_computation()->root_instruction();

  EXPECT_EQ(FindTiledLogicalTranspose(*tr),
            std::make_optional(
                TransposeDescription{Vector3{1100, 12, 8}, Vector3{2, 1, 0}}));
}

}  // namespace gpu
}  // namespace xla
