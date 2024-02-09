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

#include "xla/service/gpu/model/gpu_indexing_performance_model.h"

#include <cstdint>
#include <memory>

#include <gtest/gtest.h>
#include "absl/strings/string_view.h"
#include "absl/time/time.h"
#include "mlir/IR/MLIRContext.h"  // from @llvm-project
#include "xla/hlo/ir/hlo_computation.h"
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/service/gpu/backend_configs.pb.h"
#include "xla/service/gpu/gpu_device_info_for_tests.h"
#include "xla/service/gpu/model/gpu_hlo_cost_analysis.h"
#include "xla/service/gpu/model/gpu_performance_model_base.h"
#include "xla/shape.h"
#include "xla/shape_util.h"
#include "xla/stream_executor/device_description.h"
#include "xla/tests/hlo_test_base.h"
#include "tsl/platform/statusor.h"

namespace xla {
namespace gpu {
namespace {

class GpuIndexingPerformanceModelTest : public HloTestBase {
  GpuHloCostAnalysis::ShapeSizeFunction ShapeSizeBytesFunction() const {
    return [&](const Shape& shape) {
      constexpr int64_t kPointerSize = 8;
      return ShapeUtil::ByteSizeOf(shape, kPointerSize);
    };
  }

 public:
  mlir::MLIRContext mlir_context_;
  // The reference times in the test cases below are measured
  // on A6000 by profiling the execution of the HLOs.
  se::DeviceDescription device_info_{TestGpuDeviceInfo::RTXA6000DeviceInfo()};
  GpuPerformanceModelWithIndexingAnalysis indexing_cost_model_{
      &device_info_, ShapeSizeBytesFunction(), &mlir_context_};

  GpuIndexingPerformanceModelTest() : HloTestBase() {}
};

TEST_F(GpuIndexingPerformanceModelTest, BroadcastElementwise) {
  TF_ASSERT_OK_AND_ASSIGN(auto module, ParseAndReturnVerifiedModule(
                                           R"(
HloModule extracted

ENTRY entry_computation {
  param_0 = f32[32]{0} parameter(0)
  broadcast = f32[32,1,768]{2,1,0} broadcast(param_0), dimensions={0}
  param_1 = f32[32,1,768]{2,1,0} parameter(1)
  ROOT multiply = f32[32,1,768]{2,1,0} multiply(broadcast, param_1)
}
)"));

  auto producer =
      module->entry_computation()->GetInstructionWithName("broadcast");
  auto consumer =
      module->entry_computation()->GetInstructionWithName("multiply");

  auto runtime_data = indexing_cost_model_.EstimateRunTimeForProducerConsumer(
      producer, consumer);
  EXPECT_EQ(runtime_data.flops, 73728);
  EXPECT_EQ(runtime_data.bytes_written, 98304);
  EXPECT_NEAR(absl::ToInt64Nanoseconds(runtime_data.write_time), 128, 2);
  EXPECT_NEAR(absl::ToInt64Nanoseconds(runtime_data.exec_time), 267, 2);
}

}  // namespace
}  // namespace gpu
}  // namespace xla
