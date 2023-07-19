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

#include <memory>
#include <optional>
#include <string>
#include <utility>

#include "tensorflow/compiler/xla/hlo/ir/hlo_computation.h"
#include "tensorflow/compiler/xla/hlo/ir/hlo_instruction.h"
#include "tensorflow/compiler/xla/hlo/ir/hlo_module.h"
#include "tensorflow/compiler/xla/service/collective_pipeliner.h"
#include "tensorflow/compiler/xla/service/hlo_dce.h"
#include "tensorflow/compiler/xla/service/hlo_parser.h"
#include "tensorflow/compiler/xla/service/hlo_pass_pipeline.h"
#include "tensorflow/compiler/xla/statusor.h"
#include "tensorflow/compiler/xla/tests/hlo_test_base.h"

namespace xla {
namespace {

using CollectivePipelinerExecutionTest = HloTestBase;

// Note: For testing the pipeliner transform, this test uses non-collective
// operations as stand-ins for collectives. This is sufficient to test the basic
// correctness of the pipelining transformation.
StatusOr<bool> RunOptimizer(
    HloModule* module, bool last_run, int64_t level_to_operate_on = 0,
    HloOpcode op = HloOpcode::kNegate,
    CollectivePipeliner::PipeliningDirection pipelining_direction =
        CollectivePipeliner::PipeliningDirection::kForward) {
  CollectivePipeliner::Config config = {
      /*op=*/op,
      /*level_to_operate_on=*/level_to_operate_on,
      /*max_pipelining_per_loop=*/INT64_MAX,
      /*last_run=*/last_run,
      /*process_different_sized_ops=*/true,
      /*/
      /*direction=*/
      pipelining_direction,
      /*should_process=*/HloPredicateTrue,
  };

  HloPassPipeline pass("optimizer");
  pass.AddPass<HloVerifier>(/*layout_sensitive=*/false,
                            /*allow_mixed_precision=*/false);
  pass.AddPass<CollectivePipeliner>(config);
  pass.AddPass<HloVerifier>(/*layout_sensitive=*/false,
                            /*allow_mixed_precision=*/false);
  pass.AddPass<HloDCE>(/*remove_cross_partition_collective_ops=*/true);
  return pass.Run(module);
}

TEST_F(CollectivePipelinerExecutionTest, TransformIncrementIndexByOne) {
  constexpr absl::string_view hlo_string = R"(
HloModule module

add {
  lhs = bf16[] parameter(0)
  rhs = bf16[] parameter(1)
  ROOT add = bf16[] add(lhs, rhs)
}

while_cond {
  param = (s32[], bf16[3,8,128]) parameter(0)
  gte = s32[] get-tuple-element(param), index=0
  constant.1 = s32[] constant(3)
  ROOT cmp = pred[] compare(gte, constant.1), direction=LT
}

while_body {
  param = (s32[], bf16[3,8,128]) parameter(0)
  get-tuple-element.394 = s32[] get-tuple-element(param), index=0
  get-tuple-element.395 = bf16[3,8,128] get-tuple-element(param), index=1
  constant.2557 = s32[] constant(1)
  add.230 = s32[] add(get-tuple-element.394, constant.2557)
  constant.2559 = s32[] constant(3)
  subtract.139 = s32[] subtract(constant.2559, get-tuple-element.394)
  constant.2560 = s32[] constant(-1)
  add.231 = s32[] add(subtract.139, constant.2560)
  constant.2561 = s32[] constant(0)
  compare.747 = pred[] compare(add.231, constant.2561), direction=LT
  constant.2562 = s32[] constant(2)
  add.232 = s32[] add(subtract.139, constant.2562)
  select.1348 = s32[] select(compare.747, add.232, add.231)
  dynamic-slice.99 = bf16[1,8,128] dynamic-slice(get-tuple-element.395, select.1348, constant.2561, constant.2561), dynamic_slice_sizes={1,8,128}
  mul = bf16[1,8,128] multiply(dynamic-slice.99, dynamic-slice.99)
  ar.1 = bf16[1,8,128] negate(mul)
  dynamic-update-slice.35 = bf16[3,8,128] dynamic-update-slice(get-tuple-element.395, ar.1, select.1348, constant.2561, constant.2561)
  ROOT tuple = (s32[], bf16[3,8,128]) tuple(add.230, dynamic-update-slice.35)
}

ENTRY entry {
  c0 = s32[] constant(0)
  p0 = bf16[3,8,128] parameter(0)
  tuple = (s32[], bf16[3,8,128]) tuple(c0, p0)
  while = (s32[], bf16[3,8,128]) while(tuple), condition=while_cond, body=while_body
  ROOT gte1 = bf16[3,8,128] get-tuple-element(while), index=1
}
)";
  auto module = ParseAndReturnUnverifiedModule(hlo_string).value();
  auto module2 = ParseAndReturnUnverifiedModule(hlo_string).value();
  EXPECT_TRUE(RunOptimizer(module.get(), /*last_run=*/true, 0).value());
  EXPECT_TRUE(RunOptimizer(module2.get(), /*last_run=*/true, 200).value());
  XLA_VLOG_LINES(1, module->ToString());
  XLA_VLOG_LINES(1, module2->ToString());
  EXPECT_TRUE(RunAndCompareTwoModules(std::move(module), std::move(module2),
                                      ErrorSpec{0.1, 0.1}));
}

TEST_F(CollectivePipelinerExecutionTest, PushAgOver) {
  constexpr absl::string_view hlo_string = R"(
HloModule module, entry_computation_layout={(bf16[3,8,128]{2,1,0})->bf16[3,8,128]{2,1,0}}

%add (lhs: bf16[], rhs: bf16[]) -> bf16[] {
  %lhs = bf16[] parameter(0)
  %rhs = bf16[] parameter(1)
  ROOT %add = bf16[] add(bf16[] %lhs, bf16[] %rhs)
}

%while_body.clone (loop_peel_param: (s32[], bf16[3,8,128], s32[])) -> (s32[], bf16[3,8,128], s32[]) {
  %loop_peel_param = (s32[], bf16[3,8,128]{2,1,0}, s32[]) parameter(0)
  %get-tuple-element.2 = s32[] get-tuple-element((s32[], bf16[3,8,128]{2,1,0}, s32[]) %loop_peel_param), index=0
  %constant.7 = s32[] constant(1)
  %add.4 = s32[] add(s32[] %get-tuple-element.2, s32[] %constant.7)
  %get-tuple-element.3 = bf16[3,8,128]{2,1,0} get-tuple-element((s32[], bf16[3,8,128]{2,1,0}, s32[]) %loop_peel_param), index=1
  %get-tuple-element.4 = s32[] get-tuple-element((s32[], bf16[3,8,128]{2,1,0}, s32[]) %loop_peel_param), index=2
  %constant.12 = s64[] constant(1)
  %custom-call = s32[] custom-call(s32[] %get-tuple-element.4, s64[] %constant.12), custom_call_target="InsertedByPreviousStep"
  %constant.13 = s32[] constant(0)
  %constant.10 = s32[] constant(0)
  %dynamic-slice.2 = bf16[1,8,128]{2,1,0} dynamic-slice(bf16[3,8,128]{2,1,0} %get-tuple-element.3, s32[] %custom-call, s32[] %constant.13, s32[] %constant.13), dynamic_slice_sizes={1,8,128}
  %ar.2 = bf16[1,8,128]{2,1,0} negate(bf16[1,8,128]{2,1,0} %dynamic-slice.2)
  %ag.2 = bf16[1,8,128]{2,1,0} negate(bf16[1,8,128]{2,1,0} %ar.2)
  %dynamic-update-slice.2 = bf16[3,8,128]{2,1,0} dynamic-update-slice(bf16[3,8,128]{2,1,0} %get-tuple-element.3, bf16[1,8,128]{2,1,0} %ag.2, s32[] %custom-call, s32[] %constant.13, s32[] %constant.13)
  %dynamic-slice.1 = bf16[1,8,128]{2,1,0} dynamic-slice(bf16[3,8,128]{2,1,0} %get-tuple-element.3, s32[] %get-tuple-element.2, s32[] %constant.10, s32[] %constant.10), dynamic_slice_sizes={1,8,128}
  %mul.2 = bf16[1,8,128]{2,1,0} multiply(bf16[1,8,128]{2,1,0} %dynamic-slice.1, bf16[1,8,128]{2,1,0} %dynamic-slice.1)
  %constant.15 = s32[] constant(0)
  %dynamic-update-slice.4 = bf16[3,8,128]{2,1,0} dynamic-update-slice(bf16[3,8,128]{2,1,0} %dynamic-update-slice.2, bf16[1,8,128]{2,1,0} %mul.2, s32[] %get-tuple-element.2, s32[] %constant.15, s32[] %constant.15)
  ROOT %tuple.3 = (s32[], bf16[3,8,128]{2,1,0}, s32[]) tuple(s32[] %add.4, bf16[3,8,128]{2,1,0} %dynamic-update-slice.4, s32[] %get-tuple-element.2)
}

%while_cond.clone (loop_peel_cond_param: (s32[], bf16[3,8,128], s32[])) -> pred[] {
  %loop_peel_cond_param = (s32[], bf16[3,8,128]{2,1,0}, s32[]) parameter(0)
  %gte.1 = s32[] get-tuple-element((s32[], bf16[3,8,128]{2,1,0}, s32[]) %loop_peel_cond_param), index=0
  %constant.6 = s32[] constant(0)
  ROOT %cmp.1 = pred[] compare(s32[] %gte.1, s32[] %constant.6), direction=LT
}

ENTRY %entry (p0: bf16[3,8,128]) -> bf16[3,8,128] {
  %c0 = s32[] constant(-3)
  %p0 = bf16[3,8,128]{2,1,0} parameter(0)
  %tuple.1 = (s32[], bf16[3,8,128]{2,1,0}) tuple(s32[] %c0, bf16[3,8,128]{2,1,0} %p0)
  %get-tuple-element.0 = s32[] get-tuple-element((s32[], bf16[3,8,128]{2,1,0}) %tuple.1), index=0
  %constant.0 = s32[] constant(1)
  %constant.4 = s32[] constant(0)
  %add.1 = s32[] add(s32[] %get-tuple-element.0, s32[] %constant.0)
  %get-tuple-element.1 = bf16[3,8,128]{2,1,0} get-tuple-element((s32[], bf16[3,8,128]{2,1,0}) %tuple.1), index=1
  %dynamic-slice.0 = bf16[1,8,128]{2,1,0} dynamic-slice(bf16[3,8,128]{2,1,0} %get-tuple-element.1, s32[] %get-tuple-element.0, s32[] %constant.4, s32[] %constant.4), dynamic_slice_sizes={1,8,128}
  %mul.1 = bf16[1,8,128]{2,1,0} multiply(bf16[1,8,128]{2,1,0} %dynamic-slice.0, bf16[1,8,128]{2,1,0} %dynamic-slice.0)
  %dynamic-update-slice.0 = bf16[3,8,128]{2,1,0} dynamic-update-slice(bf16[3,8,128]{2,1,0} %get-tuple-element.1, bf16[1,8,128]{2,1,0} %mul.1, s32[] %get-tuple-element.0, s32[] %constant.4, s32[] %constant.4)
  %tuple.4 = (s32[], bf16[3,8,128]{2,1,0}, s32[]) tuple(s32[] %add.1, bf16[3,8,128]{2,1,0} %dynamic-update-slice.0, s32[] %get-tuple-element.0)
  %while.1 = (s32[], bf16[3,8,128]{2,1,0}, s32[]) while((s32[], bf16[3,8,128]{2,1,0}, s32[]) %tuple.4), condition=%while_cond.clone, body=%while_body.clone
  %get-tuple-element.6 = bf16[3,8,128]{2,1,0} get-tuple-element((s32[], bf16[3,8,128]{2,1,0}, s32[]) %while.1), index=1
  %get-tuple-element.5 = s32[] get-tuple-element((s32[], bf16[3,8,128]{2,1,0}, s32[]) %while.1), index=2
  %constant.14 = s32[] constant(0)
  %dynamic-slice.3 = bf16[1,8,128]{2,1,0} dynamic-slice(bf16[3,8,128]{2,1,0} %get-tuple-element.6, s32[] %get-tuple-element.5, s32[] %constant.14, s32[] %constant.14), dynamic_slice_sizes={1,8,128}
  %ar.3 = bf16[1,8,128]{2,1,0} add(bf16[1,8,128]{2,1,0} %dynamic-slice.3, bf16[1,8,128]{2,1,0} %dynamic-slice.3)
  ROOT %dynamic-update-slice.3 = bf16[3,8,128]{2,1,0} dynamic-update-slice(bf16[3,8,128]{2,1,0} %get-tuple-element.6, bf16[1,8,128]{2,1,0} %ar.3, s32[] %get-tuple-element.5, s32[] %constant.14, s32[] %constant.14)
}
)";
  auto module = ParseAndReturnUnverifiedModule(hlo_string).value();
  auto module2 = ParseAndReturnUnverifiedModule(hlo_string).value();
  EXPECT_TRUE(RunOptimizer(module.get(), /*last_run=*/true, 1).value());
  EXPECT_TRUE(RunOptimizer(module2.get(), /*last_run=*/true, 200).value());
  XLA_VLOG_LINES(1, module->ToString());
  XLA_VLOG_LINES(1, module2->ToString());
  EXPECT_TRUE(RunAndCompareTwoModules(std::move(module), std::move(module2),
                                      ErrorSpec{0.1, 0.1}));
}

TEST_F(CollectivePipelinerExecutionTest,
       TransformIncrementIndexByOneNotFirstIdx) {
  constexpr absl::string_view hlo_string = R"(
 HloModule module

 add {
   lhs = bf16[] parameter(0)
   rhs = bf16[] parameter(1)
   ROOT add = bf16[] add(lhs, rhs)
 }

 while_cond {
   param = (s32[], bf16[8,3,128]) parameter(0)
   gte = s32[] get-tuple-element(param), index=0
   constant.1 = s32[] constant(3)
   ROOT cmp = pred[] compare(gte, constant.1), direction=LT
 }

 while_body {
   param = (s32[], bf16[8,3,128]) parameter(0)
   get-tuple-element.394 = s32[] get-tuple-element(param), index=0
   get-tuple-element.395 = bf16[8,3,128] get-tuple-element(param), index=1
   constant.2557 = s32[] constant(1)
   add.230 = s32[] add(get-tuple-element.394, constant.2557)
   constant.2559 = s32[] constant(3)
   subtract.139 = s32[] subtract(constant.2559, get-tuple-element.394)
   constant.2560 = s32[] constant(-1)
   add.231 = s32[] add(subtract.139, constant.2560)
   constant.2561 = s32[] constant(0)
   compare.747 = pred[] compare(add.231, constant.2561), direction=LT
   constant.2562 = s32[] constant(2)
   add.232 = s32[] add(subtract.139, constant.2562)
   select.1348 = s32[] select(compare.747, add.232, add.231)
   dynamic-slice.99 = bf16[8,1,128] dynamic-slice(get-tuple-element.395,
   constant.2561, select.1348, constant.2561), dynamic_slice_sizes={8,1,128}
   mul = bf16[8,1,128] multiply(dynamic-slice.99, dynamic-slice.99)
   ar.1 = bf16[8,1,128] negate(mul)
   dynamic-update-slice.35 = bf16[8,3,128]
   dynamic-update-slice(get-tuple-element.395, ar.1, constant.2561,
   select.1348, constant.2561) ROOT tuple = (s32[], bf16[8,3,128])
   tuple(add.230, dynamic-update-slice.35)
 }

 ENTRY entry {
   c0 = s32[] constant(0)
   p0 = bf16[8,3,128] parameter(0)
   tuple = (s32[], bf16[8,3,128]) tuple(c0, p0)
   while = (s32[], bf16[8,3,128]) while(tuple), condition=while_cond,
   body=while_body ROOT gte1 = bf16[8,3,128] get-tuple-element(while),
   index=1
 }
)";
  auto module = ParseAndReturnUnverifiedModule(hlo_string).value();
  auto module2 = ParseAndReturnUnverifiedModule(hlo_string).value();
  EXPECT_TRUE(RunOptimizer(module.get(), /*last_run=*/true, 0).value());
  EXPECT_TRUE(RunOptimizer(module2.get(), /*last_run=*/true, 200).value());
  XLA_VLOG_LINES(1, module->ToString());
  XLA_VLOG_LINES(1, module2->ToString());
  EXPECT_TRUE(RunAndCompareTwoModules(std::move(module), std::move(module2),
                                      ErrorSpec{0.1, 0.1}));
}

TEST_F(CollectivePipelinerExecutionTest, TransformIncrementByTwo) {
  constexpr absl::string_view hlo_string = R"(
 HloModule module

 add {
   lhs = bf16[] parameter(0)
   rhs = bf16[] parameter(1)
   ROOT add = bf16[] add(lhs, rhs)
 }

 while_cond {
   param = (s32[], bf16[3,8,128]) parameter(0)
   gte = s32[] get-tuple-element(param), index=0
   constant.1 = s32[] constant(3)
   ROOT cmp = pred[] compare(gte, constant.1), direction=LT
 }

 while_body {
   param = (s32[], bf16[3,8,128]) parameter(0)
   get-tuple-element.394 = s32[] get-tuple-element(param), index=0
   get-tuple-element.395 = bf16[3,8,128] get-tuple-element(param), index=1
   constant.2557 = s32[] constant(2)
   add.230 = s32[] add(get-tuple-element.394, constant.2557)
   constant.2559 = s32[] constant(3)
   subtract.139 = s32[] subtract(constant.2559, get-tuple-element.394)
   constant.2560 = s32[] constant(-1)
   add.231 = s32[] add(subtract.139, constant.2560)
   constant.2561 = s32[] constant(0)
   compare.747 = pred[] compare(add.231, constant.2561), direction=LT
   constant.2562 = s32[] constant(2)
   add.232 = s32[] add(subtract.139, constant.2562)
   select.1348 = s32[] select(compare.747, add.232, add.231)
   dynamic-slice.99 = bf16[1,8,128] dynamic-slice(get-tuple-element.395,
   select.1348, constant.2561, constant.2561), dynamic_slice_sizes={1,8,128}
   mul = bf16[1,8,128] multiply(dynamic-slice.99, dynamic-slice.99)
   ar.1 = bf16[1,8,128] negate(mul)
   dynamic-update-slice.35 = bf16[3,8,128]
   dynamic-update-slice(get-tuple-element.395, ar.1, select.1348,
   constant.2561, constant.2561) ROOT tuple = (s32[], bf16[3,8,128])
   tuple(add.230, dynamic-update-slice.35)
 }

 ENTRY entry {
   c0 = s32[] constant(0)
   p0 = bf16[3,8,128] parameter(0)
   tuple = (s32[], bf16[3,8,128]) tuple(c0, p0)
   while = (s32[], bf16[3,8,128]) while(tuple), condition=while_cond,
   body=while_body ROOT gte1 = bf16[3,8,128] get-tuple-element(while),
   index=1
 }
)";
  auto module = ParseAndReturnUnverifiedModule(hlo_string).value();
  auto module2 = ParseAndReturnUnverifiedModule(hlo_string).value();
  EXPECT_TRUE(RunOptimizer(module.get(), /*last_run=*/true, 0).value());
  EXPECT_TRUE(RunOptimizer(module2.get(), /*last_run=*/true, 200).value());
  XLA_VLOG_LINES(1, module->ToString());
  XLA_VLOG_LINES(1, module2->ToString());
  EXPECT_TRUE(RunAndCompareTwoModules(std::move(module), std::move(module2),
                                      ErrorSpec{0.1, 0.1}));
}

TEST_F(CollectivePipelinerExecutionTest, NoTransformCantProveIndexDoesntWrap) {
  constexpr absl::string_view hlo_string = R"(
 HloModule module

 add {
   lhs = bf16[] parameter(0)
   rhs = bf16[] parameter(1)
   ROOT add = bf16[] add(lhs, rhs)
 }

 while_cond {
   param = (s32[], bf16[3,8,128]) parameter(0)
   gte = s32[] get-tuple-element(param), index=0
   constant.1 = s32[] constant(4)
   ROOT cmp = pred[] compare(gte, constant.1), direction=LT
 }

 while_body {
   param = (s32[], bf16[3,8,128]) parameter(0)
   get-tuple-element.394 = s32[] get-tuple-element(param), index=0
   get-tuple-element.395 = bf16[3,8,128] get-tuple-element(param), index=1
   constant.2557 = s32[] constant(1)
   add.230 = s32[] add(get-tuple-element.394, constant.2557)
   constant.2559 = s32[] constant(3)
   subtract.139 = s32[] subtract(constant.2559, get-tuple-element.394)
   constant.2560 = s32[] constant(-1)
   add.231 = s32[] add(subtract.139, constant.2560)
   constant.2561 = s32[] constant(0)
   compare.747 = pred[] compare(add.231, constant.2561), direction=LT
   constant.2562 = s32[] constant(2)
   add.232 = s32[] add(subtract.139, constant.2562)
   select.1348 = s32[] select(compare.747, add.232, add.231)
   dynamic-slice.99 = bf16[1,8,128] dynamic-slice(get-tuple-element.395,
   select.1348, constant.2561, constant.2561), dynamic_slice_sizes={1,8,128}
   mul = bf16[1,8,128] multiply(dynamic-slice.99, dynamic-slice.99)
   ar.1 = bf16[1,8,128] negate(mul)
   dynamic-update-slice.35 = bf16[3,8,128]
   dynamic-update-slice(get-tuple-element.395, ar.1, select.1348,
   constant.2561, constant.2561) ROOT tuple = (s32[], bf16[3,8,128])
   tuple(add.230, dynamic-update-slice.35)
 }

 ENTRY entry {
   c0 = s32[] constant(-1)
   p0 = bf16[3,8,128] parameter(0)
   tuple = (s32[], bf16[3,8,128]) tuple(c0, p0)
   while = (s32[], bf16[3,8,128]) while(tuple), condition=while_cond,
   body=while_body ROOT gte1 = bf16[3,8,128] get-tuple-element(while),
   index=1
 }
)";
  auto module = ParseAndReturnUnverifiedModule(hlo_string).value();
  auto module2 = ParseAndReturnUnverifiedModule(hlo_string).value();
  EXPECT_TRUE(RunOptimizer(module.get(), /*last_run=*/true, 0).value());
  EXPECT_TRUE(RunOptimizer(module2.get(), /*last_run=*/true, 200).value());
  XLA_VLOG_LINES(1, module->ToString());
  XLA_VLOG_LINES(1, module2->ToString());
  EXPECT_TRUE(RunAndCompareTwoModules(std::move(module), std::move(module2),
                                      ErrorSpec{0.1, 0.1}));
}

TEST_F(CollectivePipelinerExecutionTest,
       TransformNegativeIndexIterationToZero) {
  constexpr absl::string_view hlo_string = R"(
 HloModule module

 add {
   lhs = bf16[] parameter(0)
   rhs = bf16[] parameter(1)
   ROOT add = bf16[] add(lhs, rhs)
 }

 while_cond {
   param = (s32[], bf16[3,8,128]) parameter(0)
   gte = s32[] get-tuple-element(param), index=0
   constant.1 = s32[] constant(0)
   ROOT cmp = pred[] compare(gte, constant.1), direction=LT
 }

 while_body {
   param = (s32[], bf16[3,8,128]) parameter(0)
   get-tuple-element.394 = s32[] get-tuple-element(param), index=0
   get-tuple-element.395 = bf16[3,8,128] get-tuple-element(param), index=1
   constant.2557 = s32[] constant(1)
   add.230 = s32[] add(get-tuple-element.394, constant.2557)
   constant.2559 = s32[] constant(3)
   subtract.139 = s32[] subtract(constant.2559, get-tuple-element.394)
   constant.2560 = s32[] constant(-1)
   add.231 = s32[] add(subtract.139, constant.2560)
   constant.2561 = s32[] constant(0)
   compare.747 = pred[] compare(add.231, constant.2561), direction=LT
   constant.2562 = s32[] constant(2)
   add.232 = s32[] add(subtract.139, constant.2562)
   select.1348 = s32[] select(compare.747, add.232, add.231)
   dynamic-slice.99 = bf16[1,8,128] dynamic-slice(get-tuple-element.395,
   select.1348, constant.2561, constant.2561), dynamic_slice_sizes={1,8,128}
   mul = bf16[1,8,128] multiply(dynamic-slice.99, dynamic-slice.99)
   ar.1 = bf16[1,8,128] negate(mul)
   dynamic-update-slice.35 = bf16[3,8,128]
   dynamic-update-slice(get-tuple-element.395, ar.1, select.1348,
   constant.2561, constant.2561) ROOT tuple = (s32[], bf16[3,8,128])
   tuple(add.230, dynamic-update-slice.35)
 }

 ENTRY entry {
   c0 = s32[] constant(-3)
   p0 = bf16[3,8,128] parameter(0)
   tuple = (s32[], bf16[3,8,128]) tuple(c0, p0)
   while = (s32[], bf16[3,8,128]) while(tuple), condition=while_cond,
   body=while_body ROOT gte1 = bf16[3,8,128] get-tuple-element(while),
   index=1
 }
)";
  auto module = ParseAndReturnUnverifiedModule(hlo_string).value();
  auto module2 = ParseAndReturnUnverifiedModule(hlo_string).value();
  EXPECT_TRUE(RunOptimizer(module.get(), /*last_run=*/true, 0).value());
  EXPECT_TRUE(RunOptimizer(module2.get(), /*last_run=*/true, 200).value());
  XLA_VLOG_LINES(1, module->ToString());
  XLA_VLOG_LINES(1, module2->ToString());
  EXPECT_TRUE(RunAndCompareTwoModules(std::move(module), std::move(module2),
                                      ErrorSpec{0.1, 0.1}));
}

TEST_F(CollectivePipelinerExecutionTest, EscapedInputNoTransform) {
  constexpr absl::string_view hlo_string = R"(
 HloModule module

 add {
   lhs = bf16[] parameter(0)
   rhs = bf16[] parameter(1)
   ROOT add = bf16[] add(lhs, rhs)
 }

 while_cond {
   param = (s32[], bf16[3,8,128], bf16[1,8,128]) parameter(0)
   gte = s32[] get-tuple-element(param), index=0
   constant.1 = s32[] constant(0)
   ROOT cmp = pred[] compare(gte, constant.1), direction=LT
 }

 while_body {
   param = (s32[], bf16[3,8,128], bf16[1,8,128]) parameter(0)
   get-tuple-element.394 = s32[] get-tuple-element(param), index=0
   get-tuple-element.395 = bf16[3,8,128] get-tuple-element(param), index=1
   constant.2557 = s32[] constant(1)
   add.230 = s32[] add(get-tuple-element.394, constant.2557)
   constant.2559 = s32[] constant(3)
   subtract.139 = s32[] subtract(constant.2559, get-tuple-element.394)
   constant.2560 = s32[] constant(-1)
   add.231 = s32[] add(subtract.139, constant.2560)
   constant.2561 = s32[] constant(0)
   compare.747 = pred[] compare(add.231, constant.2561), direction=LT
   constant.2562 = s32[] constant(2)
   add.232 = s32[] add(subtract.139, constant.2562)
   select.1348 = s32[] select(compare.747, add.232, add.231)
   dynamic-slice.911 = bf16[1,8,128] dynamic-slice(get-tuple-element.395,
   constant.2561, constant.2561, constant.2561),
   dynamic_slice_sizes={1,8,128} dynamic-slice.99 = bf16[1,8,128]
   dynamic-slice(get-tuple-element.395, select.1348, constant.2561,
   constant.2561), dynamic_slice_sizes={1,8,128} mul = bf16[1,8,128]
   multiply(dynamic-slice.99, dynamic-slice.99) ar.1 = bf16[1,8,128]
   negate(mul)
   dynamic-update-slice.35 = bf16[3,8,128]
   dynamic-update-slice(get-tuple-element.395, ar.1, select.1348,
   constant.2561, constant.2561) ROOT tuple = (s32[], bf16[3,8,128],
   bf16[1,8,128]) tuple(add.230, dynamic-update-slice.35, dynamic-slice.911)
 }

 ENTRY entry {
   c0 = s32[] constant(-3)
   p0 = bf16[3,8,128] parameter(0)
   cc = bf16[] constant(0)
   c1 = bf16[1,8,128] broadcast(cc), dimensions={}
   tuple = (s32[], bf16[3,8,128], bf16[1,8,128]) tuple(c0, p0, c1)
   while = (s32[], bf16[3,8,128], bf16[1,8,128]) while(tuple),
   condition=while_cond, body=while_body ROOT gte1 = bf16[3,8,128]
   get-tuple-element(while), index=1
 }
)";
  auto module = ParseAndReturnUnverifiedModule(hlo_string).value();
  auto module2 = ParseAndReturnUnverifiedModule(hlo_string).value();
  EXPECT_TRUE(RunOptimizer(module.get(), /*last_run=*/true, 0).value());
  EXPECT_TRUE(RunOptimizer(module2.get(), /*last_run=*/true, 200).value());
  XLA_VLOG_LINES(1, module->ToString());
  XLA_VLOG_LINES(1, module2->ToString());
  EXPECT_TRUE(RunAndCompareTwoModules(std::move(module), std::move(module2),
                                      ErrorSpec{0.1, 0.1}));
}

TEST_F(CollectivePipelinerExecutionTest, TransformWithAg) {
  constexpr absl::string_view hlo_string = R"(
 HloModule module

 add {
   lhs = bf16[] parameter(0)
   rhs = bf16[] parameter(1)
   ROOT add = bf16[] add(lhs, rhs)
 }

 while_cond {
   param = (s32[], bf16[3,8,128]) parameter(0)
   gte = s32[] get-tuple-element(param), index=0
   constant.1 = s32[] constant(0)
   ROOT cmp = pred[] compare(gte, constant.1), direction=LT
 }

 while_body {
   param = (s32[], bf16[3,8,128]) parameter(0)
   get-tuple-element.394 = s32[] get-tuple-element(param), index=0
   get-tuple-element.395 = bf16[3,8,128] get-tuple-element(param), index=1
   constant.2557 = s32[] constant(1)
   add.230 = s32[] add(get-tuple-element.394, constant.2557)
   constant.2559 = s32[] constant(3)
   subtract.139 = s32[] subtract(constant.2559, get-tuple-element.394)
   constant.2560 = s32[] constant(-1)
   add.231 = s32[] add(subtract.139, constant.2560)
   constant.2561 = s32[] constant(0)
   compare.747 = pred[] compare(add.231, constant.2561), direction=LT
   constant.2562 = s32[] constant(2)
   add.232 = s32[] add(subtract.139, constant.2562)
   select.1348 = s32[] select(compare.747, add.232, add.231)
   dynamic-slice.99 = bf16[1,8,128] dynamic-slice(get-tuple-element.395,
   select.1348, constant.2561, constant.2561), dynamic_slice_sizes={1,8,128}
   mul = bf16[1,8,128] multiply(dynamic-slice.99, dynamic-slice.99)
   rs.1 = bf16[1,8,128] negate(mul)
   ag.1 = bf16[1,8,128] negate(rs.1)
   dynamic-update-slice.35 =
   bf16[3,8,128] dynamic-update-slice(get-tuple-element.395, ag.1,
   select.1348, constant.2561, constant.2561) ROOT tuple = (s32[],
   bf16[3,8,128]) tuple(add.230, dynamic-update-slice.35)
 }

 ENTRY entry {
   c0 = s32[] constant(-3)
   p0 = bf16[3,8,128] parameter(0)
   cc = bf16[] constant(0)
   tuple = (s32[], bf16[3,8,128]) tuple(c0, p0)
   while = (s32[], bf16[3,8,128]) while(tuple), condition=while_cond,
   body=while_body ROOT gte1 = bf16[3,8,128] get-tuple-element(while),
   index=1
 }
)";
  auto module = ParseAndReturnUnverifiedModule(hlo_string).value();
  auto module2 = ParseAndReturnUnverifiedModule(hlo_string).value();
  EXPECT_TRUE(RunOptimizer(module.get(), /*last_run=*/true, 0).value());
  EXPECT_TRUE(RunOptimizer(module2.get(), /*last_run=*/true, 200).value());
  XLA_VLOG_LINES(1, module->ToString());
  XLA_VLOG_LINES(1, module2->ToString());
  EXPECT_TRUE(RunAndCompareTwoModules(std::move(module), std::move(module2),
                                      ErrorSpec{0.1, 0.1}));
}

TEST_F(CollectivePipelinerExecutionTest, TransformWithAgWithFormatting) {
  constexpr absl::string_view hlo_string = R"(
HloModule module

add {
  lhs = bf16[] parameter(0)
  rhs = bf16[] parameter(1)
  ROOT add = bf16[] add(lhs, rhs)
}

while_cond {
  param = (s32[], bf16[3,9,128]) parameter(0)
  gte = s32[] get-tuple-element(param), index=0
  constant.1 = s32[] constant(0)
  ROOT cmp = pred[] compare(gte, constant.1), direction=LT
}

while_body {
  param = (s32[], bf16[3,9,128]) parameter(0)
  get-tuple-element.394 = s32[] get-tuple-element(param), index=0
  get-tuple-element.395 = bf16[3,9,128] get-tuple-element(param), index=1
  constant.2557 = s32[] constant(1)
  add.230 = s32[] add(get-tuple-element.394, constant.2557)
  constant.2559 = s32[] constant(3)
  subtract.139 = s32[] subtract(constant.2559, get-tuple-element.394)
  constant.2560 = s32[] constant(-1)
  add.231 = s32[] add(subtract.139, constant.2560)
  constant.2561 = s32[] constant(0)
  compare.747 = pred[] compare(add.231, constant.2561), direction=LT
  constant.2562 = s32[] constant(2)
  add.232 = s32[] add(subtract.139, constant.2562)
  select.1348 = s32[] select(compare.747, add.232, add.231)
  dynamic-slice.99 = bf16[1,9,128] dynamic-slice(get-tuple-element.395, select.1348, constant.2561, constant.2561), dynamic_slice_sizes={1,9,128}
  mul = bf16[1,9,128] multiply(dynamic-slice.99, dynamic-slice.99)
  cpd = bf16[] constant(0)
  %pd = bf16[1,16,128] pad(mul, cpd), padding=0_0x0_7x0_0
  rs.1 = bf16[1,16,128] negate(pd)
  ag.1 = bf16[1,16,128] negate(rs.1)
  slc = bf16[1,9,128] slice(ag.1), slice={[0:1], [0:9], [0:128]}
  dynamic-update-slice.35 = bf16[3,9,128] dynamic-update-slice(get-tuple-element.395, slc, select.1348, constant.2561, constant.2561)
  ROOT tuple = (s32[], bf16[3,9,128]) tuple(add.230, dynamic-update-slice.35)
}

ENTRY entry {
  c0 = s32[] constant(-3)
  p0 = bf16[3,9,128] parameter(0)
  cc = bf16[] constant(0)
  tuple = (s32[], bf16[3,9,128]) tuple(c0, p0)
  while = (s32[], bf16[3,9,128]) while(tuple), condition=while_cond, body=while_body
  ROOT gte1 = bf16[3,9,128] get-tuple-element(while), index=1
}
)";
  auto module = ParseAndReturnUnverifiedModule(hlo_string).value();
  auto module2 = ParseAndReturnUnverifiedModule(hlo_string).value();
  EXPECT_TRUE(RunOptimizer(module.get(), /*last_run=*/true, 0).value());
  EXPECT_TRUE(RunOptimizer(module2.get(), /*last_run=*/true, 200).value());
  XLA_VLOG_LINES(1, module->ToString());
  XLA_VLOG_LINES(1, module2->ToString());
  EXPECT_TRUE(RunAndCompareTwoModules(std::move(module), std::move(module2),
                                      ErrorSpec{0.1, 0.1}));
}

TEST_F(CollectivePipelinerExecutionTest, TransformWithAgInsertCustomCall) {
  constexpr absl::string_view hlo_string = R"(
 HloModule module

 add {
   lhs = bf16[] parameter(0)
   rhs = bf16[] parameter(1)
   ROOT add = bf16[] add(lhs, rhs)
 }

 while_cond {
   param = (s32[], bf16[3,8,128]) parameter(0)
   gte = s32[] get-tuple-element(param), index=0
   constant.1 = s32[] constant(0)
   ROOT cmp = pred[] compare(gte, constant.1), direction=LT
 }

 while_body {
   param = (s32[], bf16[3,8,128]) parameter(0)
   get-tuple-element.394 = s32[] get-tuple-element(param), index=0
   get-tuple-element.395 = bf16[3,8,128] get-tuple-element(param), index=1
   constant.2557 = s32[] constant(1)
   constant.2561 = s32[] constant(0)
   add.230 = s32[] add(get-tuple-element.394, constant.2557)
   dynamic-slice.99 = bf16[1,8,128] dynamic-slice(get-tuple-element.395,
   get-tuple-element.394, constant.2561, constant.2561),
   dynamic_slice_sizes={1,8,128} mul = bf16[1,8,128]
   multiply(dynamic-slice.99, dynamic-slice.99) rs.1 = bf16[1,8,128]
   negate(mul)
   ag.1 = bf16[1,8,128] negate(rs.1)
   dynamic-update-slice.35 = bf16[3,8,128]
   dynamic-update-slice(get-tuple-element.395, ag.1, get-tuple-element.394,
   constant.2561, constant.2561) ROOT tuple = (s32[], bf16[3,8,128])
   tuple(add.230, dynamic-update-slice.35)
 }

 ENTRY entry {
   c0 = s32[] constant(-8)
   p0 = bf16[3,8,128] parameter(0)
   cc = bf16[] constant(0)
   tuple = (s32[], bf16[3,8,128]) tuple(c0, p0)
   while = (s32[], bf16[3,8,128]) while(tuple), condition=while_cond,
   body=while_body ROOT gte1 = bf16[3,8,128] get-tuple-element(while),
   index=1
 }
)";
  auto module = ParseAndReturnUnverifiedModule(hlo_string).value();
  auto module2 = ParseAndReturnUnverifiedModule(hlo_string).value();
  EXPECT_TRUE(RunOptimizer(module.get(), /*last_run=*/true, 0).value());
  EXPECT_TRUE(RunOptimizer(module2.get(), /*last_run=*/true, 200).value());
  XLA_VLOG_LINES(1, module->ToString());
  XLA_VLOG_LINES(1, module2->ToString());
  EXPECT_TRUE(RunAndCompareTwoModules(std::move(module), std::move(module2),
                                      ErrorSpec{0.1, 0.1}));
}

TEST_F(CollectivePipelinerExecutionTest,
       TransformIncrementIndexByOneBackwardsPlusForward) {
  constexpr absl::string_view hlo_string = R"(
HloModule module

add {
  lhs = bf16[] parameter(0)
  rhs = bf16[] parameter(1)
  ROOT add = bf16[] add(lhs, rhs)
}

while_cond {
  param = (s32[], bf16[3,8,128], bf16[3,1,2,128]) parameter(0)
  gte = s32[] get-tuple-element(param), index=0
  constant.1 = s32[] constant(3)
  ROOT cmp = pred[] compare(gte, constant.1), direction=LT
}

while_body {
  param = (s32[], bf16[3,8,128], bf16[3,1,2,128]) parameter(0)
  get-tuple-element.394 = s32[] get-tuple-element(param), index=0
  get-tuple-element.395 = bf16[3,8,128] get-tuple-element(param), index=1
  get-tuple-element.k = bf16[3,1,2,128] get-tuple-element(param), index=2
  constant.2561 = s32[] constant(0)
  constant.2557 = s32[] constant(1)
  add.230 = s32[] add(get-tuple-element.394, constant.2557)
  constant.2559 = s32[] constant(3)
  subtract.139 = s32[] subtract(constant.2559, get-tuple-element.394)
  constant.2560 = s32[] constant(-1)
  add.231 = s32[] add(subtract.139, constant.2560)
  compare.747 = pred[] compare(add.231, constant.2561), direction=LT
  constant.2562 = s32[] constant(2)
  add.232 = s32[] add(subtract.139, constant.2562)
  select.1348 = s32[] select(compare.747, add.232, add.231)
  dynamic-slice.k = bf16[1,1,2,128] dynamic-slice(get-tuple-element.k, select.1348, constant.2561, constant.2561, constant.2561), dynamic_slice_sizes={1,1,2,128}
  r = bf16[1,2,128] reshape(dynamic-slice.k)
  a = bf16[1,2,128] add(r, r)
  ag = bf16[1,8,128] concatenate(a, a, a, a), dimensions={1}
  dynamic-slice.99 = bf16[1,8,128] dynamic-slice(get-tuple-element.395, select.1348, constant.2561, constant.2561), dynamic_slice_sizes={1,8,128}
  mul = bf16[1,8,128] multiply(dynamic-slice.99, ag)
  ar.1 = bf16[1,8,128] negate(mul)
  dynamic-update-slice.35 = bf16[3,8,128] dynamic-update-slice(get-tuple-element.395, ar.1, select.1348, constant.2561, constant.2561)
  ROOT tuple = (s32[], bf16[3,8,128], bf16[3,1,2,128]) tuple(add.230, dynamic-update-slice.35, get-tuple-element.k)
}

ENTRY entry {
  c0 = s32[] constant(0)
  p0 = bf16[3,8,128] parameter(0)
  p1 = bf16[3,1,2,128] parameter(1)
  tuple = (s32[], bf16[3,8,128], bf16[3,1,2,128]) tuple(c0, p0, p1)
  while = (s32[], bf16[3,8,128], bf16[3,1,2,128]) while(tuple), condition=while_cond, body=while_body
  ROOT gte1 = bf16[3,8,128] get-tuple-element(while), index=1
}
)";
  auto module = ParseAndReturnUnverifiedModule(hlo_string).value();
  auto module2 = ParseAndReturnUnverifiedModule(hlo_string).value();

  EXPECT_TRUE(RunOptimizer(module.get(), /*last_run=*/true, 0,
                           /*op=*/HloOpcode::kConcatenate,
                           CollectivePipeliner::PipeliningDirection::kBackward)
                  .value());
  EXPECT_TRUE(RunOptimizer(module.get(), /*last_run=*/true, 0).value());
  XLA_VLOG_LINES(1, module->ToString());
  XLA_VLOG_LINES(1, module2->ToString());
  EXPECT_TRUE(RunAndCompareTwoModules(std::move(module), std::move(module2),
                                      ErrorSpec{0.1, 0.1}));
}

}  // namespace
}  // namespace xla
