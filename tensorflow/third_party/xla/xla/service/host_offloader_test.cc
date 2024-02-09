/* Copyright 2024 The OpenXLA Authors.

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

#include "xla/service/host_offloader.h"

#include <cstdint>
#include <stack>
#include <string>

#include <gmock/gmock.h>
#include <gtest/gtest.h>
#include "absl/container/flat_hash_set.h"
#include "absl/status/status.h"
#include "xla/hlo/ir/hlo_computation.h"
#include "xla/hlo/ir/hlo_module.h"
#include "xla/hlo/ir/hlo_opcode.h"
#include "xla/service/host_memory_offload_annotations.h"
#include "xla/service/pattern_matcher.h"
#include "xla/service/pattern_matcher_gmock.h"
#include "xla/shape.h"
#include "xla/shape_util.h"
#include "xla/statusor.h"
#include "xla/tests/hlo_test_base.h"
#include "xla/util.h"
#include "tsl/lib/core/status_test_util.h"
#include "tsl/platform/statusor.h"

namespace m = ::xla::match;

namespace xla {
namespace {

class HostOffloaderTest : public HloTestBase {
 protected:
  static constexpr int64_t kHostMemorySpaceColor{5};

  StatusOr<bool> RunHostOffloader(HloModule* module) {
    TF_EXPECT_OK(verifier().Run(module).status());
    if (module->has_schedule()) {
      return absl::InternalError("Expected a non-scheduled module");
    }

    HostOffloader host_offloader(kHostMemorySpaceColor);
    return host_offloader.Run(module);
  }

  void TestShapeHasMemorySpace(const Shape& shape, int64_t memory_space) {
    ASSERT_TRUE(shape.has_layout());
    EXPECT_EQ(shape.layout().memory_space(), memory_space);
  }

  bool HaveRemainingOffloadAnnotations(const HloModule* module) {
    for (const HloComputation* computation : module->computations()) {
      for (const HloInstruction* instruction : computation->instructions()) {
        if (instruction->IsCustomCall(
                {host_memory_offload_annotations::kMoveToHostCustomCallTarget,
                 host_memory_offload_annotations::
                     kMoveToDeviceCustomCallTarget})) {
          return true;
        }
      }
    }
    return false;
  }
};

TEST_F(HostOffloaderTest, BasicDusDs) {
  const std::string& hlo_string = R"(
HloModule my_module
ENTRY main {
  data_param = f32[1,2048,2048] parameter(0)
  index_param = s32[] parameter(1)
  constant_f32_0 = f32[] constant(0)
  constant_s32_0 = s32[] constant(0)
  broadcast = f32[2,2048,2048] broadcast(constant_f32_0), dimensions={}
  offload_custom_call = f32[1,2048,2048] custom-call(data_param), custom_call_target="PipelineForward"
  dynamic_update_slice = f32[2,2048,2048] dynamic-update-slice(broadcast, offload_custom_call, index_param, constant_s32_0, constant_s32_0)
  dynamic_slice = f32[1,2048,2048] dynamic-slice(dynamic_update_slice, index_param, constant_s32_0, constant_s32_0), dynamic_slice_sizes={1,2048,2048}
  ROOT load_custom_call = f32[1,2048,2048] custom-call(dynamic_slice), custom_call_target="PipelineBackward"
}
)";

  TF_ASSERT_OK_AND_ASSIGN(auto module,
                          ParseAndReturnVerifiedModule(hlo_string));

  TF_ASSERT_OK_AND_ASSIGN(bool changed, RunHostOffloader(module.get()));

  EXPECT_TRUE(changed);

  // Look for the following pattern:
  // "AllocateBuffer"  param_0  _...
  //               |  /        /
  //           dynamic-update-slice  _...
  //                          |     /
  //                       dynamic-slice
  HloInstruction* param;
  HloInstruction* allocate_buffer;
  HloInstruction* dynamic_update_slice;
  HloInstruction* dynamic_slice;
  ASSERT_THAT(module->entry_computation()->root_instruction(),
              GmockMatch(m::DynamicSlice(
                  &dynamic_slice,
                  m::DynamicUpdateSlice(
                      &dynamic_update_slice,
                      m::CustomCall(&allocate_buffer, {"AllocateBuffer"}),
                      m::Parameter(&param, 0), m::Op(), m::Op(), m::Op()),
                  m::Op(), m::Op(), m::Op())));
  TestShapeHasMemorySpace(param->shape(), Layout::kDefaultMemorySpace);
  TestShapeHasMemorySpace(allocate_buffer->shape(), kHostMemorySpaceColor);
  TestShapeHasMemorySpace(dynamic_update_slice->shape(), kHostMemorySpaceColor);
  TestShapeHasMemorySpace(dynamic_slice->shape(), Layout::kDefaultMemorySpace);

  EXPECT_FALSE(HaveRemainingOffloadAnnotations(module.get()));
}

TEST_F(HostOffloaderTest, BasicCopy) {
  const std::string& hlo_string = R"(
HloModule my_module
ENTRY main {
  data_param = f32[2048] parameter(0)
  offload_custom_call = f32[2048] custom-call(data_param), custom_call_target="PipelineForward"
  copy_0 = f32[2048] copy(offload_custom_call)
  copy_1 = f32[2048] copy(copy_0)
  ROOT load_custom_call = f32[2048] custom-call(copy_1), custom_call_target="PipelineBackward"
}
)";

  TF_ASSERT_OK_AND_ASSIGN(auto module,
                          ParseAndReturnVerifiedModule(hlo_string));

  TF_ASSERT_OK_AND_ASSIGN(bool changed, RunHostOffloader(module.get()));

  EXPECT_TRUE(changed);

  // Look for the following pattern:
  // param
  //   |
  // copy (to host)
  //   |
  // copy (to device)

  HloInstruction* param;
  HloInstruction* copy_to_host;
  HloInstruction* copy_to_device;
  ASSERT_THAT(
      module->entry_computation()->root_instruction(),
      GmockMatch(m::Copy(&copy_to_device,
                         m::Copy(&copy_to_host, m::Parameter(&param, 0)))));
  TestShapeHasMemorySpace(param->shape(), Layout::kDefaultMemorySpace);
  TestShapeHasMemorySpace(copy_to_host->shape(), kHostMemorySpaceColor);
  TestShapeHasMemorySpace(copy_to_device->shape(), Layout::kDefaultMemorySpace);

  EXPECT_FALSE(HaveRemainingOffloadAnnotations(module.get()));
}

TEST_F(HostOffloaderTest, BasicNoCopy) {
  const std::string& hlo_string = R"(
HloModule my_module
ENTRY main {
  data_param = f32[2048] parameter(0)
  offload_custom_call = f32[2048] custom-call(data_param), custom_call_target="PipelineForward"
  ROOT load_custom_call = f32[2048] custom-call(offload_custom_call), custom_call_target="PipelineBackward"
}
)";

  TF_ASSERT_OK_AND_ASSIGN(auto module,
                          ParseAndReturnVerifiedModule(hlo_string));

  TF_ASSERT_OK_AND_ASSIGN(bool changed, RunHostOffloader(module.get()));

  EXPECT_TRUE(changed);

  // Look for the following pattern:
  // param
  //   |
  // copy (to host)
  //   |
  // copy (to device)

  HloInstruction* param;
  HloInstruction* copy_to_host;
  HloInstruction* copy_to_device;
  ASSERT_THAT(
      module->entry_computation()->root_instruction(),
      GmockMatch(m::Copy(&copy_to_device,
                         m::Copy(&copy_to_host, m::Parameter(&param, 0)))));
  TestShapeHasMemorySpace(param->shape(), Layout::kDefaultMemorySpace);
  TestShapeHasMemorySpace(copy_to_host->shape(), kHostMemorySpaceColor);
  TestShapeHasMemorySpace(copy_to_device->shape(), Layout::kDefaultMemorySpace);

  EXPECT_FALSE(HaveRemainingOffloadAnnotations(module.get()));
}

TEST_F(HostOffloaderTest, NoCopyWithOptBarrier) {
  const std::string& hlo_string = R"(
HloModule my_module
ENTRY main {
  data_param = f32[2048] parameter(0)
  offload_custom_call = f32[2048] custom-call(data_param), custom_call_target="PipelineForward"
  tuple = (f32[2048]) tuple(offload_custom_call)
  opt_barrier = (f32[2048]) opt-barrier(tuple)
  get_tuple_element = f32[2048] get-tuple-element(opt_barrier), index=0
  ROOT load_custom_call = f32[2048] custom-call(get_tuple_element), custom_call_target="PipelineBackward"
}
)";

  TF_ASSERT_OK_AND_ASSIGN(auto module,
                          ParseAndReturnVerifiedModule(hlo_string));

  TF_ASSERT_OK_AND_ASSIGN(bool changed, RunHostOffloader(module.get()));

  EXPECT_TRUE(changed);

  // Look for the following pattern:
  // param
  //   |
  // copy (to host)
  //   |
  // tuple
  //   |
  // opt-barrier
  //   |
  // get-tuple-element
  //   |
  // copy (to device)

  HloInstruction* param;
  HloInstruction* copy_to_host;
  HloInstruction* tuple;
  HloInstruction* opt_barrier;
  HloInstruction* gte;
  HloInstruction* copy_to_device;
  ASSERT_THAT(
      module->entry_computation()->root_instruction(),
      GmockMatch(m::Copy(
          &copy_to_device,
          m::GetTupleElement(
              &gte, m::OptimizationBarrier(
                        &opt_barrier,
                        m::Tuple(&tuple, m::Copy(&copy_to_host,
                                                 m::Parameter(&param, 0))))))));
  TestShapeHasMemorySpace(param->shape(), Layout::kDefaultMemorySpace);
  TestShapeHasMemorySpace(copy_to_host->shape(), kHostMemorySpaceColor);
  TestShapeHasMemorySpace(ShapeUtil::GetSubshape(tuple->shape(), {0}),
                          kHostMemorySpaceColor);
  TestShapeHasMemorySpace(ShapeUtil::GetSubshape(opt_barrier->shape(), {0}),
                          kHostMemorySpaceColor);
  TestShapeHasMemorySpace(gte->shape(), kHostMemorySpaceColor);
  TestShapeHasMemorySpace(copy_to_device->shape(), Layout::kDefaultMemorySpace);

  EXPECT_FALSE(HaveRemainingOffloadAnnotations(module.get()));
}

TEST_F(HostOffloaderTest, NoCopyWithOptBarrierMoreElaborate) {
  const std::string& hlo_string = R"(
HloModule jit_f, entry_computation_layout={(f32[16]{0})->f32[16]{0}}

ENTRY main.24 {
  Arg_0.1 = f32[16]{0} parameter(0), sharding={devices=[2]<=[2]}
  cosine.4 = f32[16]{0} cosine(Arg_0.1)
  custom-call.5 = f32[16]{0} custom-call(cosine.4), custom_call_target="PipelineForward"
  sine.3 = f32[16]{0} sine(Arg_0.1)
  cosine.7 = f32[16]{0} cosine(sine.3)
  custom-call.8 = f32[16]{0} custom-call(cosine.7), custom_call_target="PipelineForward"
  sine.6 = f32[16]{0} sine(sine.3)
  cosine.9 = f32[16]{0} cosine(sine.6)
  custom-call.10 = f32[16]{0} custom-call(cosine.9), custom_call_target="PipelineForward"
  constant.2 = f32[] constant(1)
  tuple.11 = (f32[16]{0}, f32[16]{0}, f32[16]{0}, f32[]) tuple(custom-call.5, custom-call.8, custom-call.10, constant.2)
  opt-barrier.12 = (f32[16]{0}, f32[16]{0}, f32[16]{0}, f32[]) opt-barrier(tuple.11)
  get-tuple-element.16 = f32[] get-tuple-element(opt-barrier.12), index=3
  broadcast.20 = f32[16]{0} broadcast(get-tuple-element.16), dimensions={}
  get-tuple-element.15 = f32[16]{0} get-tuple-element(opt-barrier.12), index=2
  custom-call.19 = f32[16]{0} custom-call(get-tuple-element.15), custom_call_target="PipelineBackward"
  multiply.21 = f32[16]{0} multiply(broadcast.20, custom-call.19)
  get-tuple-element.14 = f32[16]{0} get-tuple-element(opt-barrier.12), index=1
  custom-call.18 = f32[16]{0} custom-call(get-tuple-element.14), custom_call_target="PipelineBackward"
  multiply.22 = f32[16]{0} multiply(multiply.21, custom-call.18)
  get-tuple-element.13 = f32[16]{0} get-tuple-element(opt-barrier.12), index=0
  custom-call.17 = f32[16]{0} custom-call(get-tuple-element.13), custom_call_target="PipelineBackward"
  ROOT multiply.23 = f32[16]{0} multiply(multiply.22, custom-call.17)
}
)";

  TF_ASSERT_OK_AND_ASSIGN(auto module,
                          ParseAndReturnVerifiedModule(hlo_string));

  TF_ASSERT_OK_AND_ASSIGN(bool changed, RunHostOffloader(module.get()));

  EXPECT_TRUE(changed);

  // Look for the following pattern:
  //                          param                         constant
  //                __________/ |                             |
  //               /            |                             |
  //          cosine           sine                           |
  //            |               |  \____________              |
  //            |               |               \             |
  //            |               |              sine           |
  //            |               |                |            |
  //            |             cosine          cosine          |
  //            |               |               |             |
  //       copy(to host)   copy(to host)   copy(to host)      |
  //                  \                \   /                  |
  //                   \______________  | |  _________________/
  //                                  \ | | /
  //                                   tuple
  //                                     |
  //                                 opt-barrier
  //                   _____________/   /  \   \_____________
  //                  /                /    \                \
  // get-tuple-element  get-tuple-element  get-tuple-element  get-tuple-element
  //        |                   |                  |                  |
  //   copy(to device)     copy(to device)    copy(to device)     broadcast
  //                  \                   \                 \    /
  //                   \                   \__________     multiply
  //                    \                             \       /
  //                     \                             multiply
  //                      \_________________________        /
  //                                                \      /
  //                                                multiply

  HloInstruction* param;
  HloInstruction* constant;
  HloInstruction* sine_0;
  HloInstruction* sine_1;
  HloInstruction* cosine_0;
  HloInstruction* cosine_1;
  HloInstruction* cosine_2;
  HloInstruction* copy_to_host_0;
  HloInstruction* copy_to_host_1;
  HloInstruction* copy_to_host_2;
  HloInstruction* tuple;
  HloInstruction* opt_barrier;
  HloInstruction* gte_0;
  HloInstruction* gte_1;
  HloInstruction* gte_2;
  HloInstruction* gte_3;
  HloInstruction* broadcast;
  HloInstruction* copy_to_device_0;
  HloInstruction* copy_to_device_1;
  HloInstruction* copy_to_device_2;
  HloInstruction* multiply_0;
  HloInstruction* multiply_1;
  HloInstruction* multiply_2;

  auto parameter_matcher = m::Parameter(&param, 0);
  auto first_sine_matcher = m::Op(&sine_0)
                                .WithOpcode(xla::HloOpcode::kSin)
                                .WithOperand(0, parameter_matcher);
  auto opt_barrier_matcher = m::OptimizationBarrier(
      &opt_barrier,
      m::Tuple(
          &tuple,
          m::Copy(&copy_to_host_0, m::Op(&cosine_0)
                                       .WithOpcode(xla::HloOpcode::kCos)
                                       .WithOperand(0, parameter_matcher)),
          m::Copy(&copy_to_host_1, m::Op(&cosine_1)
                                       .WithOpcode(xla::HloOpcode::kCos)
                                       .WithOperand(0, first_sine_matcher)),
          m::Copy(&copy_to_host_2,
                  m::Op(&cosine_2)
                      .WithOpcode(xla::HloOpcode::kCos)
                      .WithOperand(0, m::Op(&sine_1)
                                          .WithOpcode(xla::HloOpcode::kSin)
                                          .WithOperand(0, first_sine_matcher))),
          m::Constant(&constant)));
  ASSERT_THAT(
      module->entry_computation()->root_instruction(),
      GmockMatch(m::Multiply(
          &multiply_0,
          m::Multiply(
              &multiply_1,
              m::Multiply(
                  &multiply_2,
                  m::Broadcast(&broadcast, m::GetTupleElement(
                                               &gte_3, opt_barrier_matcher, 3)),
                  m::Copy(&copy_to_device_2,
                          m::GetTupleElement(&gte_2, opt_barrier_matcher, 2))),
              m::Copy(&copy_to_device_1,
                      m::GetTupleElement(&gte_1, opt_barrier_matcher, 1))),
          m::Copy(&copy_to_device_0,
                  m::GetTupleElement(&gte_0, opt_barrier_matcher, 0)))));

  TestShapeHasMemorySpace(param->shape(), Layout::kDefaultMemorySpace);
  TestShapeHasMemorySpace(constant->shape(), Layout::kDefaultMemorySpace);
  TestShapeHasMemorySpace(sine_0->shape(), Layout::kDefaultMemorySpace);
  TestShapeHasMemorySpace(sine_1->shape(), Layout::kDefaultMemorySpace);
  TestShapeHasMemorySpace(cosine_0->shape(), Layout::kDefaultMemorySpace);
  TestShapeHasMemorySpace(cosine_1->shape(), Layout::kDefaultMemorySpace);
  TestShapeHasMemorySpace(cosine_2->shape(), Layout::kDefaultMemorySpace);
  TestShapeHasMemorySpace(copy_to_host_0->shape(), kHostMemorySpaceColor);
  TestShapeHasMemorySpace(copy_to_host_1->shape(), kHostMemorySpaceColor);
  TestShapeHasMemorySpace(copy_to_host_2->shape(), kHostMemorySpaceColor);
  TestShapeHasMemorySpace(ShapeUtil::GetSubshape(tuple->shape(), {0}),
                          kHostMemorySpaceColor);
  TestShapeHasMemorySpace(ShapeUtil::GetSubshape(tuple->shape(), {1}),
                          kHostMemorySpaceColor);
  TestShapeHasMemorySpace(ShapeUtil::GetSubshape(tuple->shape(), {2}),
                          kHostMemorySpaceColor);
  TestShapeHasMemorySpace(ShapeUtil::GetSubshape(tuple->shape(), {3}),
                          Layout::kDefaultMemorySpace);
  TestShapeHasMemorySpace(ShapeUtil::GetSubshape(opt_barrier->shape(), {0}),
                          kHostMemorySpaceColor);
  TestShapeHasMemorySpace(ShapeUtil::GetSubshape(opt_barrier->shape(), {1}),
                          kHostMemorySpaceColor);
  TestShapeHasMemorySpace(ShapeUtil::GetSubshape(opt_barrier->shape(), {2}),
                          kHostMemorySpaceColor);
  TestShapeHasMemorySpace(ShapeUtil::GetSubshape(opt_barrier->shape(), {3}),
                          Layout::kDefaultMemorySpace);
  TestShapeHasMemorySpace(gte_0->shape(), kHostMemorySpaceColor);
  TestShapeHasMemorySpace(gte_1->shape(), kHostMemorySpaceColor);
  TestShapeHasMemorySpace(gte_2->shape(), kHostMemorySpaceColor);
  TestShapeHasMemorySpace(gte_3->shape(), Layout::kDefaultMemorySpace);
  TestShapeHasMemorySpace(broadcast->shape(), Layout::kDefaultMemorySpace);
  TestShapeHasMemorySpace(copy_to_device_0->shape(),
                          Layout::kDefaultMemorySpace);
  TestShapeHasMemorySpace(copy_to_device_1->shape(),
                          Layout::kDefaultMemorySpace);
  TestShapeHasMemorySpace(copy_to_device_2->shape(),
                          Layout::kDefaultMemorySpace);
  TestShapeHasMemorySpace(multiply_0->shape(), Layout::kDefaultMemorySpace);
  TestShapeHasMemorySpace(multiply_1->shape(), Layout::kDefaultMemorySpace);
  TestShapeHasMemorySpace(multiply_2->shape(), Layout::kDefaultMemorySpace);

  EXPECT_FALSE(HaveRemainingOffloadAnnotations(module.get()));
}

TEST_F(HostOffloaderTest, BasicDusDsWithMultipleBroadcastUsers) {
  const std::string& hlo_string = R"(
HloModule my_module
ENTRY main {
  data_param = f32[1,2048,2048] parameter(0)
  index_param = s32[] parameter(1)
  constant_f32_0 = f32[] constant(0)
  constant_s32_0 = s32[] constant(0)
  broadcast = f32[2,2048,2048] broadcast(constant_f32_0), dimensions={}
  tanh = f32[2,2048,2048] tanh(broadcast)
  offload_custom_call = f32[1,2048,2048] custom-call(data_param), custom_call_target="PipelineForward"
  dynamic_update_slice = f32[2,2048,2048] dynamic-update-slice(broadcast, offload_custom_call, index_param, constant_s32_0, constant_s32_0)
  dynamic_slice = f32[1,2048,2048] dynamic-slice(dynamic_update_slice, index_param, constant_s32_0, constant_s32_0), dynamic_slice_sizes={1,2048,2048}
  ROOT load_custom_call = f32[1,2048,2048] custom-call(dynamic_slice), custom_call_target="PipelineBackward"
}
)";

  TF_ASSERT_OK_AND_ASSIGN(auto module,
                          ParseAndReturnVerifiedModule(hlo_string));

  TF_ASSERT_OK_AND_ASSIGN(bool changed, RunHostOffloader(module.get()));

  EXPECT_TRUE(changed);

  // Look for the following pattern:
  // "AllocateBuffer"  param_0  _...
  //               |  /        /
  //           dynamic-update-slice  _...
  //                          |     /
  //                       dynamic-slice
  HloInstruction* param;
  HloInstruction* allocate_buffer;
  HloInstruction* dynamic_update_slice;
  HloInstruction* dynamic_slice;
  ASSERT_THAT(module->entry_computation()->root_instruction(),
              GmockMatch(m::DynamicSlice(
                  &dynamic_slice,
                  m::DynamicUpdateSlice(
                      &dynamic_update_slice,
                      m::CustomCall(&allocate_buffer, {"AllocateBuffer"}),
                      m::Parameter(&param, 0), m::Op(), m::Op(), m::Op()),
                  m::Op(), m::Op(), m::Op())));
  TestShapeHasMemorySpace(param->shape(), Layout::kDefaultMemorySpace);
  TestShapeHasMemorySpace(allocate_buffer->shape(), kHostMemorySpaceColor);
  TestShapeHasMemorySpace(dynamic_update_slice->shape(), kHostMemorySpaceColor);
  TestShapeHasMemorySpace(dynamic_slice->shape(), Layout::kDefaultMemorySpace);

  EXPECT_FALSE(HaveRemainingOffloadAnnotations(module.get()));

  // Look for the tanh and make sure that it still uses the original broadcast.
  HloInstruction* tanh = nullptr;
  for (HloInstruction* instruction :
       module->entry_computation()->instructions()) {
    if (instruction->opcode() == HloOpcode::kTanh) {
      tanh = instruction;
      break;
    }
  }
  ASSERT_NE(tanh, nullptr);
  HloInstruction* broadcast;
  EXPECT_THAT(tanh, GmockMatch(m::Tanh(m::Broadcast(&broadcast))));
  TestShapeHasMemorySpace(broadcast->shape(), Layout::kDefaultMemorySpace);
  TestShapeHasMemorySpace(tanh->shape(), Layout::kDefaultMemorySpace);
}

TEST_F(HostOffloaderTest, BasicDusDsBitcastBeforeDus) {
  const std::string& hlo_string = R"(
HloModule my_module
ENTRY main {
  data_param = f32[2048,2048] parameter(0)
  index_param = s32[] parameter(1)
  constant_f32_0 = f32[] constant(0)
  constant_s32_0 = s32[] constant(0)
  broadcast = f32[2,2048,2048] broadcast(constant_f32_0), dimensions={}
  offload_custom_call = f32[2048,2048] custom-call(data_param), custom_call_target="PipelineForward"
  bitcast = f32[1,2048,2048] bitcast(offload_custom_call)
  dynamic_update_slice = f32[2,2048,2048] dynamic-update-slice(broadcast, bitcast, index_param, constant_s32_0, constant_s32_0)
  dynamic_slice = f32[1,2048,2048] dynamic-slice(dynamic_update_slice, index_param, constant_s32_0, constant_s32_0), dynamic_slice_sizes={1,2048,2048}
  ROOT load_custom_call = f32[1,2048,2048] custom-call(dynamic_slice), custom_call_target="PipelineBackward"
}
)";

  TF_ASSERT_OK_AND_ASSIGN(auto module,
                          ParseAndReturnVerifiedModule(hlo_string));

  TF_ASSERT_OK_AND_ASSIGN(bool changed, RunHostOffloader(module.get()));

  EXPECT_TRUE(changed);

  // Look for the following pattern:
  //                   param_0
  //                     |
  // "AllocateBuffer"  bitcast  _...
  //               |  /        /
  //           dynamic-update-slice  _...
  //                          |     /
  //                       dynamic-slice
  HloInstruction* param;
  HloInstruction* bitcast;
  HloInstruction* allocate_buffer;
  HloInstruction* dynamic_update_slice;
  HloInstruction* dynamic_slice;
  ASSERT_THAT(module->entry_computation()->root_instruction(),
              GmockMatch(m::DynamicSlice(
                  &dynamic_slice,
                  m::DynamicUpdateSlice(
                      &dynamic_update_slice,
                      m::CustomCall(&allocate_buffer, {"AllocateBuffer"}),
                      m::Bitcast(&bitcast, m::Parameter(&param, 0)), m::Op(),
                      m::Op(), m::Op()),
                  m::Op(), m::Op(), m::Op())));
  TestShapeHasMemorySpace(param->shape(), Layout::kDefaultMemorySpace);
  TestShapeHasMemorySpace(bitcast->shape(), Layout::kDefaultMemorySpace);
  TestShapeHasMemorySpace(allocate_buffer->shape(), kHostMemorySpaceColor);
  TestShapeHasMemorySpace(dynamic_update_slice->shape(), kHostMemorySpaceColor);
  TestShapeHasMemorySpace(dynamic_slice->shape(), Layout::kDefaultMemorySpace);

  EXPECT_FALSE(HaveRemainingOffloadAnnotations(module.get()));
}

// The annotation is mistakenly after the dynamic-update-slice; it should be
// before.
TEST_F(HostOffloaderTest, BasicDusDsDusAnnotationOnWrongSide) {
  const std::string& hlo_string = R"(
HloModule my_module
ENTRY main {
  data_param = f32[1,2048,2048] parameter(0)
  index_param = s32[] parameter(1)
  constant_f32_0 = f32[] constant(0)
  constant_s32_0 = s32[] constant(0)
  broadcast = f32[2,2048,2048] broadcast(constant_f32_0), dimensions={}
  dynamic_update_slice = f32[2,2048,2048] dynamic-update-slice(broadcast, data_param, index_param, constant_s32_0, constant_s32_0)
  offload_custom_call = f32[1,2048,2048] custom-call(dynamic_update_slice), custom_call_target="PipelineForward"
  dynamic_slice = f32[1,2048,2048] dynamic-slice(offload_custom_call, index_param, constant_s32_0, constant_s32_0), dynamic_slice_sizes={1,2048,2048}
  ROOT load_custom_call = f32[1,2048,2048] custom-call(dynamic_slice), custom_call_target="PipelineBackward"
}
)";

  TF_ASSERT_OK_AND_ASSIGN(auto module,
                          ParseAndReturnVerifiedModule(hlo_string));

  StatusOr<bool> statusOrChanged = RunHostOffloader(module.get());
  // The pass should return an error.
  ASSERT_FALSE(statusOrChanged.ok());
}

// The annotation is mistakenly before the dynamic-slice; it should be after.
TEST_F(HostOffloaderTest, BasicDusDsDsAnnotationOnWrongSide) {
  const std::string& hlo_string = R"(
HloModule my_module
ENTRY main {
  data_param = f32[1,2048,2048] parameter(0)
  index_param = s32[] parameter(1)
  constant_f32_0 = f32[] constant(0)
  constant_s32_0 = s32[] constant(0)
  broadcast = f32[2,2048,2048] broadcast(constant_f32_0), dimensions={}
  offload_custom_call = f32[1,2048,2048] custom-call(data_param), custom_call_target="PipelineForward"
  dynamic_update_slice = f32[2,2048,2048] dynamic-update-slice(broadcast, offload_custom_call, index_param, constant_s32_0, constant_s32_0)
  load_custom_call = f32[2,2048,2048] custom-call(dynamic_update_slice), custom_call_target="PipelineBackward"
  ROOT dynamic_slice = f32[1,2048,2048] dynamic-slice(load_custom_call, index_param, constant_s32_0, constant_s32_0), dynamic_slice_sizes={1,2048,2048}
}
)";

  TF_ASSERT_OK_AND_ASSIGN(auto module,
                          ParseAndReturnVerifiedModule(hlo_string));

  StatusOr<bool> statusOrChanged = RunHostOffloader(module.get());
  // The pass should return an error.
  ASSERT_FALSE(statusOrChanged.ok());
}

TEST_F(HostOffloaderTest, LlmActivation) {
  const std::string& hlo_string = R"(
HloModule llm_while

producing_while_condition {
  producing_condition_param = (s32[], f32[96,8,6,2048,2048], f32[96,8,6,2048,1]) parameter(0)
  producing_condition_current_iteration_index = s32[] get-tuple-element(producing_condition_param), index=0
  producing_condition_iteration_count = s32[] constant(96)
  ROOT producing_condition_result = pred[] compare(producing_condition_current_iteration_index, producing_condition_iteration_count), direction=LT
}

consuming_while_condition {
  consuming_condition_param = (s32[], f32[96,8,6,2048,2048], f32[96,8,6,2048,1]) parameter(0)
  consuming_condition_current_iteration_index = s32[] get-tuple-element(consuming_condition_param), index=0
  consuming_condition_iteration_count = s32[] constant(96)
  ROOT consuming_condition_result = pred[] compare(consuming_condition_current_iteration_index, consuming_condition_iteration_count), direction=LT
}

producing_while_body {
  input_tuple.0 = (s32[], f32[96,8,6,2048,2048], f32[96,8,6,2048,1]) parameter(0)
  current_iteration_index.0 = s32[] get-tuple-element(input_tuple.0), index=0
  data_0.0 = f32[96,8,6,2048,2048] get-tuple-element(input_tuple.0), index=1
  data_1.0 = f32[96,8,6,2048,1] get-tuple-element(input_tuple.0), index=2
  constant_0.0 = s32[] constant(0)
  constant_1.0 = s32[] constant(1)
  constant_96 = s32[] constant(96)

  /* Create dummy data used in DUS */
  slice_data_0 = f32[1,8,6,2048,2048]  constant({...})
  slice_data_1 = f32[1,8,6,2048,1]  constant({...})

  /* Build DUS index */
  compare_result.0 = pred[] compare(current_iteration_index.0, constant_0.0), direction=LT
  add_result = s32[] add(current_iteration_index.0, constant_96)
  select_result.0 = s32[] select(compare_result.0, add_result, current_iteration_index.0)

  /* Annotate DUS for offload */
  custom_call_0.0 = f32[1,8,6,2048,2048] custom-call(slice_data_0), custom_call_target="PipelineForward"
  custom_call_1.0 = f32[1,8,6,2048,1] custom-call(slice_data_1), custom_call_target="PipelineForward"

  dynamic_update_slice_0 = f32[96,8,6,2048,2048] dynamic-update-slice(data_0.0, custom_call_0.0, select_result.0, constant_0.0, constant_0.0, constant_0.0, constant_0.0)
  dynamic_update_slice_1 = f32[96,8,6,2048,1] dynamic-update-slice(data_1.0, custom_call_1.0, select_result.0, constant_0.0, constant_0.0, constant_0.0, constant_0.0)

  /* Increment iteration index */
  incremented_index.0 = s32[] add(current_iteration_index.0, constant_1.0)
  ROOT tuple_result.0 = (s32[], f32[96,8,6,2048,2048], f32[96,8,6,2048,1]) tuple(incremented_index.0, dynamic_update_slice_0, dynamic_update_slice_1)
}

consuming_while_body {
  input_tuple.1 = (s32[], f32[96,8,6,2048,2048], f32[96,8,6,2048,1]) parameter(0)
  current_iteration_index.1 = s32[] get-tuple-element(input_tuple.1), index=0
  data_0.1 = f32[96,8,6,2048,2048] get-tuple-element(input_tuple.1), index=1
  data_1.1 = f32[96,8,6,2048,1] get-tuple-element(input_tuple.1), index=2
  constant_0.1 = s32[] constant(0)
  constant_1.1 = s32[] constant(1)
  constant_95 = s32[] constant(95)
  constant_191 = s32[] constant(191)

  /* Build DS index */
  subtract_0 = s32[] subtract(constant_95, current_iteration_index.1)
  compare_result.1 = pred[] compare(subtract_0, constant_0.1), direction=LT
  subtract_1 = s32[] subtract(constant_191, current_iteration_index.1)
  select_result.1 = s32[] select(compare_result.1, subtract_1, subtract_0)

  dynamic_slice_0 = f32[1,8,6,2048,2048] dynamic-slice(data_0.1, select_result.1, constant_0.1, constant_0.1, constant_0.1, constant_0.1), dynamic_slice_sizes={1,8,6,2048,2048}
  dynamic_slice_1 = f32[1,8,6,2048,1] dynamic-slice(data_1.1, select_result.1, constant_0.1, constant_0.1, constant_0.1, constant_0.1), dynamic_slice_sizes={1,8,6,2048,1}

  /* Annotate DS for offload */
  custom_call_0.1 = f32[1,8,6,2048,2048] custom-call(dynamic_slice_0), custom_call_target="PipelineBackward"
  custom_call_1.1 = f32[1,8,6,2048,1] custom-call(dynamic_slice_1), custom_call_target="PipelineBackward"

  /* Do some work with the dynamic slice outputs. */
  tanh_0 = f32[1,8,6,2048,2048] tanh(custom_call_0.1)
  tanh_1 = f32[1,8,6,2048,1] tanh(custom_call_1.1)

  /* Increment iteration index */
  incremented_index.1 = s32[] add(current_iteration_index.1, constant_1.1)
  ROOT tuple_result.1 = (s32[], f32[96,8,6,2048,2048], f32[96,8,6,2048,1]) tuple(incremented_index.1, data_0.1, data_1.1)
}

ENTRY main {
  moop = f32[] parameter(0)
  broadcast_0 = f32[96,8,6,2048,2048] broadcast(moop), dimensions={}
  broadcast_1 = f32[96,8,6,2048,1] broadcast(moop), dimensions={}
  constant_s32_0 = s32[] constant(0)
  tuple_for_producing_while = (s32[], f32[96,8,6,2048,2048], f32[96,8,6,2048,1]) tuple(constant_s32_0, broadcast_0, broadcast_1)
  producing_while = (s32[], f32[96,8,6,2048,2048], f32[96,8,6,2048,1]) while(tuple_for_producing_while), condition=producing_while_condition, body=producing_while_body
  while_output_1 = f32[96,8,6,2048,2048] get-tuple-element(producing_while), index=1
  while_output_2 = f32[96,8,6,2048,1] get-tuple-element(producing_while), index=2
  tuple_for_consuming_while = (s32[], f32[96,8,6,2048,2048], f32[96,8,6,2048,1]) tuple(constant_s32_0, while_output_1, while_output_2)
  ROOT consuming_while = (s32[], f32[96,8,6,2048,2048], f32[96,8,6,2048,1]) while(tuple_for_consuming_while), condition=consuming_while_condition, body=consuming_while_body
}
)";

  TF_ASSERT_OK_AND_ASSIGN(auto module,
                          ParseAndReturnVerifiedModule(hlo_string));

  TF_ASSERT_OK_AND_ASSIGN(bool changed, RunHostOffloader(module.get()));

  EXPECT_TRUE(changed);

  // First, look for the pattern:
  //  producing_while
  //       /  \
  //     gte  gte  constant
  //       \  /   /
  //        \/   /
  //        tuple
  //         |
  //  consuming_while
  HloInstruction* consuming_while;
  HloInstruction* producing_while_0;
  HloInstruction* producing_while_1;
  {
    HloInstruction* tuple;
    HloInstruction* gte_0;
    HloInstruction* gte_1;
    ASSERT_THAT(
        module->entry_computation()->root_instruction(),
        GmockMatch(m::While(
            &consuming_while,
            m::Tuple(
                &tuple, m::Constant(),
                m::GetTupleElement(&gte_0, m::While(&producing_while_0)),
                m::GetTupleElement(&gte_1, m::While(&producing_while_1))))));
    ASSERT_EQ(producing_while_0, producing_while_1);

    // Check that the memory spaces were properly set.
    TestShapeHasMemorySpace(gte_0->shape(), kHostMemorySpaceColor);
    TestShapeHasMemorySpace(gte_1->shape(), kHostMemorySpaceColor);
    TestShapeHasMemorySpace(
        ShapeUtil::GetSubshape(consuming_while->shape(), {1}),
        kHostMemorySpaceColor);
    TestShapeHasMemorySpace(
        ShapeUtil::GetSubshape(consuming_while->shape(), {2}),
        kHostMemorySpaceColor);
    TestShapeHasMemorySpace(
        ShapeUtil::GetSubshape(producing_while_0->shape(), {1}),
        kHostMemorySpaceColor);
    TestShapeHasMemorySpace(
        ShapeUtil::GetSubshape(producing_while_0->shape(), {2}),
        kHostMemorySpaceColor);
    TestShapeHasMemorySpace(ShapeUtil::GetSubshape(tuple->shape(), {1}),
                            kHostMemorySpaceColor);
    TestShapeHasMemorySpace(ShapeUtil::GetSubshape(tuple->shape(), {2}),
                            kHostMemorySpaceColor);
  }

  // Now, look for the AllocateBuffers leading into the producing while.
  {
    HloInstruction* allocate_buffer_0;
    HloInstruction* allocate_buffer_1;
    ASSERT_THAT(producing_while_0,
                GmockMatch(m::While(m::Tuple(
                    m::Constant(),
                    m::CustomCall(&allocate_buffer_0, {"AllocateBuffer"}),
                    m::CustomCall(&allocate_buffer_1, {"AllocateBuffer"})))));
    // Check that the memory spaces were properly set.
    ASSERT_TRUE(allocate_buffer_0->shape().has_layout());
    EXPECT_EQ(allocate_buffer_0->shape().layout().memory_space(),
              kHostMemorySpaceColor);
    ASSERT_TRUE(allocate_buffer_1->shape().has_layout());
    EXPECT_EQ(allocate_buffer_1->shape().layout().memory_space(),
              kHostMemorySpaceColor);
  }

  // There are 4 computations to look at:
  //  - Consuming while's body
  //  - Consuming while's condition
  //  - Producing while's body
  //  - Producing while's condition

  // For the condition computations, just check that the parameters have the
  // right memory space.
  TestShapeHasMemorySpace(
      ShapeUtil::GetSubshape(
          consuming_while->while_condition()->parameter_instruction(0)->shape(),
          {1}),
      kHostMemorySpaceColor);
  TestShapeHasMemorySpace(
      ShapeUtil::GetSubshape(
          consuming_while->while_condition()->parameter_instruction(0)->shape(),
          {2}),
      kHostMemorySpaceColor);

  // Now, check the producing while for the following pattern:
  //    param      param
  //      |          |
  //     gte  _...  gte  _...
  //     |   /      |   /
  //     |  /       |  /
  //     | /        | /
  //     dus       dus
  //      |       /
  //      |      /
  //  _   |     /
  //   \  |    /
  //    \ |   /
  //     \|  /
  //    tuple
  {
    HloInstruction* tuple;
    HloInstruction* dynamic_update_slice_0;
    HloInstruction* dynamic_update_slice_1;
    HloInstruction* dynamic_update_slice_second_param_0;
    HloInstruction* dynamic_update_slice_second_param_1;
    HloInstruction* gte_0;
    HloInstruction* gte_1;
    HloInstruction* param_0;
    HloInstruction* param_1;
    ASSERT_THAT(producing_while_0->while_body()->root_instruction(),
                GmockMatch(m::Tuple(
                    &tuple, m::Op(),
                    m::DynamicUpdateSlice(
                        &dynamic_update_slice_0,
                        m::GetTupleElement(&gte_0, m::Parameter(&param_0)),
                        m::Op(&dynamic_update_slice_second_param_0), m::Op(),
                        m::Op(), m::Op(), m::Op(), m::Op()),
                    m::DynamicUpdateSlice(
                        &dynamic_update_slice_1,
                        m::GetTupleElement(&gte_1, m::Parameter(&param_1)),
                        m::Op(&dynamic_update_slice_second_param_1), m::Op(),
                        m::Op(), m::Op(), m::Op(), m::Op()))));
    EXPECT_EQ(param_0, param_1);

    // Check that the memory spaces were properly set.
    // HOST:
    //  tuple subshape 1
    //  tuple subshape 2
    //  dynamic_update_slice_0 shape
    //  dynamic_update_slice_1 shape
    //  gte_0 shape
    //  gte_1 shape
    //  param_0 subshape 1
    //  param_0 subshape 2
    // DEVICE:
    //  dynamic_update_slice_second_param_0
    //  dynamic_update_slice_second_param_1

    TestShapeHasMemorySpace(ShapeUtil::GetSubshape(tuple->shape(), {1}),
                            kHostMemorySpaceColor);
    TestShapeHasMemorySpace(ShapeUtil::GetSubshape(tuple->shape(), {2}),
                            kHostMemorySpaceColor);
    TestShapeHasMemorySpace(dynamic_update_slice_0->shape(),
                            kHostMemorySpaceColor);
    TestShapeHasMemorySpace(dynamic_update_slice_1->shape(),
                            kHostMemorySpaceColor);
    TestShapeHasMemorySpace(gte_0->shape(), kHostMemorySpaceColor);
    TestShapeHasMemorySpace(gte_1->shape(), kHostMemorySpaceColor);
    TestShapeHasMemorySpace(ShapeUtil::GetSubshape(param_0->shape(), {1}),
                            kHostMemorySpaceColor);
    TestShapeHasMemorySpace(ShapeUtil::GetSubshape(param_0->shape(), {2}),
                            kHostMemorySpaceColor);
    TestShapeHasMemorySpace(dynamic_update_slice_second_param_0->shape(),
                            Layout::kDefaultMemorySpace);
    TestShapeHasMemorySpace(dynamic_update_slice_second_param_1->shape(),
                            Layout::kDefaultMemorySpace);
  }

  // Now, check the consuming while for the following pattern:
  //  param
  //  |   |
  // gte gte
  //  |   |
  //  ds  ds
  {
    // Since we do not do anything meaningful with the result of the
    // dynamic-slices, there is no easy way to access them from the root.
    // Instead, search from the parameter and find all dynamic-slices.
    EXPECT_EQ(consuming_while->while_body()->parameter_instructions().size(),
              1);
    const HloInstruction* param =
        consuming_while->while_body()->parameter_instruction(0);
    absl::flat_hash_set<const HloInstruction*> dynamic_slices;
    std::stack<const HloInstruction*> stack;
    stack.emplace(param);
    while (!stack.empty()) {
      const HloInstruction* current = stack.top();
      stack.pop();
      if (current->opcode() == HloOpcode::kDynamicSlice) {
        dynamic_slices.emplace(current);
        continue;
      }
      // Add all users.
      for (const HloInstruction* user : current->users()) {
        stack.emplace(user);
      }
    }
    // There should only be two dynamic-slices.
    ASSERT_EQ(dynamic_slices.size(), 2);
    for (const HloInstruction* dynamic_slice : dynamic_slices) {
      const HloInstruction* get_tuple_element;
      const HloInstruction* parameter;
      ASSERT_THAT(
          dynamic_slice,
          GmockMatch(m::DynamicSlice(
              m::GetTupleElement(&get_tuple_element, m::Parameter(&parameter)),
              m::Op(), m::Op(), m::Op(), m::Op(), m::Op())));

      // Check that the memory spaces were properly set.
      // HOST:
      //  parameter subshape 1
      //  parameter subshape 2
      //  get_tuple_element
      // DEVICE:
      //  dynamic_slice
      TestShapeHasMemorySpace(ShapeUtil::GetSubshape(parameter->shape(), {1}),
                              kHostMemorySpaceColor);
      TestShapeHasMemorySpace(ShapeUtil::GetSubshape(parameter->shape(), {2}),
                              kHostMemorySpaceColor);
      TestShapeHasMemorySpace(get_tuple_element->shape(),
                              kHostMemorySpaceColor);
      TestShapeHasMemorySpace(dynamic_slice->shape(),
                              Layout::kDefaultMemorySpace);
    }
  }

  // Finally, ensure that all annotations have been removed.
  EXPECT_FALSE(HaveRemainingOffloadAnnotations(module.get()));
}

TEST_F(HostOffloaderTest, LlmActivationDsWithReshape) {
  const std::string& hlo_string = R"(
HloModule llm_while

producing_while_condition {
  producing_condition_param = (s32[], f32[96,8,6,2048,2048], f32[96,8,6,2048,1]) parameter(0)
  producing_condition_current_iteration_index = s32[] get-tuple-element(producing_condition_param), index=0
  producing_condition_iteration_count = s32[] constant(96)
  ROOT producing_condition_result = pred[] compare(producing_condition_current_iteration_index, producing_condition_iteration_count), direction=LT
}

consuming_while_condition {
  consuming_condition_param = (s32[], f32[96,8,6,2048,2048], f32[96,8,6,2048,1]) parameter(0)
  consuming_condition_current_iteration_index = s32[] get-tuple-element(consuming_condition_param), index=0
  consuming_condition_iteration_count = s32[] constant(96)
  ROOT consuming_condition_result = pred[] compare(consuming_condition_current_iteration_index, consuming_condition_iteration_count), direction=LT
}

producing_while_body {
  input_tuple.0 = (s32[], f32[96,8,6,2048,2048], f32[96,8,6,2048,1]) parameter(0)
  current_iteration_index.0 = s32[] get-tuple-element(input_tuple.0), index=0
  data_0.0 = f32[96,8,6,2048,2048] get-tuple-element(input_tuple.0), index=1
  data_1.0 = f32[96,8,6,2048,1] get-tuple-element(input_tuple.0), index=2
  constant_0.0 = s32[] constant(0)
  constant_1.0 = s32[] constant(1)
  constant_96 = s32[] constant(96)

  /* Create dummy data used in DUS */
  slice_data_0 = f32[1,8,6,2048,2048]  constant({...})
  slice_data_1 = f32[1,8,6,2048,1]  constant({...})

  /* Build DUS index */
  compare_result.0 = pred[] compare(current_iteration_index.0, constant_0.0), direction=LT
  add_result = s32[] add(current_iteration_index.0, constant_96)
  select_result.0 = s32[] select(compare_result.0, add_result, current_iteration_index.0)

  /* Annotate DUS for offload */
  custom_call_0.0 = f32[1,8,6,2048,2048] custom-call(slice_data_0), custom_call_target="PipelineForward"
  custom_call_1.0 = f32[1,8,6,2048,1] custom-call(slice_data_1), custom_call_target="PipelineForward"

  dynamic_update_slice_0 = f32[96,8,6,2048,2048] dynamic-update-slice(data_0.0, custom_call_0.0, select_result.0, constant_0.0, constant_0.0, constant_0.0, constant_0.0)
  dynamic_update_slice_1 = f32[96,8,6,2048,1] dynamic-update-slice(data_1.0, custom_call_1.0, select_result.0, constant_0.0, constant_0.0, constant_0.0, constant_0.0)

  /* Increment iteration index */
  incremented_index.0 = s32[] add(current_iteration_index.0, constant_1.0)
  ROOT tuple_result.0 = (s32[], f32[96,8,6,2048,2048], f32[96,8,6,2048,1]) tuple(incremented_index.0, dynamic_update_slice_0, dynamic_update_slice_1)
}

consuming_while_body {
  input_tuple.1 = (s32[], f32[96,8,6,2048,2048], f32[96,8,6,2048,1]) parameter(0)
  current_iteration_index.1 = s32[] get-tuple-element(input_tuple.1), index=0
  data_0.1 = f32[96,8,6,2048,2048] get-tuple-element(input_tuple.1), index=1
  data_1.1 = f32[96,8,6,2048,1] get-tuple-element(input_tuple.1), index=2
  constant_0.1 = s32[] constant(0)
  constant_1.1 = s32[] constant(1)
  constant_95 = s32[] constant(95)
  constant_191 = s32[] constant(191)

  /* Build DS index */
  subtract_0 = s32[] subtract(constant_95, current_iteration_index.1)
  compare_result.1 = pred[] compare(subtract_0, constant_0.1), direction=LT
  subtract_1 = s32[] subtract(constant_191, current_iteration_index.1)
  select_result.1 = s32[] select(compare_result.1, subtract_1, subtract_0)

  dynamic_slice_0 = f32[1,8,6,2048,2048] dynamic-slice(data_0.1, select_result.1, constant_0.1, constant_0.1, constant_0.1, constant_0.1), dynamic_slice_sizes={1,8,6,2048,2048}
  dynamic_slice_1 = f32[1,8,6,2048,1] dynamic-slice(data_1.1, select_result.1, constant_0.1, constant_0.1, constant_0.1, constant_0.1), dynamic_slice_sizes={1,8,6,2048,1}
  rs = f32[1,8,6,2048,2048] reshape(dynamic_slice_0)
  rs2 = f32[1,8,6,2048,1] reshape(dynamic_slice_1)
  /* Annotate DS for offload */
  custom_call_0.1 = f32[1,8,6,2048,2048] custom-call(rs), custom_call_target="PipelineBackward"
  custom_call_1.1 = f32[1,8,6,2048,1] custom-call(rs2), custom_call_target="PipelineBackward"

  /* Do some work with the dynamic slice outputs. */
  tanh_0 = f32[1,8,6,2048,2048] tanh(custom_call_0.1)
  tanh_1 = f32[1,8,6,2048,1] tanh(custom_call_1.1)

  /* Increment iteration index */
  incremented_index.1 = s32[] add(current_iteration_index.1, constant_1.1)
  ROOT tuple_result.1 = (s32[], f32[96,8,6,2048,2048], f32[96,8,6,2048,1]) tuple(incremented_index.1, data_0.1, data_1.1)
}

ENTRY main {
  moop = f32[] parameter(0)
  broadcast_0 = f32[96,8,6,2048,2048] broadcast(moop), dimensions={}
  broadcast_1 = f32[96,8,6,2048,1] broadcast(moop), dimensions={}
  constant_s32_0 = s32[] constant(0)
  tuple_for_producing_while = (s32[], f32[96,8,6,2048,2048], f32[96,8,6,2048,1]) tuple(constant_s32_0, broadcast_0, broadcast_1)
  producing_while = (s32[], f32[96,8,6,2048,2048], f32[96,8,6,2048,1]) while(tuple_for_producing_while), condition=producing_while_condition, body=producing_while_body
  while_output_1 = f32[96,8,6,2048,2048] get-tuple-element(producing_while), index=1
  while_output_2 = f32[96,8,6,2048,1] get-tuple-element(producing_while), index=2
  tuple_for_consuming_while = (s32[], f32[96,8,6,2048,2048], f32[96,8,6,2048,1]) tuple(constant_s32_0, while_output_1, while_output_2)
  ROOT consuming_while = (s32[], f32[96,8,6,2048,2048], f32[96,8,6,2048,1]) while(tuple_for_consuming_while), condition=consuming_while_condition, body=consuming_while_body
}
)";

  TF_ASSERT_OK_AND_ASSIGN(auto module,
                          ParseAndReturnVerifiedModule(hlo_string));

  TF_ASSERT_OK_AND_ASSIGN(bool changed, RunHostOffloader(module.get()));

  EXPECT_TRUE(changed);

  // First, look for the pattern:
  //  producing_while
  //       /  \
  //     gte  gte  constant
  //       \  /   /
  //        \/   /
  //        tuple
  //         |
  //  consuming_while
  HloInstruction* consuming_while;
  HloInstruction* producing_while_0;
  HloInstruction* producing_while_1;
  {
    HloInstruction* tuple;
    HloInstruction* gte_0;
    HloInstruction* gte_1;
    ASSERT_THAT(
        module->entry_computation()->root_instruction(),
        GmockMatch(m::While(
            &consuming_while,
            m::Tuple(
                &tuple, m::Constant(),
                m::GetTupleElement(&gte_0, m::While(&producing_while_0)),
                m::GetTupleElement(&gte_1, m::While(&producing_while_1))))));
    ASSERT_EQ(producing_while_0, producing_while_1);

    // Check that the memory spaces were properly set.
    TestShapeHasMemorySpace(gte_0->shape(), kHostMemorySpaceColor);
    TestShapeHasMemorySpace(gte_1->shape(), kHostMemorySpaceColor);
    TestShapeHasMemorySpace(
        ShapeUtil::GetSubshape(consuming_while->shape(), {1}),
        kHostMemorySpaceColor);
    TestShapeHasMemorySpace(
        ShapeUtil::GetSubshape(consuming_while->shape(), {2}),
        kHostMemorySpaceColor);
    TestShapeHasMemorySpace(
        ShapeUtil::GetSubshape(producing_while_0->shape(), {1}),
        kHostMemorySpaceColor);
    TestShapeHasMemorySpace(
        ShapeUtil::GetSubshape(producing_while_0->shape(), {2}),
        kHostMemorySpaceColor);
    TestShapeHasMemorySpace(ShapeUtil::GetSubshape(tuple->shape(), {1}),
                            kHostMemorySpaceColor);
    TestShapeHasMemorySpace(ShapeUtil::GetSubshape(tuple->shape(), {2}),
                            kHostMemorySpaceColor);
  }

  // Now, look for the AllocateBuffers leading into the producing while.
  {
    HloInstruction* allocate_buffer_0;
    HloInstruction* allocate_buffer_1;
    ASSERT_THAT(producing_while_0,
                GmockMatch(m::While(m::Tuple(
                    m::Constant(),
                    m::CustomCall(&allocate_buffer_0, {"AllocateBuffer"}),
                    m::CustomCall(&allocate_buffer_1, {"AllocateBuffer"})))));
    // Check that the memory spaces were properly set.
    ASSERT_TRUE(allocate_buffer_0->shape().has_layout());
    EXPECT_EQ(allocate_buffer_0->shape().layout().memory_space(),
              kHostMemorySpaceColor);
    ASSERT_TRUE(allocate_buffer_1->shape().has_layout());
    EXPECT_EQ(allocate_buffer_1->shape().layout().memory_space(),
              kHostMemorySpaceColor);
  }

  // There are 4 computations to look at:
  //  - Consuming while's body
  //  - Consuming while's condition
  //  - Producing while's body
  //  - Producing while's condition

  // For the condition computations, just check that the parameters have the
  // right memory space.
  TestShapeHasMemorySpace(
      ShapeUtil::GetSubshape(
          consuming_while->while_condition()->parameter_instruction(0)->shape(),
          {1}),
      kHostMemorySpaceColor);
  TestShapeHasMemorySpace(
      ShapeUtil::GetSubshape(
          consuming_while->while_condition()->parameter_instruction(0)->shape(),
          {2}),
      kHostMemorySpaceColor);

  // Now, check the producing while for the following pattern:
  //    param      param
  //      |          |
  //     gte  _...  gte  _...
  //     |   /      |   /
  //     |  /       |  /
  //     | /        | /
  //     dus       dus
  //      |       /
  //      |      /
  //  _   |     /
  //   \  |    /
  //    \ |   /
  //     \|  /
  //    tuple
  {
    HloInstruction* tuple;
    HloInstruction* dynamic_update_slice_0;
    HloInstruction* dynamic_update_slice_1;
    HloInstruction* dynamic_update_slice_second_param_0;
    HloInstruction* dynamic_update_slice_second_param_1;
    HloInstruction* gte_0;
    HloInstruction* gte_1;
    HloInstruction* param_0;
    HloInstruction* param_1;
    ASSERT_THAT(producing_while_0->while_body()->root_instruction(),
                GmockMatch(m::Tuple(
                    &tuple, m::Op(),
                    m::DynamicUpdateSlice(
                        &dynamic_update_slice_0,
                        m::GetTupleElement(&gte_0, m::Parameter(&param_0)),
                        m::Op(&dynamic_update_slice_second_param_0), m::Op(),
                        m::Op(), m::Op(), m::Op(), m::Op()),
                    m::DynamicUpdateSlice(
                        &dynamic_update_slice_1,
                        m::GetTupleElement(&gte_1, m::Parameter(&param_1)),
                        m::Op(&dynamic_update_slice_second_param_1), m::Op(),
                        m::Op(), m::Op(), m::Op(), m::Op()))));
    EXPECT_EQ(param_0, param_1);

    // Check that the memory spaces were properly set.
    // HOST:
    //  tuple subshape 1
    //  tuple subshape 2
    //  dynamic_update_slice_0 shape
    //  dynamic_update_slice_1 shape
    //  gte_0 shape
    //  gte_1 shape
    //  param_0 subshape 1
    //  param_0 subshape 2
    // DEVICE:
    //  dynamic_update_slice_second_param_0
    //  dynamic_update_slice_second_param_1

    TestShapeHasMemorySpace(ShapeUtil::GetSubshape(tuple->shape(), {1}),
                            kHostMemorySpaceColor);
    TestShapeHasMemorySpace(ShapeUtil::GetSubshape(tuple->shape(), {2}),
                            kHostMemorySpaceColor);
    TestShapeHasMemorySpace(dynamic_update_slice_0->shape(),
                            kHostMemorySpaceColor);
    TestShapeHasMemorySpace(dynamic_update_slice_1->shape(),
                            kHostMemorySpaceColor);
    TestShapeHasMemorySpace(gte_0->shape(), kHostMemorySpaceColor);
    TestShapeHasMemorySpace(gte_1->shape(), kHostMemorySpaceColor);
    TestShapeHasMemorySpace(ShapeUtil::GetSubshape(param_0->shape(), {1}),
                            kHostMemorySpaceColor);
    TestShapeHasMemorySpace(ShapeUtil::GetSubshape(param_0->shape(), {2}),
                            kHostMemorySpaceColor);
    TestShapeHasMemorySpace(dynamic_update_slice_second_param_0->shape(),
                            Layout::kDefaultMemorySpace);
    TestShapeHasMemorySpace(dynamic_update_slice_second_param_1->shape(),
                            Layout::kDefaultMemorySpace);
  }

  // Now, check the consuming while for the following pattern:
  //  param
  //  |   |
  // gte gte
  //  |   |
  //  ds  ds
  {
    // Since we do not do anything meaningful with the result of the
    // dynamic-slices, there is no easy way to access them from the root.
    // Instead, search from the parameter and find all dynamic-slices.
    EXPECT_EQ(consuming_while->while_body()->parameter_instructions().size(),
              1);
    const HloInstruction* param =
        consuming_while->while_body()->parameter_instruction(0);
    absl::flat_hash_set<const HloInstruction*> dynamic_slices;
    std::stack<const HloInstruction*> stack;
    stack.emplace(param);
    while (!stack.empty()) {
      const HloInstruction* current = stack.top();
      stack.pop();
      if (current->opcode() == HloOpcode::kDynamicSlice) {
        dynamic_slices.emplace(current);
        continue;
      }
      // Add all users.
      for (const HloInstruction* user : current->users()) {
        stack.emplace(user);
      }
    }
    // There should only be two dynamic-slices.
    ASSERT_EQ(dynamic_slices.size(), 2);
    for (const HloInstruction* dynamic_slice : dynamic_slices) {
      const HloInstruction* get_tuple_element;
      const HloInstruction* parameter;
      ASSERT_THAT(
          dynamic_slice,
          GmockMatch(m::DynamicSlice(
              m::GetTupleElement(&get_tuple_element, m::Parameter(&parameter)),
              m::Op(), m::Op(), m::Op(), m::Op(), m::Op())));

      // Check that the memory spaces were properly set.
      // HOST:
      //  parameter subshape 1
      //  parameter subshape 2
      //  get_tuple_element
      // DEVICE:
      //  dynamic_slice
      TestShapeHasMemorySpace(ShapeUtil::GetSubshape(parameter->shape(), {1}),
                              kHostMemorySpaceColor);
      TestShapeHasMemorySpace(ShapeUtil::GetSubshape(parameter->shape(), {2}),
                              kHostMemorySpaceColor);
      TestShapeHasMemorySpace(get_tuple_element->shape(),
                              kHostMemorySpaceColor);
      TestShapeHasMemorySpace(dynamic_slice->shape(),
                              Layout::kDefaultMemorySpace);
    }
  }

  // Finally, ensure that all annotations have been removed.
  EXPECT_FALSE(HaveRemainingOffloadAnnotations(module.get()));
}

}  // namespace

}  // namespace xla
