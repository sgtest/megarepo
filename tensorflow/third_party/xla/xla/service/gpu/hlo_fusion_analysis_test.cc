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
#include "xla/service/gpu/hlo_fusion_analysis.h"

#include "xla/service/gpu/backend_configs.pb.h"
#include "xla/service/gpu/gpu_device_info_for_tests.h"
#include "xla/service/gpu/hlo_traversal.h"
#include "xla/stream_executor/device_description.pb.h"
#include "xla/tests/hlo_test_base.h"
#include "tsl/platform/statusor.h"

namespace xla::gpu {
namespace {

class HloFusionAnalysisTest : public HloTestBase {};

TEST_F(HloFusionAnalysisTest, DoesNotPeekOutsideBoundary) {
  auto module = ParseAndReturnVerifiedModule(R"(
    HloModule test_module

    add {
      p0 = f32[] parameter(0)
      p1 = f32[] parameter(1)
      ROOT add = f32[] add(p0, p1)
    }

    ENTRY main {
      %p0 = f32[1024] parameter(0)
      %p1 = f32[] parameter(1)
      %reduce = f32[] reduce(%p0, %p1), dimensions={0}, to_apply=add
      ROOT %bitcast = s32[] bitcast(%reduce)
    })")
                    .value();

  auto device_info = TestGpuDeviceInfo::RTXA6000DeviceInfo();

  auto* root = module->entry_computation()->root_instruction();
  auto analysis = AnalyzeFusion(*root, device_info);
  ASSERT_NE(analysis, std::nullopt);
  EXPECT_EQ(analysis->GetEmitterFusionKind(),
            HloFusionAnalysis::EmitterFusionKind::kLoop);

  auto analysis_fused =
      AnalyzeProducerConsumerFusion(*root->operand(0), *root, device_info);
  ASSERT_NE(analysis_fused, std::nullopt);
  EXPECT_EQ(analysis_fused->GetEmitterFusionKind(),
            HloFusionAnalysis::EmitterFusionKind::kReduction);
}

TEST_F(HloFusionAnalysisTest, ReductionWithMultipleUsers) {
  auto module = ParseAndReturnVerifiedModule(R"(
    HloModule test_module

    add {
      p0 = f32[] parameter(0)
      p1 = f32[] parameter(1)
      ROOT add = f32[] add(p0, p1)
    }

    fused_computation {
      %p0 = f32[1024] parameter(0)
      %p1 = f32[] parameter(1)
      %reduce = f32[] reduce(%p0, %p1), dimensions={0}, to_apply=add
      %negate = f32[] negate(%reduce)
      %log = f32[] log(%reduce)
      ROOT %tuple = (f32[], f32[]) tuple(%negate, %log)
    }

    ENTRY main {
      %p0 = f32[1024] parameter(0)
      %p1 = f32[] parameter(1)
      ROOT %fusion = (f32[], f32[]) fusion(%p0, %p1), kind=kLoop, calls=fused_computation
    })")
                    .value();

  auto device_info = TestGpuDeviceInfo::RTXA6000DeviceInfo();

  TF_ASSERT_OK_AND_ASSIGN(
      auto analysis, HloFusionAnalysis::Create(
                         FusionBackendConfig::default_instance(),
                         HloFusionAdaptor::ForInstruction(
                             module->entry_computation()->root_instruction()),
                         &device_info));
  // This fusion cannot use the reduction emitter because the reduce has two
  // users.
  EXPECT_EQ(analysis.GetEmitterFusionKind(),
            HloFusionAnalysis::EmitterFusionKind::kLoop);
}

TEST_F(HloFusionAnalysisTest, ReductionEpilogueFusion) {
  auto module = ParseAndReturnVerifiedModule(R"(
    HloModule test_module

    add {
      p0 = f32[] parameter(0)
      p1 = f32[] parameter(1)
      ROOT add = f32[] add(p0, p1)
    }

    fused_computation {
      %p0 = f32[1024] parameter(0)
      %p1 = f32[] parameter(1)
      %reduce = f32[] reduce(%p0, %p1), dimensions={0}, to_apply=add
      ROOT %negate = f32[] negate(%reduce)
    }

    ENTRY main {
      %p0 = f32[1024] parameter(0)
      %p1 = f32[] parameter(1)
      ROOT %fusion = f32[] fusion(%p0, %p1), kind=kInput, calls=fused_computation
    })")
                    .value();

  auto device_info = TestGpuDeviceInfo::RTXA6000DeviceInfo();

  auto* root = module->entry_computation()->root_instruction();
  TF_ASSERT_OK_AND_ASSIGN(
      auto analysis, HloFusionAnalysis::Create(
                         FusionBackendConfig::default_instance(),
                         HloFusionAdaptor::ForInstruction(root), &device_info));
  EXPECT_EQ(analysis.GetEmitterFusionKind(),
            HloFusionAnalysis::EmitterFusionKind::kReduction);
}

TEST_F(HloFusionAnalysisTest, ReductionEpilogueFusionPartiallyFused) {
  auto module = ParseAndReturnVerifiedModule(R"(
    HloModule test_module

    add {
      p0 = f32[] parameter(0)
      p1 = f32[] parameter(1)
      ROOT add = f32[] add(p0, p1)
    }

    fusion {
      %p0 = f32[1024] parameter(0)
      %p1 = f32[] parameter(1)
      ROOT %reduce = f32[] reduce(%p0, %p1), dimensions={0}, to_apply=add
    }

    ENTRY main {
      %p0 = f32[1024] parameter(0)
      %p1 = f32[] parameter(1)
      %fusion = f32[] fusion(%p0, %p1), kind=kInput, calls=fusion
      ROOT %negate = f32[] negate(%fusion)
    })")
                    .value();

  auto device_info = TestGpuDeviceInfo::RTXA6000DeviceInfo();

  auto* root = module->entry_computation()->root_instruction();

  auto analysis =
      AnalyzeProducerConsumerFusion(*root->operand(0), *root, device_info);
  ASSERT_NE(analysis, std::nullopt);
  EXPECT_EQ(analysis->GetEmitterFusionKind(),
            HloFusionAnalysis::EmitterFusionKind::kReduction);
}

TEST_F(HloFusionAnalysisTest, ReductionEpilogueFusionPartiallyFusedInConsumer) {
  auto module = ParseAndReturnVerifiedModule(R"(
    HloModule test_module

    add {
      p0 = f32[] parameter(0)
      p1 = f32[] parameter(1)
      ROOT add = f32[] add(p0, p1)
    }

    fusion {
      %p0 = f32[] parameter(0)
      ROOT %negate = f32[] negate(%p0)
    }

    ENTRY main {
      %p0 = f32[1024] parameter(0)
      %p1 = f32[] parameter(1)
      %reduce = f32[] reduce(%p0, %p1), dimensions={0}, to_apply=add
      ROOT %fusion = f32[] fusion(%reduce), kind=kInput, calls=fusion
    })")
                    .value();

  auto device_info = TestGpuDeviceInfo::RTXA6000DeviceInfo();

  auto* root = module->entry_computation()->root_instruction();
  auto analysis =
      AnalyzeProducerConsumerFusion(*root->operand(0), *root, device_info);
  ASSERT_NE(analysis, std::nullopt);
  EXPECT_EQ(analysis->GetEmitterFusionKind(),
            HloFusionAnalysis::EmitterFusionKind::kReduction);
}

TEST_F(HloFusionAnalysisTest, ReductionEpilogueFusionPartiallyFusedInBoth) {
  auto module = ParseAndReturnVerifiedModule(R"(
    HloModule test_module

    add {
      p0 = f32[] parameter(0)
      p1 = f32[] parameter(1)
      ROOT add = f32[] add(p0, p1)
    }

    fusion.1 {
      %p0 = f32[1024] parameter(0)
      %p1 = f32[] parameter(1)
      ROOT %reduce = f32[] reduce(%p0, %p1), dimensions={0}, to_apply=add
    }

    fusion.2 {
      %p0 = f32[] parameter(0)
      ROOT %negate = f32[] negate(%p0)
    }

    ENTRY main {
      %p0 = f32[1024] parameter(0)
      %p1 = f32[] parameter(1)
      %fusion.1 = f32[] fusion(%p0, %p1), kind=kInput, calls=fusion.1
      ROOT %fusion.2 = f32[] fusion(%fusion.1), kind=kInput, calls=fusion.2
    })")
                    .value();

  auto device_info = TestGpuDeviceInfo::RTXA6000DeviceInfo();

  auto* root = module->entry_computation()->root_instruction();
  auto analysis =
      AnalyzeProducerConsumerFusion(*root->operand(0), *root, device_info);
  ASSERT_NE(analysis, std::nullopt);
  EXPECT_EQ(analysis->GetEmitterFusionKind(),
            HloFusionAnalysis::EmitterFusionKind::kReduction);
}

TEST_F(HloFusionAnalysisTest, InvalidDevice) {
  // Verifies that an analysis can be created even with an invalid/empty device
  // info, and that the emitter type is determined correctly.
  // Don't rely on this behavior.
  auto module = ParseAndReturnVerifiedModule(R"(
    HloModule test_module

    add {
      p0 = f32[] parameter(0)
      p1 = f32[] parameter(1)
      ROOT add = f32[] add(p0, p1)
    }

    ENTRY main {
      %p0 = f32[1024,128] parameter(0)
      %p1 = f32[] parameter(1)
      %reduce = f32[128] reduce(%p0, %p1), dimensions={0}, to_apply=add
      ROOT %bitcast = s32[128] bitcast(%reduce)
    })")
                    .value();

  stream_executor::GpuDeviceInfoProto device_info_proto;
  stream_executor::DeviceDescription device_info(device_info_proto);

  auto* root = module->entry_computation()->root_instruction();
  auto analysis_fused =
      AnalyzeProducerConsumerFusion(*root->operand(0), *root, device_info);
  ASSERT_NE(analysis_fused, std::nullopt);
  EXPECT_EQ(analysis_fused->GetEmitterFusionKind(),
            HloFusionAnalysis::EmitterFusionKind::kReduction);
}

}  // namespace
}  // namespace xla::gpu
