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

#include "xla/service/gpu/split_k_gemm_rewriter.h"

#include <memory>
#include <string>

#include <gmock/gmock.h>
#include <gtest/gtest.h>
#include "absl/strings/str_format.h"
#include "absl/strings/string_view.h"
#include "xla/autotuning.pb.h"
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/hlo/ir/hlo_opcode.h"
#include "xla/layout.h"
#include "xla/service/gpu/gemm_rewriter_triton.h"
#include "xla/service/pattern_matcher.h"
#include "xla/service/pattern_matcher_gmock.h"
#include "xla/shape_util.h"
#include "xla/tests/hlo_test_base.h"
#include "xla/tests/verified_hlo_module.h"
#include "xla/xla.pb.h"
#include "xla/xla_data.pb.h"
#include "tsl/lib/core/status_test_util.h"
#include "tsl/platform/errors.h"
#include "tsl/platform/status_matchers.h"
#include "tsl/platform/statusor.h"

namespace xla {
namespace gpu {
namespace {

using ::testing::ElementsAre;
using ::testing::FieldsAre;

namespace m = ::xla::match;

TEST(HasDivisibleSuffixAllowingSplitTest, AllTests) {
  EXPECT_TRUE(HasDivisibleSuffixAllowingSplit({1}, 1));
  EXPECT_TRUE(HasDivisibleSuffixAllowingSplit({2}, 2));
  EXPECT_TRUE(HasDivisibleSuffixAllowingSplit({2, 2}, 2));
  EXPECT_TRUE(HasDivisibleSuffixAllowingSplit({3, 2}, 6));
  EXPECT_TRUE(HasDivisibleSuffixAllowingSplit({2, 3, 2}, 6));
  // True, because 15 can be rewritten as {5, 3}.
  EXPECT_TRUE(HasDivisibleSuffixAllowingSplit({15, 2}, 6));
  EXPECT_TRUE(HasDivisibleSuffixAllowingSplit({3, 15, 2}, 6));
  EXPECT_FALSE(HasDivisibleSuffixAllowingSplit({}, 1));
  EXPECT_FALSE(HasDivisibleSuffixAllowingSplit({1}, 2));
  EXPECT_FALSE(HasDivisibleSuffixAllowingSplit({3}, 2));
  EXPECT_FALSE(HasDivisibleSuffixAllowingSplit({2, 3}, 2));
}

using SplitKTest = HloTestBase;

TEST_F(SplitKTest, MakeSplitK) {
  const std::string hlo_text = R"(
HloModule t

triton_gemm_dot {
  parameter_0 = s8[3,128,5,32]{3,2,1,0} parameter(0)
  bitcast.1 = s8[3,5,32,128]{2,1,3,0} bitcast(parameter_0)
  copy.1 = s8[3,5,32,128]{3,2,1,0} copy(bitcast.1)
  reshape.5 = s8[480,128]{1,0} reshape(copy.1)
  convert.8 = bf16[480,128]{1,0} convert(reshape.5)
  parameter_1 = bf16[16,128]{1,0} parameter(1)
  ROOT dot.0 = bf16[480,16]{1,0} dot(convert.8, parameter_1),
    lhs_contracting_dims={1}, rhs_contracting_dims={1}
}

ENTRY e {
  p0 = s8[3,128,5,32]{3,2,1,0} parameter(0)
  p1 = bf16[16,128]{1,0} parameter(1)
  ROOT fusion = bf16[480,16]{1,0} fusion(p0, p1),
    kind=kCustom, calls=triton_gemm_dot, backend_config="__triton_gemm"
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  AutotuneResult::TritonGemmKey key;
  key.set_block_m(16);
  key.set_block_n(16);
  key.set_block_k(16);
  key.set_split_k(4);
  key.set_num_stages(1);
  key.set_num_warps(4);
  TF_EXPECT_OK(
      MakeDotSplitKBatch(module->entry_computation()->root_instruction(), key));
  EXPECT_EQ(module->entry_computation()->root_instruction()->opcode(),
            HloOpcode::kReduce);
}

TEST_F(SplitKTest, MakeSplitKWithOutputFusion) {
  const std::string hlo_text = R"(
HloModule t

triton_gemm_dot {
  p0 = f16[480,128]{1,0} parameter(0)
  p1 = f16[16,128]{1,0} parameter(1)
  d = f16[480,16]{1,0} dot(p0, p1),
    lhs_contracting_dims={1}, rhs_contracting_dims={1}
  c = bf16[] constant(123)
  n = bf16[] negate(c)
  bc = bf16[480,16]{1,0} broadcast(n)
  cv = bf16[480,16]{1,0} convert(d)
  ROOT a = bf16[480,16]{1,0} multiply(bc, cv)
}

ENTRY e {
  p0 = f16[480,128]{1,0} parameter(0)
  p1 = f16[16,128]{1,0} parameter(1)
  ROOT fusion = bf16[480,16]{1,0} fusion(p0, p1),
    kind=kCustom, calls=triton_gemm_dot, backend_config="__triton_gemm"
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  AutotuneResult::TritonGemmKey key;
  key.set_block_m(16);
  key.set_block_n(16);
  key.set_block_k(16);
  key.set_split_k(4);
  key.set_num_stages(1);
  key.set_num_warps(4);
  TF_EXPECT_OK(
      MakeDotSplitKBatch(module->entry_computation()->root_instruction(), key));
  EXPECT_EQ(module->entry_computation()->root_instruction()->opcode(),
            HloOpcode::kReduce);
}

TEST_F(SplitKTest, PreventSplitKWithNonDistributiveOperations) {
  const std::string hlo_text = R"(
HloModule t

triton_gemm_dot {
  p0 = f16[480,128]{1,0} parameter(0)
  p1 = f16[16,128]{1,0} parameter(1)
  d = f16[480,16]{1,0} dot(p0, p1),
    lhs_contracting_dims={1}, rhs_contracting_dims={1}
  c = f32[480,16]{1,0} convert(d)
  ROOT s = f32[480,16]{1,0} tanh(c)
}

ENTRY e {
  p0 = f16[480,128]{1,0} parameter(0)
  p1 = f16[16,128]{1,0} parameter(1)
  ROOT fusion = f32[480,16]{1,0} fusion(p0, p1),
    kind=kCustom, calls=triton_gemm_dot, backend_config="__triton_gemm"
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  AutotuneResult::TritonGemmKey key;
  key.set_block_m(16);
  key.set_block_n(16);
  key.set_block_k(16);
  key.set_split_k(4);
  key.set_num_stages(1);
  key.set_num_warps(4);
  EXPECT_THAT(
      MakeDotSplitKBatch(module->entry_computation()->root_instruction(), key),
      tsl::testing::StatusIs(
          tsl::error::CANCELLED,
          absl::StrFormat(
              "Operation non-distributive over addition after dot.")));
}

TEST_F(SplitKTest, MakeSplitKWithNonDivisibleDimensionSize) {
  constexpr absl::string_view kHloText = R"(
t {
  c1 = s32[] constant(1)
  bc1 = s32[31]{0} broadcast(c1), dimensions={}
  p0 = s32[31]{0} parameter(0)
  cmp = pred[31]{0} compare(bc1, p0), direction=EQ
  cvt = f32[31]{0} convert(cmp)
  bc2 = f32[17,31]{1,0} broadcast(cvt), dimensions={1}
  c0 = f32[] constant(0)
  bc0 = f32[17,16]{1,0} broadcast(c0), dimensions={}
  ROOT dot = f32[31,16]{1,0} dot(bc2, bc0),
    lhs_contracting_dims={0}, rhs_contracting_dims={0}
}

ENTRY e {
  p0 = s32[31]{0} parameter(0)
  ROOT r = f32[31,16]{1,0} fusion(p0),
    kind=kCustom, calls=t, backend_config="__triton_gemm"
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(kHloText));
  AutotuneResult::TritonGemmKey key;
  key.set_block_m(16);
  key.set_block_n(16);
  key.set_block_k(16);
  key.set_split_k(2);
  key.set_num_stages(1);
  key.set_num_warps(2);
  TF_EXPECT_OK(
      MakeDotSplitKBatch(module->entry_computation()->root_instruction(), key));
}

TEST_F(SplitKTest, AvoidSplitKWithSlicedContractingDimension) {
  const std::string hlo_text = R"(
t {
  p0 = f16[32,1234] parameter(0)
  s0 = f16[32,256] slice(p0), slice={[0:32], [41:297]}
  p1 = f16[256,768] parameter(1)
  ROOT d = f16[32,768] dot(s0, p1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

ENTRY e {
  p0 = f16[32,1234] parameter(0)
  p1 = f16[256,768] parameter(1)
  ROOT r = f16[32,768] fusion(p0, p1),
    kind=kCustom, calls=t, backend_config="__triton_gemm"
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  AutotuneResult::TritonGemmKey key;
  key.set_block_m(16);
  key.set_block_n(16);
  key.set_block_k(16);
  key.set_split_k(2);
  key.set_num_stages(1);
  key.set_num_warps(2);
  EXPECT_THAT(
      MakeDotSplitKBatch(module->entry_computation()->root_instruction(), key),
      tsl::testing::StatusIs(
          tsl::error::CANCELLED,
          absl::StrFormat(
              "Sliced contracting dimension is not supported yet.")));
}

TEST_F(SplitKTest, MakeSplitKWithNonStandardOutputLayout) {
  const std::string kHloText = R"(
HloModule t

triton_gemm_dot {
  parameter_0 = s8[3,128,5,32]{3,2,1,0} parameter(0)
  bitcast.1 = s8[3,5,32,128]{2,1,3,0} bitcast(parameter_0)
  copy.1 = s8[3,5,32,128]{3,2,1,0} copy(bitcast.1)
  reshape.5 = s8[480,128]{1,0} reshape(copy.1)
  convert.8 = bf16[480,128]{1,0} convert(reshape.5)
  parameter_1 = bf16[16,128]{1,0} parameter(1)
  ROOT dot.0 = bf16[480,16]{0,1} dot(convert.8, parameter_1),
    lhs_contracting_dims={1}, rhs_contracting_dims={1}
}

ENTRY e {
  p0 = s8[3,128,5,32]{3,2,1,0} parameter(0)
  p1 = bf16[16,128]{1,0} parameter(1)
  ROOT fusion = bf16[480,16]{0,1} fusion(p0, p1),
    kind=kCustom, calls=triton_gemm_dot, backend_config="__triton_gemm"
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(kHloText));
  AutotuneResult::TritonGemmKey key;
  key.set_block_m(16);
  key.set_block_n(16);
  key.set_block_k(16);
  key.set_split_k(4);
  key.set_num_stages(1);
  key.set_num_warps(4);

  TF_EXPECT_OK(
      MakeDotSplitKBatch(module->entry_computation()->root_instruction(), key));

  EXPECT_EQ(module->entry_computation()->root_instruction()->opcode(),
            HloOpcode::kReduce);
  EXPECT_EQ(module->entry_computation()->root_instruction()->shape().layout(),
            Layout({0, 1}));
}

TEST_F(SplitKTest, MakeSplitKWithExistingBatchDim) {
  const std::string hlo_text = R"(
HloModule m

triton_gemm_dot.24 {
  parameter_1 = bf16[1,1,800,5,128]{4,3,2,1,0} parameter(1)
  bitcast.3 = bf16[800,5,128]{2,1,0} bitcast(parameter_1)
  convert.3 = f32[800,5,128]{2,1,0} convert(bitcast.3)
  parameter_0 = f32[1,5,700,800]{3,2,1,0} parameter(0)
  bitcast.2 = f32[5,700,800]{2,1,0} bitcast(parameter_0)
  ROOT dot.26 = f32[5,128,700]{2,1,0} dot(convert.3, bitcast.2),
    lhs_batch_dims={1}, lhs_contracting_dims={0},
    rhs_batch_dims={0}, rhs_contracting_dims={2}
}

ENTRY e {
  tmp_3 = f32[1,5,700,800]{3,2,1,0} parameter(0)
  tmp_0 = bf16[1,1,800,5,128]{4,3,2,1,0} parameter(1)
  ROOT triton_gemm_dot.24 = f32[5,128,700]{2,1,0} fusion(tmp_3, tmp_0),
    kind=kCustom, calls=triton_gemm_dot.24,
    backend_config="__triton_gemm"
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  AutotuneResult::TritonGemmKey key;
  key.set_block_m(32);
  key.set_block_n(64);
  key.set_block_k(64);
  key.set_split_k(8);
  key.set_num_stages(1);
  key.set_num_warps(4);
  TF_EXPECT_OK(
      MakeDotSplitKBatch(module->entry_computation()->root_instruction(), key));
  EXPECT_EQ(module->entry_computation()->root_instruction()->opcode(),
            HloOpcode::kReduce);
}

TEST_F(SplitKTest, SupportsIndivisible) {
  constexpr absl::string_view kHloText = R"(
HloModule t

triton_gemm_dot {
  parameter_0 = s8[3,129,5,32]{3,2,1,0} parameter(0)
  bitcast.1 = s8[3,5,32,129]{2,1,3,0} bitcast(parameter_0)
  copy.1 = s8[3,5,32,129]{3,2,1,0} copy(bitcast.1)
  reshape.5 = s8[480,129]{1,0} reshape(copy.1)
  convert.8 = bf16[480,129]{1,0} convert(reshape.5)
  parameter_1 = bf16[16,129]{1,0} parameter(1)
  ROOT dot.0 = bf16[480,16]{1,0} dot(convert.8, parameter_1),
    lhs_contracting_dims={1}, rhs_contracting_dims={1}
}

ENTRY e {
  p0 = s8[3,129,5,32]{3,2,1,0} parameter(0)
  p1 = bf16[16,129]{1,0} parameter(1)
  ROOT fusion = bf16[480,16]{1,0} fusion(p0, p1),
    kind=kCustom, calls=triton_gemm_dot, backend_config="__triton_gemm"
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(kHloText));
  AutotuneResult::TritonGemmKey key;
  key.set_block_m(16);
  key.set_block_n(16);
  key.set_block_k(16);
  key.set_split_k(4);
  key.set_num_stages(1);
  key.set_num_warps(4);
  TF_EXPECT_OK(
      MakeDotSplitKBatch(module->entry_computation()->root_instruction(), key));
}

TEST_F(SplitKTest, SupportsIndivisibleSimpleSplitK4) {
  constexpr absl::string_view kHloText = R"(
HloModule t

triton_gemm_dot {
  parameter_0 = s8[480,129]{1,0} parameter(0)
  convert_0 = bf16[480,129]{1,0} convert(parameter_0)
  parameter_1 = bf16[16,129]{1,0} parameter(1)
  ROOT dot.0 = bf16[480,16]{1,0} dot(convert_0, parameter_1),
    lhs_contracting_dims={1}, rhs_contracting_dims={1}
}

ENTRY e {
  p0 = s8[480,129]{1,0} parameter(0)
  p1 = bf16[16,129]{1,0} parameter(1)
  ROOT fusion = bf16[480,16]{1,0} fusion(p0, p1),
    kind=kCustom, calls=triton_gemm_dot, backend_config="__triton_gemm"
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(kHloText));
  AutotuneResult::TritonGemmKey key;
  key.set_block_m(16);
  key.set_block_n(16);
  key.set_block_k(16);
  key.set_split_k(4);
  key.set_num_stages(1);
  key.set_num_warps(4);
  TF_EXPECT_OK(
      MakeDotSplitKBatch(module->entry_computation()->root_instruction(), key));
}

TEST_F(SplitKTest, SupportsIndivisibleSimpleSplitK16) {
  constexpr absl::string_view kHloText = R"(
HloModule t

triton_gemm_dot {
  parameter_0 = s8[480,255]{1,0} parameter(0)
  convert_0 = bf16[480,255]{1,0} convert(parameter_0)
  parameter_1 = bf16[16,255]{1,0} parameter(1)
  ROOT dot.0 = bf16[480,16]{1,0} dot(convert_0, parameter_1),
    lhs_contracting_dims={1}, rhs_contracting_dims={1}
}

ENTRY e {
  p0 = s8[480,255]{1,0} parameter(0)
  p1 = bf16[16,255]{1,0} parameter(1)
  ROOT fusion = bf16[480,16]{1,0} fusion(p0, p1),
    kind=kCustom, calls=triton_gemm_dot, backend_config="__triton_gemm"
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(kHloText));
  AutotuneResult::TritonGemmKey key;
  key.set_block_m(16);
  key.set_block_n(16);
  key.set_block_k(16);
  key.set_split_k(16);
  key.set_num_stages(1);
  key.set_num_warps(4);
  TF_EXPECT_OK(
      MakeDotSplitKBatch(module->entry_computation()->root_instruction(), key));
}

TEST_F(SplitKTest, SupportsIndivisibleWithTranspose) {
  constexpr absl::string_view kHloText = R"(
HloModule t

triton_gemm_dot {
  parameter_0 = s8[480,255]{1,0} parameter(0)
  convert_0 = bf16[480,255]{1,0} convert(parameter_0)
  transpose_0 = bf16[255,480]{1,0} transpose(convert_0), dimensions={1,0}
  parameter_1 = bf16[16,255]{1,0} parameter(1)
  ROOT dot.0 = bf16[480,16]{1,0} dot(transpose_0, parameter_1),
    lhs_contracting_dims={0}, rhs_contracting_dims={1}
}

ENTRY e {
  p0 = s8[480,255]{1,0} parameter(0)
  p1 = bf16[16,255]{1,0} parameter(1)
  ROOT fusion = bf16[480,16]{1,0} fusion(p0, p1),
    kind=kCustom, calls=triton_gemm_dot, backend_config="__triton_gemm"
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(kHloText));
  AutotuneResult::TritonGemmKey key;
  key.set_block_m(16);
  key.set_block_n(16);
  key.set_block_k(16);
  key.set_split_k(16);
  key.set_num_stages(1);
  key.set_num_warps(4);
  TF_EXPECT_OK(
      MakeDotSplitKBatch(module->entry_computation()->root_instruction(), key));
}

TEST_F(SplitKTest, SupportIndivisibleWithBroadcast) {
  constexpr absl::string_view kHloText = R"(
HloModule t

triton_gemm_dot {
  parameter_0 = s8[] parameter(0)
  convert_0 = bf16[] convert(parameter_0)
  broadcast_0 = bf16[480,255]{1,0} broadcast(convert_0)
  parameter_1 = bf16[16,255]{1,0} parameter(1)
  ROOT dot.0 = bf16[480,16]{1,0} dot(broadcast_0, parameter_1),
    lhs_contracting_dims={1}, rhs_contracting_dims={1}
}

ENTRY e {
  p0 = s8[] parameter(0)
  p1 = bf16[16,255]{1,0} parameter(1)
  ROOT fusion = bf16[480,16]{1,0} fusion(p0, p1),
    kind=kCustom, calls=triton_gemm_dot, backend_config="__triton_gemm"
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(kHloText));
  AutotuneResult::TritonGemmKey key;
  key.set_block_m(16);
  key.set_block_n(16);
  key.set_block_k(16);
  key.set_split_k(16);
  key.set_num_stages(1);
  key.set_num_warps(4);
  TF_EXPECT_OK(
      MakeDotSplitKBatch(module->entry_computation()->root_instruction(), key));
}

TEST_F(SplitKTest, SupportsIndivisibleWithBitcast) {
  constexpr absl::string_view kHloText = R"(
HloModule t

triton_gemm_dot {
  parameter_0 = s8[3,5,480,17]{3,0,1,2} parameter(0)
  convert_0 = bf16[3,5,480,17]{3,0,1,2} convert(parameter_0)
  bitcast_0 = bf16[480,255]{1,0} bitcast(convert_0)
  parameter_1 = bf16[16,255]{1,0} parameter(1)
  ROOT dot.0 = bf16[480,16]{1,0} dot(bitcast_0, parameter_1),
    lhs_contracting_dims={1}, rhs_contracting_dims={1}
}

ENTRY e {
  p0 = s8[3,5,480,17]{3,0,1,2} parameter(0)
  p1 = bf16[16,255]{1,0} parameter(1)
  ROOT fusion = bf16[480,16]{1,0} fusion(p0, p1),
    kind=kCustom, calls=triton_gemm_dot, backend_config="__triton_gemm"
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(kHloText));
  AutotuneResult::TritonGemmKey key;
  key.set_block_m(16);
  key.set_block_n(16);
  key.set_block_k(16);
  key.set_split_k(16);
  key.set_num_stages(1);
  key.set_num_warps(4);
  TF_EXPECT_OK(
      MakeDotSplitKBatch(module->entry_computation()->root_instruction(), key));
}

TEST_F(SplitKTest, SkipSmallK) {
  const std::string hlo_text = R"(
HloModule t

triton_gemm_dot {
  parameter_0 = s8[3,64,5,32]{3,2,1,0} parameter(0)
  bitcast.1 = s8[3,5,32,64]{2,1,3,0} bitcast(parameter_0)
  copy.1 = s8[3,5,32,64]{3,2,1,0} copy(bitcast.1)
  reshape.5 = s8[480,64]{1,0} reshape(copy.1)
  convert.8 = bf16[480,64]{1,0} convert(reshape.5)
  parameter_1 = bf16[16,64]{1,0} parameter(1)
  ROOT dot.0 = bf16[480,16]{1,0} dot(convert.8, parameter_1),
    lhs_contracting_dims={1}, rhs_contracting_dims={1}
}

ENTRY e {
  p0 = s8[3,64,5,32]{3,2,1,0} parameter(0)
  p1 = bf16[16,64]{1,0} parameter(1)
  ROOT fusion = bf16[480,16]{1,0} fusion(p0, p1),
    kind=kCustom, calls=triton_gemm_dot, backend_config="__triton_gemm"
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  AutotuneResult::TritonGemmKey key;
  key.set_block_m(16);
  key.set_block_n(16);
  key.set_block_k(128);
  key.set_split_k(4);
  key.set_num_stages(1);
  key.set_num_warps(4);
  EXPECT_THAT(
      MakeDotSplitKBatch(module->entry_computation()->root_instruction(), key),
      tsl::testing::StatusIs(
          tsl::error::CANCELLED,
          "Too small divisible part of the contracting dimension."));
}

TEST_F(SplitKTest, FragmentedKSupported) {
  const std::string hlo_text = R"(
HloModule t

triton_gemm_dot {
  p0 = f16[7,2,16,4,20] parameter(0)
  t0 = f16[2,16,4,20,7] transpose(p0), dimensions={1,2,3,4,0}
  b0 = f16[2560,7] bitcast(t0)
  a1 = f16[2560,5] parameter(1)
  ROOT r = f16[7,5] dot(b0, a1),
    lhs_contracting_dims={0}, rhs_contracting_dims={0}
}

ENTRY e {
  p0 = f16[7,2,16,4,20] parameter(0)
  p1 = f16[2560,5] parameter(1)
  ROOT fusion = f16[7,5] fusion(p0, p1),
    kind=kCustom, calls=triton_gemm_dot, backend_config="__triton_gemm"
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));

  AutotuneResult::TritonGemmKey key;
  key.set_block_m(32);
  key.set_block_n(32);
  key.set_block_k(16);
  key.set_num_stages(1);
  key.set_num_warps(4);

  // 5 divides the contracting dimension, but not its major subdimensions.
  key.set_split_k(5);
  EXPECT_THAT(
      MakeDotSplitKBatch(module->entry_computation()->root_instruction(), key),
      tsl::testing::StatusIs(tsl::error::CANCELLED,
                             "Contracting dimension is too fragmented."));

  // 8 fits the constraints.
  key.set_split_k(8);
  TF_EXPECT_OK(
      MakeDotSplitKBatch(module->entry_computation()->root_instruction(), key));
  const HloInstruction* root = module->entry_computation()->root_instruction();
  EXPECT_EQ(root->opcode(), HloOpcode::kReduce);
  const HloComputation* dot_computation = module->entry_computation()
                                              ->root_instruction()
                                              ->operand(0)
                                              ->called_computations()[0];
  const HloInstruction* p0 = dot_computation->parameter_instruction(0);
  TF_ASSERT_OK_AND_ASSIGN(
      const auto analysis,
      TritonFusionAnalysis::Execute(*dot_computation, key.split_k()));
  EXPECT_EQ(dot_computation->root_instruction()->shape(),
            ShapeUtil::MakeShapeWithDescendingLayout(F16, {8, 7, 5}));
  EXPECT_THAT(
      *analysis.IterSpec(TritonFusionAnalysis::Scope::LHS, p0, 1),
      ElementsAre(FieldsAre(/*stride=*/1, /*count=*/2560, /*slice_start=*/0,
                            /*slice_limit=*/2560,
                            /*subfragments=*/ElementsAre(20, 4, 4, 4, 2))));
}

TEST_F(SplitKTest, FragmentedKUnsupported) {
  const std::string hlo_text = R"(
HloModule t

triton_gemm_dot {
  p0 = f32[3,128,77] parameter(0)
  b0 = f32[384,77] bitcast(p0)
  a1 = f32[384,25] parameter(1)
  ROOT r = f32[77,25] dot(b0, a1),
    lhs_contracting_dims={0}, rhs_contracting_dims={0}
}

ENTRY e {
  p0 = f32[3,128,77] parameter(0)
  p1 = f32[384,25] parameter(1)
  ROOT fusion = f32[77,25] fusion(p0, p1),
    kind=kCustom, calls=triton_gemm_dot, backend_config="__triton_gemm"
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));

  AutotuneResult::TritonGemmKey key;
  key.set_block_m(16);
  key.set_block_n(16);
  key.set_block_k(16);
  key.set_num_stages(1);
  key.set_num_warps(4);
  key.set_split_k(4);
  // Because HasDivisibleSuffixAllowingSplit({128, 3}, 4) == false.
  EXPECT_THAT(
      MakeDotSplitKBatch(module->entry_computation()->root_instruction(), key),
      tsl::testing::StatusIs(tsl::error::CANCELLED,
                             "Contracting dimension is too fragmented."));
}

TEST_F(SplitKTest, MakeSplitKWithNonDefaultOutputLayout) {
  const std::string kHloText = R"(
triton_gemm_dot.4842_computation {
  parameter_0 = bf16[96,96]{1,0} parameter(0)
  parameter_1 = bf16[96,7]{1,0} parameter(1)
  dot.0 = bf16[96,7]{0,1} dot(parameter_0, parameter_1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
  ROOT bitcast.2 = bf16[7,3,32]{2,1,0} bitcast(dot.0)
}

ENTRY e {
  parameter_0.91 = bf16[96,96]{1,0} parameter(0)
  parameter_1.86 = bf16[96,7]{1,0} parameter(1)
  ROOT triton_gemm_dot.4842 = bf16[7,3,32]{2,1,0}
    fusion(parameter_0.91, parameter_1.86), kind=kCustom,
    calls=triton_gemm_dot.4842_computation
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(kHloText));
  AutotuneResult::TritonGemmKey key;
  key.set_block_m(16);
  key.set_block_n(16);
  key.set_block_k(16);
  key.set_split_k(2);
  key.set_num_stages(1);
  key.set_num_warps(4);
  TF_EXPECT_OK(
      MakeDotSplitKBatch(module->entry_computation()->root_instruction(), key));
  EXPECT_EQ(module->entry_computation()->root_instruction()->opcode(),
            HloOpcode::kReduce);
  const HloComputation* dot_computation = module->entry_computation()
                                              ->root_instruction()
                                              ->operand(0)
                                              ->called_computations()[0];
  TF_ASSERT_OK_AND_ASSIGN(const auto analysis,
                          TritonFusionAnalysis::Execute(*dot_computation));
}

class SplitKTestWithMorePreciseReduction
    : public HloTestBase,
      public ::testing::WithParamInterface<int> {
 public:
  DebugOptions GetDebugOptionsForTest() override {
    DebugOptions debug_options = HloTestBase::GetDebugOptionsForTest();
    debug_options.set_xla_gpu_triton_gemm_disable_reduced_precision_reduction(
        true);
    return debug_options;
  }
};

TEST_F(SplitKTestWithMorePreciseReduction, MakeSplitK) {
  constexpr absl::string_view kHloText = R"(
HloModule t

triton_gemm_dot {
  parameter_0 = s8[3,128,5,32]{3,2,1,0} parameter(0)
  bitcast.1 = s8[3,5,32,128]{2,1,3,0} bitcast(parameter_0)
  copy.1 = s8[3,5,32,128]{3,2,1,0} copy(bitcast.1)
  reshape.5 = s8[480,128]{1,0} reshape(copy.1)
  convert.8 = bf16[480,128]{1,0} convert(reshape.5)
  parameter_1 = bf16[16,128]{1,0} parameter(1)
  ROOT dot.0 = bf16[480,16]{1,0} dot(convert.8, parameter_1),
    lhs_contracting_dims={1}, rhs_contracting_dims={1}
}

ENTRY e {
  p0 = s8[3,128,5,32]{3,2,1,0} parameter(0)
  p1 = bf16[16,128]{1,0} parameter(1)
  ROOT fusion = bf16[480,16]{1,0} fusion(p0, p1),
    kind=kCustom, calls=triton_gemm_dot, backend_config="__triton_gemm"
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(kHloText));

  AutotuneResult::TritonGemmKey key;
  key.set_block_m(16);
  key.set_block_n(16);
  key.set_block_k(16);
  key.set_split_k(4);
  key.set_num_stages(1);
  key.set_num_warps(4);
  TF_EXPECT_OK(
      MakeDotSplitKBatch(module->entry_computation()->root_instruction(), key));

  EXPECT_THAT(module->entry_computation()->root_instruction(),
              GmockMatch(m::Convert(m::Reduce(m::Fusion(), m::Constant()))));
}

TEST_F(SplitKTestWithMorePreciseReduction, MakeSplitKWithOutputFusion) {
  const std::string hlo_text = R"(
HloModule t

triton_gemm_dot {
  p0 = f16[480,128]{1,0} parameter(0)
  p1 = f16[16,128]{1,0} parameter(1)
  d = f16[480,16]{1,0} dot(p0, p1),
    lhs_contracting_dims={1}, rhs_contracting_dims={1}
  c = bf16[] constant(123)
  n = bf16[] negate(c)
  bc = bf16[480,16]{1,0} broadcast(n)
  cv = bf16[480,16]{1,0} convert(d)
  ROOT a = bf16[480,16]{1,0} multiply(bc, cv)
}

ENTRY e {
  p0 = f16[480,128]{1,0} parameter(0)
  p1 = f16[16,128]{1,0} parameter(1)
  ROOT fusion = bf16[480,16]{1,0} fusion(p0, p1),
    kind=kCustom, calls=triton_gemm_dot, backend_config="__triton_gemm"
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  AutotuneResult::TritonGemmKey key;
  key.set_block_m(16);
  key.set_block_n(16);
  key.set_block_k(16);
  key.set_split_k(4);
  key.set_num_stages(1);
  key.set_num_warps(4);
  TF_EXPECT_OK(
      MakeDotSplitKBatch(module->entry_computation()->root_instruction(), key));
  EXPECT_THAT(module->entry_computation()->root_instruction(),
              GmockMatch(m::Convert(m::Reduce(m::Fusion(), m::Constant()))));
}

}  // namespace
}  // namespace gpu
}  // namespace xla
