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

#include "xla/service/gpu/ir_emission_utils.h"

#include <cstdint>
#include <cstring>
#include <memory>
#include <vector>

#include "mlir/Dialect/Func/IR/FuncOps.h"  // from @llvm-project
#include "mlir/IR/Builders.h"  // from @llvm-project
#include "mlir/IR/BuiltinAttributes.h"  // from @llvm-project
#include "mlir/IR/BuiltinOps.h"  // from @llvm-project
#include "mlir/IR/DialectRegistry.h"  // from @llvm-project
#include "mlir/IR/MLIRContext.h"  // from @llvm-project
#include "mlir/IR/Operation.h"  // from @llvm-project
#include "mlir/Parser/Parser.h"  // from @llvm-project
#include "mlir/Support/LLVM.h"  // from @llvm-project
#include "xla/hlo/ir/hlo_opcode.h"
#include "xla/literal.h"
#include "xla/literal_util.h"
#include "xla/mlir_hlo/lhlo/IR/lhlo_ops.h"
#include "xla/mlir_hlo/mhlo/IR/hlo_ops.h"
#include "xla/tests/hlo_test_base.h"
#include "xla/translate/hlo_to_mhlo/hlo_utils.h"
#include "xla/types.h"
#include "xla/util.h"
#include "tsl/lib/core/status_test_util.h"
#include "tsl/platform/statusor.h"
#include "tsl/platform/test.h"

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

  auto result = GetDescriptionForTiledTransposeEmitter(*tr, *tr);
  EXPECT_TRUE(result.has_value());
  EXPECT_EQ(result->instr, tr);
  EXPECT_EQ(result->dimensions, Vector3({1, 64, 1536}));
  EXPECT_EQ(result->permutation, Vector3({0, 2, 1}));
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

  HloInstruction* r = module->entry_computation()->root_instruction();
  auto result = GetDescriptionForTiledTransposeEmitter(*r, *r);
  EXPECT_TRUE(result.has_value());
  EXPECT_EQ(result->instr, r);
  EXPECT_EQ(result->dimensions, Vector3({64, 48, 32}));
  EXPECT_EQ(result->permutation, Vector3({2, 1, 0}));
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
  auto result = GetDescriptionForTiledTransposeEmitter(*r, *r->operand(0));
  EXPECT_TRUE(result.has_value());
  EXPECT_EQ(result->instr, r->operand(0));
  EXPECT_EQ(result->dimensions, Vector3({64, 48, 32}));
  EXPECT_EQ(result->permutation, Vector3({2, 1, 0}));
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
  EXPECT_FALSE(
      GetDescriptionForTiledTransposeEmitter(*r, *r->operand(0)).has_value());
  EXPECT_EQ(FindNonTrivialHero(*r).name(), "t");
}

TEST_F(IrEmissionUtilsTest, FindReduceHeroEpilogueFusion) {
  const char* hlo = R"(
    HloModule module

    %add {
      %x = f32[] parameter(0)
      %y = f32[] parameter(1)
      ROOT %add = f32[] add(%x, %y)
    }

    %fused_computation (param_0.4: f32[128,64], param_1.4: bf16[]) -> bf16[64] {
      %param_0 = f32[128,64]{1,0} parameter(0)
      %param_1 = bf16[] parameter(1)
      %convert.0 = f32[] convert(bf16[] %param_1)
      %reduce.0 = f32[64]{0} reduce(f32[128,64]{1,0} %param_0, f32[] %convert.0), dimensions={0}, to_apply=%add
      ROOT %convert.1 = bf16[64]{0} convert(f32[64]{0} %reduce.0)
    }

    ENTRY %main {
      %param_0 = f32[128,64]{1,0} parameter(0)
      %param_1 = bf16[] parameter(1)
      ROOT fusion = bf16[64]{0} fusion(%param_0, %param_1), kind=kInput, calls=fused_computation
    }
    )";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo));

  HloInstruction* r = module->entry_computation()->root_instruction();
  auto fusion = HloFusionAdaptor::ForInstruction(r);
  const auto& result =
      FindNonTrivialHero(fusion->GetRoots()[0].instruction(), *fusion);
  EXPECT_EQ(result.name(), "reduce.0");
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

  auto result = GetDescriptionForTiledTransposeEmitter(*r, *r->operand(0));
  EXPECT_TRUE(result.has_value());
  EXPECT_EQ(result->instr, r->operand(0));
  EXPECT_EQ(result->dimensions, Vector3({64, 48, 32}));
  EXPECT_EQ(result->permutation, Vector3({2, 1, 0}));
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
  auto result =
      GetDescriptionForTiledTransposeEmitter(*r, FindNonTrivialHero(*r));
  EXPECT_TRUE(result.has_value());
  EXPECT_EQ(result->instr, r->operand(0)->operand(0));
  EXPECT_EQ(result->dimensions, Vector3({64, 48, 32}));
  EXPECT_EQ(result->permutation, Vector3({2, 1, 0}));
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
  EXPECT_FALSE(
      GetDescriptionForTiledTransposeEmitter(*r, FindNonTrivialHero(*r))
          .has_value());
  EXPECT_EQ(&FindNonTrivialHero(*r), r);
}

TEST_F(IrEmissionUtilsTest, FindNonTrivialHeroOutsideFusion) {
  const char* hlo = R"(
HloModule module

f {
  p0 = f32[100,200,300]{2,1,0} parameter(0)
  ROOT add = f32[100,200,300]{2,1,0} add(p0, p0)
}

ENTRY entry {
  p0 = f32[300,200,100]{2,1,0} parameter(0)
  t = f32[100,200,300]{2,1,0} transpose(p0), dimensions={2,1,0}
  fusion = f32[100,200,300]{2,1,0} fusion(t), kind=kLoop, calls=f
  ROOT add = f32[100,200,300]{2,1,0} add(t, fusion)
}
)";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo));

  HloInstruction* r = module->GetComputationWithName("f")->root_instruction();
  HloInstruction* transpose =
      module->entry_computation()->GetInstructionWithName("t");
  HloInstruction* fusion =
      module->entry_computation()->GetInstructionWithName("fusion");
  EXPECT_EQ(
      &FindNonTrivialHero(*r, ProducerConsumerFusion(
                                  HloFusionAdaptor::ForInstruction(transpose),
                                  HloFusionAdaptor::ForInstruction(fusion))),
      transpose);
}

TEST_F(IrEmissionUtilsTest, FindNonTrivialHeroInsideFusion) {
  const char* hlo = R"(
HloModule module

f {
  p0 = f32[300,200,100]{2,1,0} parameter(0)
  t = f32[100,200,300]{2,1,0} transpose(p0), dimensions={2,1,0}
  ROOT add = f32[100,200,300]{2,1,0} add(t, t)
}

ENTRY entry {
  p0 = f32[300,200,100]{2,1,0} parameter(0)
  p1 = f32[100,200,300]{2,1,0} parameter(1)
  fusion = f32[100,200,300]{2,1,0} fusion(p0), kind=kLoop, calls=f
  ROOT add = f32[100,200,300]{2,1,0} add(p1, fusion)
}
)";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo));

  HloInstruction* r = module->entry_computation()->root_instruction();
  HloInstruction* transpose = module->GetComputationWithName("f")
                                  ->parameter_instruction(0)
                                  ->users()
                                  .front();
  HloInstruction* fusion =
      module->entry_computation()->GetInstructionWithName("fusion");
  EXPECT_EQ(
      &FindNonTrivialHero(
          *r, ProducerConsumerFusion(HloFusionAdaptor::ForInstruction(fusion),
                                     HloFusionAdaptor::ForInstruction(r))),
      transpose);
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
  auto result =
      GetDescriptionForTiledTransposeEmitter(*copy, FindNonTrivialHero(*copy));
  EXPECT_TRUE(result.has_value());
  EXPECT_EQ(result->instr, copy);
  EXPECT_EQ(result->dimensions, Vector3({8, 12, 1100}));
  EXPECT_EQ(result->permutation, Vector3({2, 1, 0}));
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
  auto result =
      GetDescriptionForTiledTransposeEmitter(*tr, FindNonTrivialHero(*tr));
  EXPECT_TRUE(result.has_value());
  EXPECT_EQ(result->instr, tr);
  EXPECT_EQ(result->dimensions, Vector3({8, 12, 1100}));
  EXPECT_EQ(result->permutation, Vector3({2, 1, 0}));
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
  auto result =
      GetDescriptionForTiledTransposeEmitter(*copy, FindNonTrivialHero(*copy));
  EXPECT_TRUE(result.has_value());
  EXPECT_EQ(result->instr, copy);
  EXPECT_EQ(result->dimensions, Vector3({1100, 12, 8}));
  EXPECT_EQ(result->permutation, Vector3({2, 1, 0}));
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
  auto result =
      GetDescriptionForTiledTransposeEmitter(*tr, FindNonTrivialHero(*tr));
  EXPECT_TRUE(result.has_value());
  EXPECT_EQ(result->instr, tr);
  EXPECT_EQ(result->dimensions, Vector3({1100, 12, 8}));
  EXPECT_EQ(result->permutation, Vector3({2, 1, 0}));
}

TEST_F(IrEmissionUtilsTest, LiteralToAttrToXlaFormat) {
  // int16, should be aliased.
  {
    Literal literal = LiteralUtil::CreateR2<int16_t>({{0, 1, 2}, {3, 4, 5}});

    TF_ASSERT_OK_AND_ASSIGN(DenseDataIntermediate data,
                            LiteralToXlaFormat(literal));
    EXPECT_EQ(data.span().size(), literal.size_bytes());
    EXPECT_EQ(reinterpret_cast<const char*>(data.span().data()),
              literal.untyped_data());
  }

  // int4, even, should be a new (unaliased) packed array.
  {
    Literal literal = LiteralUtil::CreateR2<s4>(
        {{s4(0), s4(1), s4(2)}, {s4(3), s4(4), s4(5)}});

    TF_ASSERT_OK_AND_ASSIGN(DenseDataIntermediate data,
                            LiteralToXlaFormat(literal));
    EXPECT_EQ(data.span(), std::vector<uint8_t>({0x01, 0x23, 0x45}));
    EXPECT_NE(reinterpret_cast<const void*>(data.span().data()),
              literal.untyped_data());
  }

  // int4, odd, should be a new (unaliased) packed array.
  {
    Literal literal = LiteralUtil::CreateR2<u4>(
        {{u4(0), u4(1), u4(2)}, {u4(3), u4(4), u4(5)}, {u4(6), u4(7), u4(8)}});

    TF_ASSERT_OK_AND_ASSIGN(DenseDataIntermediate data,
                            LiteralToXlaFormat(literal));
    EXPECT_EQ(data.span(),
              std::vector<uint8_t>({0x01, 0x23, 0x45, 0x67, 0x80}));
    EXPECT_NE(reinterpret_cast<const void*>(data.span().data()),
              literal.untyped_data());
  }
}

}  // namespace gpu
}  // namespace xla
