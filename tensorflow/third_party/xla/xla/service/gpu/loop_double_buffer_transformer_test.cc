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

#include "xla/service/gpu/loop_double_buffer_transformer.h"

#include <cstdint>
#include <memory>
#include <optional>

#include "absl/container/flat_hash_set.h"
#include "xla/hlo/ir/hlo_computation.h"
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/hlo/ir/hlo_module.h"
#include "xla/hlo/ir/hlo_opcode.h"
#include "xla/service/hlo_dce.h"
#include "xla/service/tuple_simplifier.h"
#include "xla/test.h"
#include "xla/test_helpers.h"
#include "xla/tests/hlo_test_base.h"
#include "xla/xla.pb.h"
#include "xla/xla_data.pb.h"
#include "tsl/platform/statusor.h"

namespace xla {
namespace gpu {
namespace {

int64_t CountInstructions(const HloComputation& computation, HloOpcode opcode) {
  int64_t count = 0;
  for (const auto& instruction : computation.instructions()) {
    if (instruction->opcode() == opcode) {
      count++;
    }
  }
  return count;
}

int64_t CountInstructions(const HloModule& module, HloOpcode opcode) {
  int64_t count = 0;
  for (const auto& computation : module.computations()) {
    count += CountInstructions((*computation), opcode);
  }
  return count;
}

class GpuLoopDoubleBufferTransformerTest : public HloTestBase {
  DebugOptions GetDebugOptionsForTest() override {
    DebugOptions debug_options = HloTestBase::GetDebugOptionsForTest();
    debug_options.set_xla_gpu_enable_while_loop_double_buffering(true);
    return debug_options;
  }
};

TEST_F(GpuLoopDoubleBufferTransformerTest, UnrolledLoopEvenTripCount) {
  const char* const kModuleString = R"(
HloModule all_gather_overlapping
condition {
  input_tuple = (f32[1,128], f32[1,128], f32[2,128], s32[]) parameter(0)
  cond = s32[] get-tuple-element(input_tuple), index=3
  trip_count = s32[] constant(10)
  ROOT done = pred[] compare(cond, trip_count), direction=LT
}

body {
 input_tuple = (f32[1,128], f32[1,128], f32[2,128], s32[]) parameter(0)
 param_0 = f32[1,128] get-tuple-element(input_tuple), index=0
 param_1 = f32[2,128] get-tuple-element(input_tuple), index=2
 cond = s32[] get-tuple-element(input_tuple), index=3
 c0 = f32[] constant(0)
 splat_c0 = f32[1,128] broadcast(c0), dimensions={}
 add = f32[1,128] add(splat_c0, param_0)
 // Start all-gather communication
 all-gather-start = (f32[1,128], f32[2,128]) all-gather-start(add), channel_id=1337, replica_groups={{0,1}}, dimensions={0}, use_global_device_ids=true
 // Intertwined with the all-gather communication, an operation happens which
 // depends on param_1, but crucially has a different output shape (which
 // excludes reusing param_1's buffer for its output).
 c1_s32 = s32[] constant(1)
 c0_s32 = s32[] constant(0)
 one = s32[] constant(1)
 cond_plus_1 = s32[] add(cond, one)
 dynamic-slice = f32[1,128] dynamic-slice(param_1, c1_s32, c0_s32), dynamic_slice_sizes={1,128}
 // The all-gather communication finishes
 all-gather-done = f32[2,128] all-gather-done(all-gather-start)
 ROOT output_tuple = (f32[1,128], f32[1,128], f32[2,128], s32[]) tuple(param_0, dynamic-slice, all-gather-done, cond_plus_1)
}

ENTRY main {
 param_0 = f32[1,128] parameter(0)
 param_1 = f32[2,128] parameter(1)
 param_2 = s32[] constant(0)
 tuple = (f32[1,128], f32[1,128], f32[2,128], s32[]) tuple(param_0, param_0, param_1, param_2)
 ROOT while = (f32[1,128], f32[1,128], f32[2,128], s32[]) while(tuple), condition=condition, body=body, backend_config={"known_trip_count":{"n":"10"}}
})";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<xla::HloModule> module,
                          ParseAndReturnVerifiedModule(kModuleString));
  LoopDoubleBufferTransformer double_buffer;
  HloDCE dce;
  TupleSimplifier tuple_simp;
  ASSERT_IS_OK(double_buffer.Run(module.get()).status());
  ASSERT_IS_OK(tuple_simp.Run(module.get()).status());
  ASSERT_IS_OK(dce.Run(module.get()).status());

  HloInstruction* while_instruction;
  for (auto instr : module->entry_computation()->instructions()) {
    if (instr->opcode() == HloOpcode::kWhile) {
      while_instruction = instr;
    }
  }
  TF_ASSERT_OK_AND_ASSIGN(
      WhileLoopBackendConfig config,
      while_instruction->backend_config<WhileLoopBackendConfig>());
  int64_t exact_trip_count = config.known_trip_count().n();
  // We expect that after unrolling, the total trip count is half of original
  // count.
  EXPECT_EQ(exact_trip_count, 5);
  // We expect that after unrolling, there should be 2 allgather starts,
  // both in while body.
  EXPECT_EQ(CountInstructions((*while_instruction->while_body()),
                              HloOpcode::kAllGatherStart),
            2);
  EXPECT_EQ(CountInstructions((*module), HloOpcode::kAllGatherStart), 2);
}

TEST_F(GpuLoopDoubleBufferTransformerTest, UnrolledLoopOddTripCount) {
  const char* const kModuleString = R"(
HloModule all_gather_overlapping
condition {
  input_tuple = (f32[1,128], f32[1,128], f32[2,128], s32[]) parameter(0)
  cond = s32[] get-tuple-element(input_tuple), index=3
  trip_count = s32[] constant(10)
  ROOT done = pred[] compare(cond, trip_count), direction=LT
}

body {
 input_tuple = (f32[1,128], f32[1,128], f32[2,128], s32[]) parameter(0)
 param_0 = f32[1,128] get-tuple-element(input_tuple), index=0
 param_1 = f32[2,128] get-tuple-element(input_tuple), index=2
 cond = s32[] get-tuple-element(input_tuple), index=3
 c0 = f32[] constant(0)
 splat_c0 = f32[1,128] broadcast(c0), dimensions={}
 add = f32[1,128] add(splat_c0, param_0)
 // Start all-gather communication
 all-gather-start = (f32[1,128], f32[2,128]) all-gather-start(add), channel_id=1337, replica_groups={{0,1}}, dimensions={0}, use_global_device_ids=true
 // Intertwined with the all-gather communication, an operation happens which
 // depends on param_1, but crucially has a different output shape (which
 // excludes reusing param_1's buffer for its output).
 c1_s32 = s32[] constant(1)
 c0_s32 = s32[] constant(0)
 one = s32[] constant(1)
 cond_plus_1 = s32[] add(cond, one)
 dynamic-slice = f32[1,128] dynamic-slice(param_1, c1_s32, c0_s32), dynamic_slice_sizes={1,128}
 // The all-gather communication finishes
 all-gather-done = f32[2,128] all-gather-done(all-gather-start)
 ROOT output_tuple = (f32[1,128], f32[1,128], f32[2,128], s32[]) tuple(param_0, dynamic-slice, all-gather-done, cond_plus_1)
}

ENTRY main {
 param_0 = f32[1,128] parameter(0)
 param_1 = f32[2,128] parameter(1)
 param_2 = s32[] constant(0)
 tuple = (f32[1,128], f32[1,128], f32[2,128], s32[]) tuple(param_0, param_0, param_1, param_2)
 ROOT while = (f32[1,128], f32[1,128], f32[2,128], s32[]) while(tuple), condition=condition, body=body, backend_config={"known_trip_count":{"n":"11"}}
})";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<xla::HloModule> module,
                          ParseAndReturnVerifiedModule(kModuleString));
  LoopDoubleBufferTransformer double_buffer;
  HloDCE dce;
  TupleSimplifier tuple_simp;
  ASSERT_IS_OK(double_buffer.Run(module.get()).status());
  ASSERT_IS_OK(tuple_simp.Run(module.get()).status());
  ASSERT_IS_OK(dce.Run(module.get()).status());

  // We expect that for the while loop, no further copy needs to be added to the
  // module.
  HloInstruction* while_instruction;
  for (auto instr : module->entry_computation()->instructions()) {
    if (instr->opcode() == HloOpcode::kWhile) {
      while_instruction = instr;
    }
  }
  TF_ASSERT_OK_AND_ASSIGN(
      WhileLoopBackendConfig config,
      while_instruction->backend_config<WhileLoopBackendConfig>());
  int64_t exact_trip_count = config.known_trip_count().n();
  // We expect that after unrolling, the total trip count is half of original
  // count.
  EXPECT_EQ(exact_trip_count, 5);

  // We expect that after unrolling, there should be 3 allgather starts,
  // 1 in parent computation, 2 in while body.
  EXPECT_EQ(CountInstructions((*while_instruction->while_body()),
                              HloOpcode::kAllGatherStart),
            2);
  EXPECT_EQ(CountInstructions((*module), HloOpcode::kAllGatherStart), 3);

  // We expect that after unrolling, the third operand of the input tuple should
  // be the peeled allgather done.
  EXPECT_EQ(while_instruction->operand(0)->operand(2)->opcode(),
            HloOpcode::kAllGatherDone);
}

TEST_F(GpuLoopDoubleBufferTransformerTest,
       UnrolledLoopNoControlDepsForConstantAdd) {
  const char* const kModuleString = R"(
HloModule loop_unrolling_no_deps
condition {
  input_tuple = (f32[], s32[]) parameter(0)
  cond = s32[] get-tuple-element(input_tuple), index=1
  trip_count = s32[] constant(10)
  ROOT done = pred[] compare(cond, trip_count), direction=LT
}

body {
 input_tuple = (f32[], s32[]) parameter(0)
 param_0 = f32[] get-tuple-element(input_tuple), index=0
 cond = s32[] get-tuple-element(input_tuple), index=1
 c2 = f32[] constant(2)
 add = f32[] add(c2, param_0)
 one = s32[] constant(1)
 cond_plus_1 = s32[] add(cond, one)
 ROOT output_tuple = (f32[], s32[]) tuple(add, cond_plus_1)
}

ENTRY main {
 param_0 = f32[] parameter(0)
 param_2 = s32[] constant(0)
 tuple = (f32[], s32[]) tuple(param_0, param_2)
 ROOT while = (f32[], s32[]) while(tuple), condition=condition, body=body, backend_config={"known_trip_count":{"n":"11"}}
})";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<xla::HloModule> module,
                          ParseAndReturnVerifiedModule(kModuleString));
  LoopDoubleBufferTransformer double_buffer;
  HloDCE dce;
  TupleSimplifier tuple_simp;
  ASSERT_IS_OK(double_buffer.Run(module.get()).status());
  ASSERT_IS_OK(tuple_simp.Run(module.get()).status());
  ASSERT_IS_OK(dce.Run(module.get()).status());

  HloInstruction* while_instruction;
  for (auto instr : module->entry_computation()->instructions()) {
    if (instr->opcode() == HloOpcode::kWhile) {
      while_instruction = instr;
    }
  }
  TF_ASSERT_OK_AND_ASSIGN(
      WhileLoopBackendConfig config,
      while_instruction->backend_config<WhileLoopBackendConfig>());
  int64_t exact_trip_count = config.known_trip_count().n();
  // We expect that after unrolling, the total trip count is half of original
  // count.
  EXPECT_EQ(exact_trip_count, 5);

  // We expect that after unrolling, there should be 4 adds
  EXPECT_EQ(
      CountInstructions((*while_instruction->while_body()), HloOpcode::kAdd),
      4);

  // We expect that after unrolling, the first operand of the output tuple
  // should not have any control dependency since it's a elementwise add with a
  // constant operand.
  EXPECT_EQ(while_instruction->while_body()
                ->root_instruction()
                ->operand(0)
                ->control_predecessors()
                .size(),
            0);
}

TEST_F(GpuLoopDoubleBufferTransformerTest,
       UnrolledLoopNoControlDepsForCollective) {
  const char* const kModuleString = R"(
HloModule loop_unrolling_no_deps
condition {
  input_tuple = (f32[], s32[]) parameter(0)
  cond = s32[] get-tuple-element(input_tuple), index=1
  trip_count = s32[] constant(10)
  ROOT done = pred[] compare(cond, trip_count), direction=LT
}

ar_add {
  Arg_1 = f32[] parameter(1)
  Arg_0 = f32[] parameter(0)
  ROOT add_ar = f32[] add(Arg_1, Arg_0)
}

body {
 input_tuple = (f32[], s32[]) parameter(0)
 param_0 = f32[] get-tuple-element(input_tuple), index=0
 cond = s32[] get-tuple-element(input_tuple), index=1
 all-reduce-start = f32[] all-reduce-start(param_0), channel_id=8, replica_groups={{0}}, to_apply=ar_add, backend_config="{\"is_sync\":false}"
 one = s32[] constant(1)
 all-reduce-done = f32[] all-reduce-done(all-reduce-start)
 cond_plus_1 = s32[] add(cond, one)
 ROOT output_tuple = (f32[], s32[]) tuple(all-reduce-done, cond_plus_1)
}

ENTRY main {
 param_0 = f32[] parameter(0)
 param_2 = s32[] constant(0)
 tuple = (f32[], s32[]) tuple(param_0, param_2)
 ROOT while = (f32[], s32[]) while(tuple), condition=condition, body=body, backend_config={"known_trip_count":{"n":"10"}}
})";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<xla::HloModule> module,
                          ParseAndReturnVerifiedModule(kModuleString));
  LoopDoubleBufferTransformer double_buffer;
  HloDCE dce;
  TupleSimplifier tuple_simp;
  ASSERT_IS_OK(double_buffer.Run(module.get()).status());
  ASSERT_IS_OK(tuple_simp.Run(module.get()).status());
  ASSERT_IS_OK(dce.Run(module.get()).status());

  HloInstruction* while_instruction;
  for (auto instr : module->entry_computation()->instructions()) {
    if (instr->opcode() == HloOpcode::kWhile) {
      while_instruction = instr;
    }
  }
  TF_ASSERT_OK_AND_ASSIGN(
      WhileLoopBackendConfig config,
      while_instruction->backend_config<WhileLoopBackendConfig>());
  int64_t exact_trip_count = config.known_trip_count().n();
  // We expect that after unrolling, the total trip count is half of original
  // count.
  EXPECT_EQ(exact_trip_count, 5);

  // We expect that after unrolling, there should be 2 all-reduce-starts
  EXPECT_EQ(CountInstructions((*while_instruction->while_body()),
                              HloOpcode::kAllReduceStart),
            2);
  absl::flat_hash_set<int64_t> channel_ids;
  for (HloInstruction* ar : while_instruction->while_body()->instructions()) {
    if (ar->opcode() == HloOpcode::kAllReduceStart) {
      // We expect that after unrolling, allreduces should not have any control
      // deps.
      EXPECT_EQ(ar->control_predecessors().size(), 0);
      channel_ids.insert(*(ar->channel_id()));
    }
  }
  // we expect that all 2 allreduces will have different channel ids.
  EXPECT_EQ(channel_ids.size(), 2);
}

}  // namespace
}  // namespace gpu
}  // namespace xla
