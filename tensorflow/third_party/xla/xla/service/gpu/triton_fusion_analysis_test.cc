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

#include "xla/service/gpu/triton_fusion_analysis.h"

#include <memory>
#include <string>

#include <gmock/gmock.h>
#include <gtest/gtest.h>
#include "absl/strings/string_view.h"
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/hlo/ir/hlo_opcode.h"
#include "xla/service/gpu/gemm_rewriter_triton.h"
#include "xla/statusor.h"
#include "xla/stream_executor/device_description.h"
#include "xla/tests/hlo_test_base.h"
#include "xla/tests/verified_hlo_module.h"
#include "tsl/platform/statusor.h"

namespace xla {
namespace gpu {
namespace {

using ::testing::ElementsAre;
using ::testing::FieldsAre;

using TritonDotAnalysisTest = HloTestBase;

TEST_F(TritonDotAnalysisTest, NopBitcasts) {
  const std::string hlo_text = R"(
HloModule t

triton_dot {
  param_0.1 = s8[48,4]{1,0} parameter(0)
  bitcast.18 = s8[1,48,4]{2,1,0} bitcast(param_0.1)
  bitcast.19 = s8[48,4]{1,0} bitcast(bitcast.18)
  convert.4 = bf16[48,4]{1,0} convert(bitcast.19)
  param_1.1 = bf16[4,3]{1,0} parameter(1)
  ROOT dot = bf16[48,3]{1,0} dot(convert.4, param_1.1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

ENTRY e {
  p0 = s8[48,4]{1,0} parameter(0)
  p1 = bf16[4,3]{1,0} parameter(1)
  custom-call = bf16[48,3]{1,0} custom-call(p0, p1),
    custom_call_target="__triton",
    called_computations={triton_dot}
  ROOT bitcast.2 = bf16[1,8,6,3]{3,2,1,0} bitcast(custom-call)
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  const HloComputation* dot_computation = module->entry_computation()
                                              ->root_instruction()
                                              ->operand(0)
                                              ->called_computations()[0];
  const HloInstruction* p0 = dot_computation->parameter_instruction(0);
  const HloInstruction* p1 = dot_computation->parameter_instruction(1);
  TF_ASSERT_OK_AND_ASSIGN(const auto analysis,
                          TritonFusionAnalysis::Execute(*dot_computation));
  EXPECT_EQ(*analysis.ScopeParameters(TritonFusionAnalysis::Scope::LHS).begin(),
            p0);
  EXPECT_EQ(*analysis.ScopeParameters(TritonFusionAnalysis::Scope::RHS).begin(),
            p1);
  EXPECT_THAT(
      *analysis.IterSpec(TritonFusionAnalysis::Scope::LHS, p0, 0),
      ElementsAre(FieldsAre(/*stride=*/4, /*count=*/48, /*slice_start=*/0,
                            /*slice_limit=*/48, ElementsAre(48))));
  EXPECT_THAT(
      *analysis.IterSpec(TritonFusionAnalysis::Scope::LHS, p0, 1),
      ElementsAre(FieldsAre(/*stride=*/1, /*count=*/4, /*slice_start=*/0,
                            /*slice_limit=*/4, ElementsAre(4))));
  EXPECT_THAT(
      *analysis.IterSpec(TritonFusionAnalysis::Scope::RHS, p1, 0),
      ElementsAre(FieldsAre(/*stride=*/3, /*count=*/4, /*slice_start=*/0,
                            /*slice_limit=*/4, ElementsAre(4))));
  EXPECT_THAT(
      *analysis.IterSpec(TritonFusionAnalysis::Scope::RHS, p1, 1),
      ElementsAre(FieldsAre(/*stride=*/1, /*count=*/3, /*slice_start=*/0,
                            /*slice_limit=*/3, ElementsAre(3))));
}

TEST_F(TritonDotAnalysisTest, Merge) {
  const std::string hlo_text = R"(
HloModule t

triton_dot {
  param_0.1 = s8[1,8,6,4]{3,2,1,0} parameter(0)
  bitcast.18 = s8[48,4]{1,0} bitcast(param_0.1)
  convert.4 = bf16[48,4]{1,0} convert(bitcast.18)
  param_1.1 = bf16[4,3]{1,0} parameter(1)
  ROOT dot = bf16[48,3]{1,0} dot(convert.4, param_1.1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

ENTRY e {
  p0 = s8[1,8,6,4]{3,2,1,0} parameter(0)
  p1 = bf16[4,3]{1,0} parameter(1)
  custom-call = bf16[48,3]{1,0} custom-call(p0, p1),
    custom_call_target="__triton",
    called_computations={triton_dot}
  ROOT bitcast.2 = bf16[1,8,6,3]{3,2,1,0} bitcast(custom-call)
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  const HloComputation* dot_computation = module->entry_computation()
                                              ->root_instruction()
                                              ->operand(0)
                                              ->called_computations()[0];
  const HloInstruction* p0 = dot_computation->parameter_instruction(0);
  const HloInstruction* p1 = dot_computation->parameter_instruction(1);
  TF_ASSERT_OK_AND_ASSIGN(const auto analysis,
                          TritonFusionAnalysis::Execute(*dot_computation));
  EXPECT_EQ(*analysis.ScopeParameters(TritonFusionAnalysis::Scope::LHS).begin(),
            p0);
  EXPECT_EQ(*analysis.ScopeParameters(TritonFusionAnalysis::Scope::RHS).begin(),
            p1);
  EXPECT_THAT(*analysis.IterSpec(TritonFusionAnalysis::Scope::LHS, p0, 0),
              ElementsAre(FieldsAre(/*stride=*/4, /*count=*/6 * 8,
                                    /*slice_start=*/0, /*slice_limit=*/6 * 8,
                                    /*subfragments=*/ElementsAre(6, 8))));
  EXPECT_THAT(*analysis.IterSpec(TritonFusionAnalysis::Scope::LHS, p0, 1),
              ElementsAre(FieldsAre(/*stride=*/1, /*count=*/4,
                                    /*slice_start=*/0, /*slice_limit=*/4,
                                    /*subfragments=*/ElementsAre(4))));
  EXPECT_THAT(*analysis.IterSpec(TritonFusionAnalysis::Scope::RHS, p1, 0),
              ElementsAre(FieldsAre(/*stride=*/3, /*count=*/4,
                                    /*slice_start=*/0, /*slice_limit=*/4,
                                    /*subfragments=*/ElementsAre(4))));
  EXPECT_THAT(*analysis.IterSpec(TritonFusionAnalysis::Scope::RHS, p1, 1),
              ElementsAre(FieldsAre(/*stride=*/1, /*count=*/3,
                                    /*slice_start=*/0, /*slice_limit=*/3,
                                    /*subfragments=*/ElementsAre(3))));
}

TEST_F(TritonDotAnalysisTest, Split) {
  const std::string hlo_text = R"(
HloModule t

triton_dot {
  %parameter_1 = f32[24000,2]{1,0} parameter(1)
  %convert.15 = f16[24000,2]{1,0} convert(%parameter_1)
  %parameter_0 = f16[4]{0} parameter(0)
  %bitcast.45 = f16[2,2]{1,0} bitcast(%parameter_0)
  ROOT %dot.26 = f16[24000,2]{1,0} dot(%convert.15, %bitcast.45),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

ENTRY e {
  p0 = f16[4]{0} parameter(0)
  p1 = f32[24000,2]{1,0} parameter(1)
  ROOT r = f16[24000,2]{1,0} custom-call(p0, p1),
    custom_call_target="__triton",
    called_computations={triton_dot}
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  const HloComputation* dot_computation =
      module->entry_computation()->root_instruction()->called_computations()[0];
  const HloInstruction* p0 = dot_computation->parameter_instruction(0);
  const HloInstruction* p1 = dot_computation->parameter_instruction(1);
  TF_ASSERT_OK_AND_ASSIGN(const auto analysis,
                          TritonFusionAnalysis::Execute(*dot_computation));
  EXPECT_EQ(*analysis.ScopeParameters(TritonFusionAnalysis::Scope::LHS).begin(),
            p1);
  EXPECT_EQ(*analysis.ScopeParameters(TritonFusionAnalysis::Scope::RHS).begin(),
            p0);
  EXPECT_THAT(*analysis.IterSpec(TritonFusionAnalysis::Scope::LHS, p1, 0),
              ElementsAre(FieldsAre(/*stride=*/2, /*count=*/24000,
                                    /*slice_start=*/0, /*slice_limit=*/24000,
                                    /*subfragments=*/ElementsAre(24000))));
  EXPECT_THAT(*analysis.IterSpec(TritonFusionAnalysis::Scope::LHS, p1, 1),
              ElementsAre(FieldsAre(/*stride=*/1, /*count=*/2,
                                    /*slice_start=*/0, /*slice_limit=*/2,
                                    /*subfragments=*/ElementsAre(2))));
  EXPECT_THAT(*analysis.IterSpec(TritonFusionAnalysis::Scope::RHS, p0, 0),
              ElementsAre(FieldsAre(/*stride=*/2, /*count=*/2,
                                    /*slice_start=*/0, /*slice_limit=*/2,
                                    /*subfragments=*/ElementsAre(2))));
  EXPECT_THAT(*analysis.IterSpec(TritonFusionAnalysis::Scope::RHS, p0, 1),
              ElementsAre(FieldsAre(/*stride=*/1, /*count=*/2,
                                    /*slice_start=*/0, /*slice_limit=*/2,
                                    /*subfragments=*/ElementsAre(2))));
}

TEST_F(TritonDotAnalysisTest, TransposeMerge) {
  const std::string hlo_text = R"(
HloModule t

triton_dot {
  param_0.1 = s8[1,4,8,6]{3,2,1,0} parameter(0)
  transpose.3 = s8[1,8,6,4]{3,2,1,0} transpose(param_0.1), dimensions={0,2,3,1}
  bitcast.18 = s8[48,4]{1,0} bitcast(transpose.3)
  convert.4 = bf16[48,4]{1,0} convert(bitcast.18)
  param_1.1 = bf16[4,3]{1,0} parameter(1)
  ROOT dot = bf16[48,3]{1,0} dot(convert.4, param_1.1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

ENTRY e {
  p0 = s8[1,4,8,6]{3,2,1,0} parameter(0)
  p1 = bf16[4,3]{1,0} parameter(1)
  custom-call = bf16[48,3]{1,0} custom-call(p0, p1),
    custom_call_target="__triton",
    called_computations={triton_dot}
  ROOT bitcast.2 = bf16[1,8,6,3]{3,2,1,0} bitcast(custom-call)
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  const HloComputation* dot_computation = module->entry_computation()
                                              ->root_instruction()
                                              ->operand(0)
                                              ->called_computations()[0];
  const HloInstruction* p0 = dot_computation->parameter_instruction(0);
  const HloInstruction* p1 = dot_computation->parameter_instruction(1);
  TF_ASSERT_OK_AND_ASSIGN(const auto analysis,
                          TritonFusionAnalysis::Execute(*dot_computation));
  EXPECT_EQ(*analysis.ScopeParameters(TritonFusionAnalysis::Scope::LHS).begin(),
            p0);
  EXPECT_EQ(*analysis.ScopeParameters(TritonFusionAnalysis::Scope::RHS).begin(),
            p1);
  EXPECT_THAT(*analysis.IterSpec(TritonFusionAnalysis::Scope::LHS, p0, 0),
              ElementsAre(FieldsAre(/*stride=*/1, /*count=*/8 * 6,
                                    /*slice_start=*/0, /*slice_limit=*/8 * 6,
                                    /*subfragments=*/ElementsAre(6, 8))));
  EXPECT_THAT(*analysis.IterSpec(TritonFusionAnalysis::Scope::LHS, p0, 1),
              ElementsAre(FieldsAre(/*stride=*/8 * 6, /*count=*/4,
                                    /*slice_start=*/0, /*slice_limit=*/4,
                                    /*subfragments=*/ElementsAre(4))));
  EXPECT_THAT(*analysis.IterSpec(TritonFusionAnalysis::Scope::RHS, p1, 0),
              ElementsAre(FieldsAre(/*stride=*/3, /*count=*/4,
                                    /*slice_start=*/0, /*slice_limit=*/4,
                                    /*subfragments=*/ElementsAre(4))));
  EXPECT_THAT(*analysis.IterSpec(TritonFusionAnalysis::Scope::RHS, p1, 1),
              ElementsAre(FieldsAre(/*stride=*/1, /*count=*/3,
                                    /*slice_start=*/0, /*slice_limit=*/3,
                                    /*subfragments=*/ElementsAre(3))));
}

TEST_F(TritonDotAnalysisTest, CopyMerge) {
  const std::string hlo_text = R"(
HloModule t

triton_dot {
  param_0.1 = s8[1,4,8,6]{3,2,1,0} parameter(0)
  bitcast.99 = s8[1,8,6,4]{2,1,3,0} bitcast(param_0.1)
  copy.3 = s8[1,8,6,4]{3,2,1,0} copy(bitcast.99)
  bitcast.18 = s8[48,4]{1,0} bitcast(copy.3)
  convert.4 = bf16[48,4]{1,0} convert(bitcast.18)
  param_1.1 = bf16[4,3]{1,0} parameter(1)
  ROOT dot = bf16[48,3]{1,0} dot(convert.4, param_1.1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

ENTRY e {
  p0 = s8[1,4,8,6]{3,2,1,0} parameter(0)
  p1 = bf16[4,3]{1,0} parameter(1)
  custom-call = bf16[48,3]{1,0} custom-call(p0, p1),
    custom_call_target="__triton",
    called_computations={triton_dot}
  ROOT bitcast.2 = bf16[1,8,6,3]{3,2,1,0} bitcast(custom-call)
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  const HloComputation* dot_computation = module->entry_computation()
                                              ->root_instruction()
                                              ->operand(0)
                                              ->called_computations()[0];
  const HloInstruction* p0 = dot_computation->parameter_instruction(0);
  const HloInstruction* p1 = dot_computation->parameter_instruction(1);
  TF_ASSERT_OK_AND_ASSIGN(const auto analysis,
                          TritonFusionAnalysis::Execute(*dot_computation));
  EXPECT_EQ(*analysis.ScopeParameters(TritonFusionAnalysis::Scope::LHS).begin(),
            p0);
  EXPECT_EQ(*analysis.ScopeParameters(TritonFusionAnalysis::Scope::RHS).begin(),
            p1);
  EXPECT_THAT(*analysis.IterSpec(TritonFusionAnalysis::Scope::LHS, p0, 0),
              ElementsAre(FieldsAre(/*stride=*/1, /*count=*/8 * 6,
                                    /*slice_start=*/0, /*slice_limit=*/8 * 6,
                                    /*subfragments=*/ElementsAre(6, 8))));
  EXPECT_THAT(*analysis.IterSpec(TritonFusionAnalysis::Scope::LHS, p0, 1),
              ElementsAre(FieldsAre(/*stride=*/8 * 6, /*count=*/4,
                                    /*slice_start=*/0, /*slice_limit=*/4,
                                    /*subfragments=*/ElementsAre(4))));
  EXPECT_THAT(*analysis.IterSpec(TritonFusionAnalysis::Scope::RHS, p1, 0),
              ElementsAre(FieldsAre(/*stride=*/3, /*count=*/4,
                                    /*slice_start=*/0, /*slice_limit=*/4,
                                    /*subfragments=*/ElementsAre(4))));
  EXPECT_THAT(*analysis.IterSpec(TritonFusionAnalysis::Scope::RHS, p1, 1),
              ElementsAre(FieldsAre(/*stride=*/1, /*count=*/3,
                                    /*slice_start=*/0, /*slice_limit=*/3,
                                    /*subfragments=*/ElementsAre(3))));
}

TEST_F(TritonDotAnalysisTest, TransposeMergeNCN) {
  const std::string hlo_text = R"(
HloModule t

triton_dot {
  param_0.1 = bf16[3,4,8,1]{3,2,1,0} parameter(0)
  transpose.3 = bf16[3,8,1,4]{3,2,1,0} transpose(param_0.1), dimensions={0,2,3,1}
  bitcast.18 = bf16[24,4]{1,0} bitcast(transpose.3)
  param_1.1 = bf16[4,3]{1,0} parameter(1)
  ROOT dot = bf16[24,3]{1,0} dot(bitcast.18, param_1.1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

ENTRY e {
  p0 = bf16[3,4,8,1]{3,2,1,0} parameter(0)
  p1 = bf16[4,3]{1,0} parameter(1)
  custom-call = bf16[24,3]{1,0} custom-call(p0, p1),
    custom_call_target="__triton", called_computations={triton_dot}
  ROOT bitcast.2 = bf16[3,8,1,3]{3,2,1,0} bitcast(custom-call)
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  const HloComputation* dot_computation = module->entry_computation()
                                              ->root_instruction()
                                              ->operand(0)
                                              ->called_computations()[0];
  const HloInstruction* p0 = dot_computation->parameter_instruction(0);
  const HloInstruction* p1 = dot_computation->parameter_instruction(1);
  TF_ASSERT_OK_AND_ASSIGN(const auto analysis,
                          TritonFusionAnalysis::Execute(*dot_computation));
  EXPECT_EQ(*analysis.ScopeParameters(TritonFusionAnalysis::Scope::LHS).begin(),
            p0);
  EXPECT_EQ(*analysis.ScopeParameters(TritonFusionAnalysis::Scope::RHS).begin(),
            p1);
  EXPECT_THAT(*analysis.IterSpec(TritonFusionAnalysis::Scope::LHS, p0, 0),
              ElementsAre(FieldsAre(/*stride=*/1, /*count=*/8,
                                    /*slice_start=*/0, /*slice_limit=*/8,
                                    /*subfragments=*/ElementsAre(8)),
                          FieldsAre(/*stride=*/4 * 8, /*count=*/3,
                                    /*slice_start=*/0, /*slice_limit=*/3,
                                    /*subfragments=*/ElementsAre(3))));
  EXPECT_THAT(*analysis.IterSpec(TritonFusionAnalysis::Scope::LHS, p0, 1),
              ElementsAre(FieldsAre(/*stride=*/8, /*count=*/4,
                                    /*slice_start=*/0, /*slice_limit=*/4,
                                    /*subfragments=*/ElementsAre(4))));
  EXPECT_THAT(*analysis.IterSpec(TritonFusionAnalysis::Scope::RHS, p1, 0),
              ElementsAre(FieldsAre(/*stride=*/3, /*count=*/4,
                                    /*slice_start=*/0, /*slice_limit=*/4,
                                    /*subfragments=*/ElementsAre(4))));
  EXPECT_THAT(*analysis.IterSpec(TritonFusionAnalysis::Scope::RHS, p1, 1),
              ElementsAre(FieldsAre(/*stride=*/1, /*count=*/3,
                                    /*slice_start=*/0, /*slice_limit=*/3,
                                    /*subfragments=*/ElementsAre(3))));
}

TEST_F(TritonDotAnalysisTest, TransposeOutput) {
  const std::string hlo_text = R"(
HloModule t

triton_dot {
  p0 = bf16[24,4]{1,0} parameter(0)
  p1 = bf16[4,3]{1,0} parameter(1)
  dot = bf16[24,3]{1,0} dot(p0, p1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
  bc = bf16[12,2,3]{2,1,0} bitcast(dot)
  ROOT t = bf16[3,12,2]{2,1,0} transpose(bc), dimensions={2,0,1}
}

ENTRY e {
  p0 = bf16[24,4]{1,0} parameter(0)
  p1 = bf16[4,3]{1,0} parameter(1)
  ROOT r = bf16[3,12,2]{2,1,0} fusion(p0, p1), kind=kCustom,
    calls=triton_dot
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  const HloComputation* dot_computation =
      module->entry_computation()->root_instruction()->called_computations()[0];
  const HloInstruction* dot_output = dot_computation->root_instruction();
  TF_ASSERT_OK_AND_ASSIGN(const auto analysis,
                          TritonFusionAnalysis::Execute(*dot_computation));
  EXPECT_THAT(
      *analysis.IterSpec(TritonFusionAnalysis::Scope::OUTPUT, dot_output, 0),
      ElementsAre(FieldsAre(/*stride=*/1, /*count=*/24, /*slice_start=*/0,
                            /*slice_limit=*/24,
                            /*subfragments=*/ElementsAre(2, 12))));
  EXPECT_THAT(
      *analysis.IterSpec(TritonFusionAnalysis::Scope::OUTPUT, dot_output, 1),
      ElementsAre(FieldsAre(/*stride=*/24, /*count=*/3, /*slice_start=*/0,
                            /*slice_limit=*/3,
                            /*subfragments=*/ElementsAre(3))));
}

TEST_F(TritonDotAnalysisTest, OutputParameterIsHandled) {
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(R"(
HloModule t

triton_dot {
  p0 = bf16[24,4]{1,0} parameter(0)
  p1 = bf16[4,3]{1,0} parameter(1)
  dot = bf16[24,3]{1,0} dot(p0, p1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
  p2 = f16[3,24]{1,0} parameter(2)
  p2t = f16[24,3]{1,0} transpose(p2), dimensions={1,0}
  p2tc = bf16[24,3]{1,0} convert(p2t)
  ROOT r = bf16[24,3]{1,0} divide(p2tc, dot)
}

ENTRY e {
  p0 = bf16[24,4]{1,0} parameter(0)
  p1 = bf16[4,3]{1,0} parameter(1)
  p2 = f16[3,24]{1,0} parameter(2)
  ROOT r = bf16[24,3]{1,0} fusion(p0, p1, p2), kind=kCustom,
    calls=triton_dot
})"));
  const HloComputation* dot_computation =
      module->entry_computation()->root_instruction()->called_computations()[0];
  const HloInstruction* output_param =
      dot_computation->parameter_instruction(2);
  TF_ASSERT_OK_AND_ASSIGN(const auto analysis,
                          TritonFusionAnalysis::Execute(*dot_computation));
  EXPECT_EQ(
      analysis.IterSpec(TritonFusionAnalysis::Scope::OUTPUT, output_param, 0)
          ->size(),
      1);
  EXPECT_THAT(
      *analysis.IterSpec(TritonFusionAnalysis::Scope::OUTPUT, output_param, 0),
      ElementsAre(FieldsAre(/*stride=*/1, /*count=*/24, /*slice_start=*/0,
                            /*slice_limit=*/24,
                            /*subfragments=*/ElementsAre(24))));
  EXPECT_EQ(
      analysis.IterSpec(TritonFusionAnalysis::Scope::OUTPUT, output_param, 1)
          ->size(),
      1);
  EXPECT_THAT(
      *analysis.IterSpec(TritonFusionAnalysis::Scope::OUTPUT, output_param, 1),
      ElementsAre(FieldsAre(/*stride=*/24, /*count=*/3, /*slice_start=*/0,
                            /*slice_limit=*/3,
                            /*subfragments=*/ElementsAre(3))));
}

TEST_F(TritonDotAnalysisTest, InputBroadcastFromScalarIsHandled) {
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(R"(
HloModule t

triton_dot {
  p0 = bf16[24,4]{1,0} parameter(0)
  p1 = bf16[] parameter(1)
  p1b = bf16[4,3] broadcast(p1)
  ROOT dot = bf16[24,3]{1,0} dot(p0, p1b),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

ENTRY e {
  p0 = bf16[24,4]{1,0} parameter(0)
  p1 = bf16[] parameter(1)
  ROOT r = bf16[24,3]{1,0} fusion(p0, p1), kind=kCustom,
    calls=triton_dot
})"));
  const HloComputation* dot_computation =
      module->entry_computation()->root_instruction()->called_computations()[0];
  const HloInstruction* scalar = dot_computation->parameter_instruction(1);
  TF_ASSERT_OK_AND_ASSIGN(const auto analysis,
                          TritonFusionAnalysis::Execute(*dot_computation));
  EXPECT_EQ(analysis.IterSpec(TritonFusionAnalysis::Scope::RHS, scalar, 0),
            nullptr);
  EXPECT_EQ(analysis.IterSpec(TritonFusionAnalysis::Scope::RHS, scalar, 1),
            nullptr);
}

TEST_F(TritonDotAnalysisTest, InputBroadcastFromVectorIsHandled) {
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(R"(
HloModule t

triton_dot {
  p0 = bf16[24,4]{1,0} parameter(0)
  p1 = bf16[4] parameter(1)
  p1b = bf16[4,3] broadcast(p1), dimensions={0}
  ROOT dot = bf16[24,3]{1,0} dot(p0, p1b),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

ENTRY e {
  p0 = bf16[24,4]{1,0} parameter(0)
  p1 = bf16[4] parameter(1)
  ROOT r = bf16[24,3]{1,0} fusion(p0, p1), kind=kCustom,
    calls=triton_dot
})"));
  const HloComputation* dot_computation =
      module->entry_computation()->root_instruction()->called_computations()[0];
  const HloInstruction* vector = dot_computation->parameter_instruction(1);
  TF_ASSERT_OK_AND_ASSIGN(const auto analysis,
                          TritonFusionAnalysis::Execute(*dot_computation));
  EXPECT_EQ(
      analysis.IterSpec(TritonFusionAnalysis::Scope::RHS, vector, 0)->size(),
      1);
  EXPECT_THAT(*analysis.IterSpec(TritonFusionAnalysis::Scope::RHS, vector, 0),
              ElementsAre(FieldsAre(/*stride=*/1, /*count=*/4,
                                    /*slice_start=*/0, /*slice_limit=*/4,
                                    /*subfragments=*/ElementsAre(4))));
}

TEST_F(TritonDotAnalysisTest, OutputBroadcastIsNotAccepted) {
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(R"(
HloModule t

ENTRY e {
  p0 = f16[2,35] parameter(0)
  p0c = bf16[2,35] convert(p0)
  p1 = bf16[35,2] parameter(1)
  dot = bf16[2,2] dot(p0c, p1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
  ROOT bc = bf16[2,2,100] broadcast(dot), dimensions={0,1}
})"));
  EXPECT_TRUE(GemmRewriterTriton(se::CudaComputeCapability{
                                     se::CudaComputeCapability::AMPERE, 0})
                  .Run(module.get())
                  .value());
  EXPECT_EQ(module->entry_computation()->root_instruction()->opcode(),
            HloOpcode::kBroadcast);
}

TEST_F(TritonDotAnalysisTest, DegenerateSplitFragmentIsHandled) {
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(R"(
triton_gemm_r {
  Arg_0.1 = s8[30,913,8,21]{3,2,1,0} parameter(0)
  bitcast.6 = s8[30,8,21,913]{2,1,3,0} bitcast(Arg_0.1)
  copy.7 = s8[30,8,21,913]{3,2,1,0} copy(bitcast.6)
  bitcast.8 = s8[5040,913]{1,0} bitcast(copy.7)
  convert.9 = bf16[5040,913]{1,0} convert(bitcast.8)
  bitcast.32 = bf16[58,913]{1,0} parameter(1)
  dot.33 = bf16[5040,58]{1,0} dot(convert.9, bitcast.32),
    lhs_contracting_dims={1}, rhs_contracting_dims={1}
  bitcast.34 = bf16[30,8,21,58]{3,2,1,0} bitcast(dot.33)
  copy.35 = bf16[30,8,21,58]{2,1,3,0} copy(bitcast.34)
  ROOT bitcast.41 = bf16[30,1,58,8,21]{4,3,2,1,0} bitcast(copy.35)
}

ENTRY e {
  Arg_0.1 = s8[30,913,8,21]{3,2,1,0} parameter(0)
  Arg_1.2 = bf16[58,913]{1,0} parameter(1)
  ROOT r = bf16[30,1,58,8,21]{4,3,2,1,0} fusion(Arg_0.1, Arg_1.2), kind=kCustom,
    calls=triton_gemm_r,
    backend_config={kind: "__triton_gemm"}
})"));
  const HloComputation* dot_computation =
      module->entry_computation()->root_instruction()->called_computations()[0];
  TF_ASSERT_OK_AND_ASSIGN(const auto analysis,
                          TritonFusionAnalysis::Execute(*dot_computation));
  EXPECT_THAT(*analysis.IterSpec(TritonFusionAnalysis::Scope::OUTPUT,
                                 dot_computation->root_instruction(), 0),
              ElementsAre(FieldsAre(/*stride=*/1, /*count=*/8 * 21,
                                    /*slice_start=*/0, /*slice_limit=*/8 * 21,
                                    /*subfragments=*/ElementsAre(21, 8)),
                          FieldsAre(/*stride=*/8 * 21 * 58, /*count=*/30,
                                    /*slice_start=*/0, /*slice_limit=*/30,
                                    /*subfragments=*/ElementsAre(30))));
}

TEST_F(TritonDotAnalysisTest,
       HandlesFurtherPropagationFromTrivialSizedTensorGracefully) {
  // We could probably support this better, just checking to avoid a crash for
  // now.
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(R"(
triton_gemm_r {
  a = f32[3,3]{1,0} parameter(0)
  constant = f32[1,1]{1,0} constant({ {0} })
  broadcast = f32[1,1]{1,0} broadcast(constant), dimensions={0,1}
  reshape = f32[] reshape(broadcast)
  broadcast2 = f32[3,3]{1,0} broadcast(reshape), dimensions={}
  ROOT dot = f32[3,3]{1,0} dot(a, broadcast2),
                 lhs_contracting_dims={0}, rhs_contracting_dims={0}
}

ENTRY e {
  a = f32[3,3]{1,0} parameter(0)
  ROOT dot = f32[3,3]{1,0} fusion(a), kind=kCustom, calls=triton_gemm_r,
             backend_config={kind: "__triton_gemm"}
}
)"));

  const HloComputation* dot_computation =
      module->entry_computation()->root_instruction()->called_computations()[0];

  StatusOr<TritonFusionAnalysis> analysis =
      TritonFusionAnalysis::Execute(*dot_computation);
  // It can fail but shouldn't crash.
  (void)analysis;
}

using TritonSoftmaxAnalysisTest = HloTestBase;

TEST_F(TritonSoftmaxAnalysisTest, DegenerateBatchDimensionIsSupported) {
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(R"(
max {
  p1 = f32[] parameter(1)
  p0 = f32[] parameter(0)
  ROOT m = f32[] maximum(p0, p1)
}

triton_softmax_computation {
  p0 = f32[1,97]{1,0} parameter(0)
  bitcast = f32[97]{0} bitcast(p0)
  constant = f32[] constant(-inf)
  reduce = f32[] reduce(bitcast, constant), dimensions={0}, to_apply=max
  broadcast = f32[1,97]{1,0} broadcast(reduce), dimensions={}
  ROOT subtract = f32[1,97]{1,0} subtract(p0, broadcast)
}

ENTRY e {
  p0 = f32[1,97]{1,0} parameter(0)
  ROOT r = f32[1,97]{1,0} fusion(p0), kind=kCustom,
    calls=triton_softmax_computation,
    backend_config={"kind":"__triton_softmax"}
})"));
  const HloComputation* computation =
      module->entry_computation()->root_instruction()->called_computations()[0];
  TF_ASSERT_OK_AND_ASSIGN(const auto analysis,
                          TritonFusionAnalysis::Execute(*computation));
  EXPECT_THAT(*analysis.IterSpec(TritonFusionAnalysis::Scope::OUTPUT,
                                 computation->root_instruction(), 0),
              ElementsAre(FieldsAre(/*stride=*/1, /*count=*/97,
                                    /*slice_start=*/0, /*slice_limit=*/97,
                                    /*subfragments=*/ElementsAre(97))));
  EXPECT_EQ(analysis.IterSpec(TritonFusionAnalysis::Scope::OUTPUT,
                              computation->root_instruction(), 1),
            nullptr);
}

TEST_F(TritonSoftmaxAnalysisTest, BroadcastIntoBatchDimensionIsSupported) {
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(R"(
c {
  p1 = f32[127]{0} parameter(0)
  ROOT b = f32[125,127]{1,0} broadcast(p1), dimensions={1}
}

ENTRY e {
  p0 = f32[127]{0} parameter(0)
  ROOT t = f32[125,127]{1,0} fusion(p0), kind=kCustom, calls=c
})"));
  const HloComputation* computation =
      module->entry_computation()->root_instruction()->called_computations()[0];
  TF_ASSERT_OK_AND_ASSIGN(const auto analysis,
                          TritonFusionAnalysis::Execute(*computation));
  EXPECT_THAT(*analysis.IterSpec(TritonFusionAnalysis::Scope::OUTPUT,
                                 computation->root_instruction(), 0),
              ElementsAre(FieldsAre(/*stride=*/1, /*count=*/127,
                                    /*slice_start=*/0, /*slice_limit=*/127,
                                    /*subfragments=*/ElementsAre(127))));
  EXPECT_THAT(*analysis.IterSpec(TritonFusionAnalysis::Scope::OUTPUT,
                                 computation->root_instruction(), 1),
              ElementsAre(FieldsAre(/*stride=*/127, /*count=*/125,
                                    /*slice_start=*/0, /*slice_limit=*/125,
                                    /*subfragments=*/ElementsAre(125))));
  EXPECT_THAT(*analysis.IterSpec(TritonFusionAnalysis::Scope::OUTPUT,
                                 computation->parameter_instruction(0), 0),
              ElementsAre(FieldsAre(/*stride=*/1, /*count=*/127,
                                    /*slice_start=*/0, /*slice_limit=*/127,
                                    /*subfragments=*/ElementsAre(127))));
  EXPECT_EQ(analysis.IterSpec(TritonFusionAnalysis::Scope::OUTPUT,
                              computation->parameter_instruction(0), 1),
            nullptr);
}

TEST_F(TritonSoftmaxAnalysisTest, ReduceOfNonRowDimensionIsNotSupported) {
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(R"(
HloModule t
add {
  p0 = f32[] parameter(0)
  p1 = f32[] parameter(1)
  ROOT add = f32[] add(p0, p1)
}

triton_softmax_computation {
  param_0 = f32[8,4,127]{2,1,0} parameter(0)
  constant = f32[] constant(0)
  ROOT reduce = f32[4,127]{1,0} reduce(param_0, constant), dimensions={0}, to_apply=add
}

ENTRY main {
  param_0 = f32[8,4,127]{2,1,0} parameter(0)
  ROOT fusion = f32[4,127]{1,0} fusion(param_0), kind=kCustom,
    calls=triton_softmax_computation,
    backend_config={"kind":"__triton_softmax"}
})"));

  const HloComputation* computation =
      module->entry_computation()->root_instruction()->called_computations()[0];
  const auto analysis = TritonFusionAnalysis::Execute(*computation);
  EXPECT_FALSE(analysis.ok());
}

TEST_F(TritonSoftmaxAnalysisTest, PadWithinTritonSoftmaxIsNotSupported) {
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(R"(
HloModule t

add {
  p0 = f32[] parameter(0)
  p1 = f32[] parameter(1)
  ROOT add = f32[] add(p0, p1)
}

triton_softmax_computation {
  param_1 = f32[4,127]{1,0} parameter(0)
  constant_0 = f32[] constant(0)
  reduce = f32[4]{0} reduce(param_1,  constant_0), dimensions={1}, to_apply=add
  broadcast = f32[4,127]{1,0} broadcast(reduce), dimensions={0}
  ROOT pad = f32[8,127]{1,0} pad(broadcast, constant_0), padding=0_4x0_0
}

ENTRY main {
  param_0 = f32[4,127]{1,0} parameter(0)
  ROOT fusion = f32[8,127]{1,0} fusion(param_0), kind=kCustom,
    calls=triton_softmax_computation,
    backend_config={"kind":"__triton_softmax"}
})"));

  const HloComputation* computation =
      module->entry_computation()->root_instruction()->called_computations()[0];
  const auto analysis = TritonFusionAnalysis::Execute(*computation);
  EXPECT_FALSE(analysis.ok());
}

TEST_F(TritonSoftmaxAnalysisTest,
       BitcastWhichSplitsBatchAndReduceDimensionsIsNotSupported) {
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(R"(
add {
 p0 = f32[] parameter(0)
 p1 = f32[] parameter(1)
 ROOT add = f32[] add(p0, p1)
}

triton_softmax_computation {
  param_0 = f32[8,16129]{1,0} parameter(0)
  bitcast = f32[8,127,127]{2,1,0} bitcast(param_0)
  constant = f32[] constant(0)
  reduce = f32[8,127]{1,0} reduce(bitcast, f32[] constant), dimensions={2}, to_apply=add
  ROOT broadcast = f32[8,127,127]{2,1,0} broadcast(reduce), dimensions={0,1}
}

ENTRY main {
  param_1 = f32[8,16129]{1,0} parameter(0)
  ROOT fusion = f32[8,127,127]{2,1,0} fusion(param_1), kind=kCustom,
   calls=triton_softmax_computation,
   backend_config={"kind":"__triton_softmax"}
})"));

  const HloComputation* computation =
      module->entry_computation()->root_instruction()->called_computations()[0];
  const auto analysis = TritonFusionAnalysis::Execute(*computation);
  EXPECT_FALSE(analysis.ok());
}

TEST_F(TritonSoftmaxAnalysisTest,
       BitcastWhichSplitsReduceDimensionIsSupported) {
  // Clone of BitcastWhichSplitsBatchAndReduceDimensionsIsNotSupported,
  // But in this case the split dimension can be fully tiled as a reduce dim.
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(R"(
add {
 p0 = f32[] parameter(0)
 p1 = f32[] parameter(1)
 ROOT add = f32[] add(p0, p1)
}

triton_softmax_computation {
  param_0 = f32[1,8,127,128]{3,2,1,0} parameter(0)
  intermediate_bitcast = f32[8,127,2,64]{3,2,1,0} bitcast(param_0)
  bitcast = f32[8,127,128]{2,1,0} bitcast(intermediate_bitcast)
  constant = f32[] constant(0)
  reduce = f32[8,127]{1,0} reduce(bitcast, constant), dimensions={2}, to_apply=add
  ROOT broadcast = f32[8,127,128]{2,1 ,0} broadcast(reduce), dimensions={0,1}
}

ENTRY main {
  param_1 = f32[1,8,127,128]{3,2,1,0} parameter(0)
  ROOT fusion = f32[8,127,128]{2,1,0} fusion(param_1), kind=kCustom,
   calls=triton_softmax_computation,
   backend_config={"kind":"__triton_softmax"}
})"));

  const HloComputation* computation =
      module->entry_computation()->root_instruction()->called_computations()[0];
  TF_ASSERT_OK_AND_ASSIGN(const auto analysis,
                          TritonFusionAnalysis::Execute(*computation));
}

TEST_F(TritonSoftmaxAnalysisTest,
       BitcastWhichDoesNotAffectReduceDimIsSupported) {
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> module,
                          ParseAndReturnVerifiedModule(R"(
add {
 p0 = f32[] parameter(0)
 p1 = f32[] parameter(1)
 ROOT add = f32[] add(p0, p1)
}

triton_softmax_computation {
  param_0 = f32[1,2,4,127,128]{4,3,2,1,0} parameter(0)
  bitcast = f32[8,127,128]{2,1,0} bitcast(param_0)
  constant = f32[] constant(0)
  reduce = f32[8,127]{1,0} reduce(bitcast, constant), dimensions={2}, to_apply=add
  ROOT broadcast = f32[8,127,128]{2,1,0} broadcast(reduce), dimensions={0,1}
}

ENTRY main {
  param_1 = f32[1,2,4,127,128]{4,3,2,1,0} parameter(0)
  ROOT fusion =  f32[8,127,128]{2,1,0} fusion(param_1), kind=kCustom,
   calls=triton_softmax_computation,
   backend_config={"kind":"__triton_softmax"}
})"));

  const HloComputation* computation =
      module->entry_computation()->root_instruction()->called_computations()[0];
  TF_ASSERT_OK_AND_ASSIGN(const auto analysis,
                          TritonFusionAnalysis::Execute(*computation));
}

}  // namespace
}  // namespace gpu
}  // namespace xla
