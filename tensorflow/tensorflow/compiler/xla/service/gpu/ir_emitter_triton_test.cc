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

#include "tensorflow/compiler/xla/service/gpu/ir_emitter_triton.h"

#include <memory>
#include <string>
#include <utility>
#include <vector>

#include "llvm/IR/LLVMContext.h"
#include "mlir/IR/MLIRContext.h"  // from @llvm-project
#include "tensorflow/compiler/xla/autotuning.pb.h"
#include "tensorflow/compiler/xla/error_spec.h"
#include "tensorflow/compiler/xla/hlo/ir/hlo_computation.h"
#include "tensorflow/compiler/xla/hlo/ir/hlo_instruction.h"
#include "tensorflow/compiler/xla/service/gpu/backend_configs.pb.h"
#include "tensorflow/compiler/xla/service/gpu/gpu_device_info_for_tests.h"
#include "tensorflow/compiler/xla/service/gpu/ir_emission_utils.h"
#include "tensorflow/compiler/xla/service/gpu/launch_dimensions.h"
#include "tensorflow/compiler/xla/service/gpu/tests/gpu_codegen_test.h"
#include "tensorflow/compiler/xla/service/pattern_matcher.h"
#include "tensorflow/compiler/xla/service/pattern_matcher_gmock.h"
#include "tensorflow/compiler/xla/stream_executor/device_description.h"
#include "tensorflow/compiler/xla/tests/verified_hlo_module.h"
#include "tensorflow/tsl/lib/core/status_test_util.h"
#include "tensorflow/tsl/platform/path.h"
#include "tensorflow/tsl/platform/status_matchers.h"
#include "tensorflow/tsl/platform/statusor.h"
#include "tensorflow/tsl/platform/tensor_float_32_utils.h"

namespace xla {
namespace gpu {
namespace {

namespace m = ::xla::match;

class TritonGemmNoTF32Test : public GpuCodegenTest {
 public:
  void SetUp() override {
    tf32_state_ = tsl::tensor_float_32_execution_enabled();
    tsl::enable_tensor_float_32_execution(false);
  }
  void TearDown() override {
    tsl::enable_tensor_float_32_execution(tf32_state_);
  }

 private:
  bool tf32_state_;
};

TEST_F(TritonGemmNoTF32Test, DoNotUseTensorCoresForF32) {
  const std::string kHloText = R"(
HloModule t, is_scheduled=true

triton_gemm_r {
  parameter_0 = s8[80,15]{1,0} parameter(0)
  convert.3 = f32[80,15]{1,0} convert(parameter_0)
  parameter_1 = f32[16,15]{1,0} parameter(1)
  ROOT r.1 = f32[80,16]{1,0} dot(convert.3, parameter_1),
    lhs_contracting_dims={1}, rhs_contracting_dims={1}
}

ENTRY e {
  p1 = f32[16,15]{1,0} parameter(1)
  p0 = s8[80,15]{1,0} parameter(0)
  ROOT triton_gemm_r = f32[80,16]{1,0} fusion(p0, p1), kind=kCustom,
    calls=triton_gemm_r,
    backend_config={kind: "__triton_gemm", triton_gemm_config: {"block_m":32,"block_n":32,"block_k":32,"split_k":1,"num_stages":1,"num_warps":2}}
})";
  CHECK(!tsl::tensor_float_32_execution_enabled());
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> verified_module,
                          ParseAndReturnVerifiedModule(kHloText));

  CompileAndOptionallyVerifyPtx(std::move(verified_module),
                                R"(
CHECK-NOT: mma
)");
}

class TritonGemmTest : public GpuCodegenTest {
 public:
  se::CudaComputeCapability GetCudaComputeCapability() {
    return backend()
        .default_stream_executor()
        ->GetDeviceDescription()
        .cuda_compute_capability();
  }
};

TEST_F(TritonGemmTest, DebugOptionsArePropagated) {
  const std::string kHloText = R"(
ENTRY e {
  p0 = f16[30,30] parameter(0)
  p1 = s8[30,30] parameter(1)
  cp1 = f16[30,30] convert(p1)
  ROOT _ = f16[30,30] dot(p0, cp1),
    lhs_contracting_dims={0}, rhs_contracting_dims={1}
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> verified_module,
                          ParseAndReturnVerifiedModule(kHloText));
  std::string output_directory;
  if (!tsl::io::GetTestUndeclaredOutputsDir(&output_directory)) {
    output_directory = tsl::testing::TmpDir();
  }
  DebugOptions debug_options = verified_module->config().debug_options();
  debug_options.set_xla_dump_to(output_directory);
  debug_options.set_xla_gpu_dump_llvmir(true);
  verified_module->config().set_debug_options(debug_options);

  EXPECT_TRUE(RunAndCompare(std::move(verified_module),
                            ErrorSpec{/*aabs=*/1e-3, /*arel=*/1e-3}));

  std::vector<std::string> paths;
  TF_EXPECT_OK(tsl::Env::Default()->GetMatchingPaths(
      tsl::io::JoinPath(output_directory, "*.triton-passes.log"), &paths));
  EXPECT_EQ(paths.size(), 1);
}

TEST_F(TritonGemmTest, UseTensorCoresForF32OnAmpere) {
  const std::string kHloText = R"(
HloModule t, is_scheduled=true

triton_gemm_r {
  parameter_0 = s8[80,15]{1,0} parameter(0)
  convert.3 = f32[80,15]{1,0} convert(parameter_0)
  parameter_1 = f32[16,15]{1,0} parameter(1)
  ROOT r.1 = f32[80,16]{1,0} dot(convert.3, parameter_1),
    lhs_contracting_dims={1}, rhs_contracting_dims={1}
}

ENTRY e {
  p1 = f32[16,15]{1,0} parameter(1)
  p0 = s8[80,15]{1,0} parameter(0)
  ROOT triton_gemm_r = f32[80,16]{1,0} fusion(p0, p1), kind=kCustom,
    calls=triton_gemm_r,
    backend_config={kind: "__triton_gemm", triton_gemm_config: {"block_m":32,"block_n":32,"block_k":32,"split_k":1,"num_stages":1,"num_warps":2}}
})";
  CHECK(tsl::tensor_float_32_execution_enabled());
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> verified_module,
                          ParseAndReturnVerifiedModule(kHloText));

  if (GetCudaComputeCapability().IsAtLeast(se::CudaComputeCapability::AMPERE)) {
    CompileAndOptionallyVerifyPtx(std::move(verified_module),
                                  R"(
CHECK: mma
)");
  } else {
    CompileAndOptionallyVerifyPtx(std::move(verified_module),
                                  R"(
CHECK: fma
)");
  }
}

TEST_F(TritonGemmTest, FailIfTooMuchShmem) {
  const std::string kHloText = R"(
HloModule module, is_scheduled=true

triton_gemm_dot {
  p0 = s8[1024,1024] parameter(0)
  p1 = f32[1024,1024] parameter(1)
  c0 = f32[1024,1024] convert(p0)
  ROOT dot.0 = f32[1024,1024] dot(c0, p1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

ENTRY entry {
  p0 = s8[1024,1024] parameter(0)
  p1 = f32[1024,1024] parameter(1)
  ROOT r = f32[1024,1024] fusion(p0, p1),
    kind=kCustom, calls=triton_gemm_dot
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> hlo_module,
                          ParseAndReturnVerifiedModule(kHloText));
  const HloComputation* triton_dot_computation =
      hlo_module->entry_computation()
          ->root_instruction()
          ->fused_instructions_computation();
  const GpuDeviceInfo dev_info = TestGpuDeviceInfo::RTXA6000DeviceInfo();
  llvm::LLVMContext llvm_ctx;
  llvm::Module llvm_module("module", llvm_ctx);
  mlir::MLIRContext mlir_context;

  AutotuneResult::TritonGemmKey config;
  config.set_block_m(16);
  config.set_block_n(32);
  config.set_block_k(512);
  config.set_split_k(1);
  config.set_num_stages(4);
  config.set_num_warps(8);
  EXPECT_THAT(
      TritonWrapper("test_fn", triton_dot_computation, kTritonGemmFusionKind,
                    se::CudaComputeCapability{se::CudaComputeCapability::AMPERE,
                                              /*minor=*/0},
                    dev_info, config, &llvm_module, &MatMul, mlir_context),
      tsl::testing::StatusIs(tsl::error::RESOURCE_EXHAUSTED,
                             "Shared memory size limit exceeded."));

  config.set_block_m(64);
  config.set_block_n(128);
  config.set_block_k(128);
  config.set_num_stages(1);
  TF_ASSERT_OK_AND_ASSIGN(
      const LaunchDimensions launch_dimensions,
      TritonWrapper("test_fn", triton_dot_computation, kTritonGemmFusionKind,
                    se::CudaComputeCapability{se::CudaComputeCapability::AMPERE,
                                              /*minor=*/0},
                    dev_info, config, &llvm_module, &MatMul, mlir_context));
  // Use optin shared memory which is > shared_memory_per_block.
  EXPECT_GT(launch_dimensions.SharedMemBytes(),
            dev_info.shared_memory_per_block);
}

TEST_F(TritonGemmTest, MultipleDims) {
  const std::string hlo_text = R"(
HloModule t

ENTRY e {
  p0 = f16[1,16,17,3] parameter(0)
  p1 = s8[16,17,3] parameter(1)
  cp1 = f16[16,17,3] convert(p1)
  ROOT _ = f16[1,16,16] dot(p0, cp1),
    lhs_contracting_dims={2,3}, rhs_contracting_dims={1,2}
})";

  MatchOptimizedHlo(hlo_text, R"(
; CHECK: ENTRY
; CHECK-NEXT: parameter
; CHECK-NEXT: parameter
; CHECK-NEXT: fusion(
; CHECK-SAME: kind=kCustom
; CHECK-SAME: "block_m":
  )");

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{/*aabs=*/1e-3, /*arel=*/1e-3}));
}

TEST_F(TritonGemmTest, NoPadding) {
  const char* hlo_text = R"(
HloModule t

ENTRY e {
  p0 = f16[15,19] parameter(0)
  p1 = s8[19,17] parameter(1)
  cp1 = f16[19,17] convert(p1)
  ROOT _ = f16[15,17] dot(p0, cp1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
})";

  MatchOptimizedHlo(hlo_text, R"(
; CHECK: ENTRY
; CHECK-NEXT: parameter
; CHECK-NEXT: parameter
; CHECK-NEXT: ROOT
; CHECK-SAME: fusion(
; CHECK-SAME: kind=kCustom
; CHECK-SAME: "block_m":
; CHECK-NOT: pad
; CHECK-NOT: slice
)");

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{/*aabs=*/1e-3, /*arel=*/1e-3}));
}

TEST_F(TritonGemmTest, SplitLhsNoncontractingTransposeRhs) {
  const std::string hlo_text = R"(
HloModule t

ENTRY e {
  p0 = s8[3,122,96,12]{3,2,1,0} parameter(0)
  cp0 = f16[3,122,96,12]{3,2,1,0} convert(p0)
  p1 = f16[1,5,122]{2,1,0} parameter(1)
  ROOT _ = f16[3,96,12,1,5]{4,3,2,1,0} dot(cp0, p1),
    lhs_contracting_dims={1}, rhs_contracting_dims={2}
})";

  MatchOptimizedHlo(hlo_text, R"(
; CHECK: ENTRY
; CHECK-NEXT: parameter
; CHECK-NEXT: parameter
; CHECK-NEXT: fusion(
; CHECK-SAME: kind=kCustom
; CHECK-SAME: "block_m":
)");

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{/*aabs=*/1e-2, /*arel=*/1e-2}));
}

TEST_F(TritonGemmTest, SplitLhsNoncontracting) {
  const std::string hlo_text = R"(
HloModule t

ENTRY e {
  p0 = f32[72,72] parameter(0)
  bc1 = f32[4,3,3,2,4,3,3,2] reshape(p0)
  tr = f32[4,3,3,2,2,4,3,3] transpose(bc1), dimensions={0,1,2,3,7,4,5,6}
  bc2 = f32[144,36] reshape(tr)
  p1 = f16[36,3] parameter(1)
  c7 = f32[36,3] convert(p1)
  ROOT _ = f32[144,3] dot(bc2, c7),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
})";

  MatchOptimizedHlo(hlo_text, R"(
; CHECK: ENTRY
; CHECK-NEXT: parameter
; CHECK-NEXT: parameter
; CHECK-NEXT: fusion(
; CHECK-SAME: kind=kCustom
; CHECK-SAME: "block_m":
)");

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{/*aabs=*/1e-3, /*arel=*/1e-3}));
}

TEST_F(TritonGemmTest, SplitAndTransposeLhsExecutesCorrectly) {
  const std::string kHloText = R"(
HloModule m

ENTRY e {
  tmp_0 = s8[5,50,2,128] parameter(1)
  tmp_2 = s8[50,5,2,128] transpose(tmp_0), dimensions={1,0,2,3}
  tmp_3 = s8[50,1280] reshape(tmp_2)
  tmp_4 = f16[50,1280] convert(tmp_3)
  tmp_5 = f16[50,79] parameter(0)
  ROOT tmp_6 = f16[1280,79] dot(tmp_4, tmp_5),
    lhs_contracting_dims={0}, rhs_contracting_dims={0}
})";

  MatchOptimizedHlo(kHloText, R"(
; CHECK: ENTRY
; CHECK-NEXT: parameter
; CHECK-NEXT: parameter
; CHECK-NEXT: ROOT
; CHECK-SAME: fusion
; CHECK-SAME: kind=kCustom
)");

  EXPECT_TRUE(RunAndCompare(kHloText, ErrorSpec{/*aabs=*/1e-3, /*arel=*/1e-3}));
}

TEST_F(TritonGemmTest, NondefaultOperandLayoutIsSupported) {
  // TODO(bchetioui): reenable when b/285866137 is fixed.
#ifndef NDEBUG
  GTEST_SKIP() << "This test times out when -UNDEBUG is set.";
#endif
  const std::string kHloText = R"(
HloModule m

ENTRY r {
  p1 = f16[9,1440,128]{2,1,0} parameter(1)
  cp6 = f16[9,1440,128]{2,0,1} copy(p1)
  cv4 = f32[9,1440,128]{2,0,1} convert(cp6)
  p0 = f32[9,1440,1234]{2,1,0} parameter(0)
  ROOT dot.10 = f32[9,128,1234]{2,1,0} dot(cv4, p0),
    lhs_batch_dims={0}, lhs_contracting_dims={1},
    rhs_batch_dims={0}, rhs_contracting_dims={1}
})";

  MatchOptimizedHlo(kHloText, R"(
; CHECK: ENTRY
; CHECK-NEXT: parameter
; CHECK-NEXT: parameter
; CHECK-NEXT: fusion
; CHECK-SAME: kind=kCustom
)");

  EXPECT_TRUE(RunAndCompare(kHloText, ErrorSpec{/*aabs=*/1e-3, /*arel=*/1e-3}));
}

TEST_F(TritonGemmTest, DoNotFuseSplitRhsContractingTranspose) {
  const std::string hlo_text = R"(
HloModule t

ENTRY e {
  p0 = f16[5,8] parameter(0)
  p1 = s8[2,3,4] parameter(1)
  c0 = f16[2,3,4] convert(p1)
  t1 = f16[3,2,4] transpose(c0), dimensions={1,0,2}
  r1 = f16[3,8] reshape(t1)
  ROOT _ = f16[5,3] dot(p0, r1),
    lhs_contracting_dims={1}, rhs_contracting_dims={1}
})";

  MatchOptimizedHlo(hlo_text, R"(
; CHECK: ENTRY
; CHECK: transpose
; CHECK: fusion
; CHECK-SAME: kind=kCustom
; CHECK-SAME: "block_m":
)");

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{/*aabs=*/1e-3, /*arel=*/1e-3}));
}

TEST_F(TritonGemmTest, DoNotFuseSplitLhsContractingTranspose) {
  const std::string hlo_text = R"(
HloModule t

ENTRY e {
  p0 = f16[3,16,25]{2,1,0} parameter(0)
  p0t = f16[16,3,25]{2,1,0} transpose(p0), dimensions={1,0,2}
  p0tr = f16[16,75]{1,0} reshape(p0t)
  p1 = s8[128,75]{1,0} parameter(1)
  cp1 = f16[128,75]{1,0} convert(p1)
  ROOT dot.126 = f16[16,128]{1,0} dot(p0tr, cp1),
    lhs_contracting_dims={1}, rhs_contracting_dims={1}
})";

  MatchOptimizedHlo(hlo_text, R"(
; CHECK: ENTRY
; CHECK: transpose
; CHECK: fusion
; CHECK-SAME: kind=kCustom
; CHECK-SAME: "block_m":
)");

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{/*aabs=*/1e-3, /*arel=*/1e-3}));
}

TEST_F(TritonGemmTest, BatchF32F16) {
  const std::string hlo_text = R"(
HloModule t

ENTRY e {
  x = f32[5,2,3] parameter(0)
  y = f16[5,3,4] parameter(1)
  cy = f32[5,3,4] convert(y)
  ROOT _ = f32[5,2,4] dot(x, cy),
    lhs_contracting_dims={2}, rhs_contracting_dims={1},
    lhs_batch_dims={0}, rhs_batch_dims={0}
})";

  MatchOptimizedHlo(hlo_text, R"(
; CHECK: ENTRY
; CHECK-NEXT: parameter
; CHECK-NEXT: parameter
; CHECK-NEXT: fusion
; CHECK-SAME: kind=kCustom
; CHECK-SAME: "block_m":
)");

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{/*aabs=*/1e-4, /*arel=*/1e-2}));
}

TEST_F(TritonGemmTest, NonMajorMostInputBatchWorksCorrectly) {
  const std::string hlo_text = R"(
HloModule t

ENTRY e {
  x = f32[20,50,30] parameter(0)
  y = f16[30,50,40] parameter(1)
  cy = f32[30,50,40] convert(y)
  ROOT _ = f32[50,20,40] dot(x, cy),
    lhs_contracting_dims={2}, rhs_contracting_dims={0},
    lhs_batch_dims={1}, rhs_batch_dims={1}
})";

  MatchOptimizedHlo(hlo_text, R"(
; CHECK: ENTRY
; CHECK-NEXT: parameter
; CHECK-NEXT: parameter
; CHECK-NEXT: fusion
; CHECK-SAME: kind=kCustom
; CHECK-SAME: "block_m":
)");

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{/*aabs=*/1e-3, /*arel=*/1e-3}));
}

TEST_F(TritonGemmTest, BatchTransposeF32F16) {
  const std::string hlo_text = R"(
HloModule t

ENTRY e {
  x = f32[5,3,2] parameter(0)
  y = f16[5,3,4] parameter(1)
  cy = f32[5,3,4] convert(y)
  x_transposed = f32[5,2,3] transpose(x), dimensions={0, 2, 1}
  ROOT _ = f32[5,2,4] dot(x_transposed, cy),
    lhs_contracting_dims={2}, rhs_contracting_dims={1},
    lhs_batch_dims={0}, rhs_batch_dims={0}
})";

  MatchOptimizedHlo(hlo_text, R"(
; CHECK: ENTRY
; CHECK-NEXT: parameter
; CHECK-NEXT: parameter
; CHECK-NEXT: fusion
; CHECK-SAME: kind=kCustom
; CHECK-SAME: "block_m":
)");

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{/*aabs=*/1e-4, /*arel=*/1e-2}));
}

TEST_F(TritonGemmTest, DoNotFuseArbitraryReshape) {
  const std::string hlo_text = R"(
HloModule m

ENTRY e {
  p0 = f16[5,2,3] parameter(0)
  p0c = f32[5,2,3] convert(p0)
  p1 = f32[20,3] parameter(1)
  p1r = f32[5,3,4] reshape(p1)
  ROOT dot.5 = f32[5,2,4] dot(p0c, p1r),
    lhs_batch_dims={0}, lhs_contracting_dims={2},
    rhs_batch_dims={0}, rhs_contracting_dims={1}
})";

  MatchOptimizedHlo(hlo_text, R"(
; CHECK: ENTRY
; CHECK: f32[5,3,4]{2,1,0} bitcast(%p1)
; CHECK: fusion
; CHECK-SAME: kind=kCustom
; CHECK-SAME: "block_m":
)");

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{/*aabs=*/1e-4, /*arel=*/1e-4}));
}

TEST_F(TritonGemmTest, MultipleBatchRequireSeparateTranspose) {
  const std::string kHloText = R"(
HloModule m

ENTRY e {
  Arg_0 = f16[3,4,2,5,4] parameter(0)
  c = f32[3,4,2,5,4] convert(Arg_0)
  Arg_1 = f32[5,3,4,3,2] parameter(1)
  ROOT dot.3 = f32[5,3,4,4,3] dot(c, Arg_1),
    lhs_batch_dims={3,0,1}, lhs_contracting_dims={2},
    rhs_batch_dims={0,1,2}, rhs_contracting_dims={4}
})";

  MatchOptimizedHlo(kHloText, R"(
; CHECK: ENTRY
; CHECK: kLoop
; CHECK: kCustom
)");

  EXPECT_TRUE(RunAndCompare(kHloText, ErrorSpec{/*aabs=*/1e-4, /*arel=*/1e-4}));
}

TEST_F(TritonGemmTest, SkipU8) {
  const std::string hlo_text = R"(
HloModule t

ENTRY e {
  p0 = f32[3,3]{1,0} parameter(0)
  p1 = u8[3,3]{1,0} parameter(1)
  c = f32[3,3]{1,0} convert(p1)
  ROOT r = f32[3,3]{1,0} dot(p0, c),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
})";

  MatchOptimizedHlo(hlo_text, R"(
; CHECK: cublas
; CHECK-NOT: triton
)");
}

TEST_F(TritonGemmTest, SkipF32F32) {
  const std::string hlo_text = R"(
HloModule t

ENTRY e {
  p0 = f32[3,5] parameter(0)
  p1 = f32[5,7] parameter(1)
  ROOT _ = f32[3,7] dot(p0, p1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
})";

  MatchOptimizedHlo(hlo_text, R"(
; CHECK: cublas
; CHECK-NOT: triton
)");
}

// This tests the complexity heuristics in TritonWrapper.
TEST_F(TritonGemmTest, FailForTooComplexTiling) {
  const std::string kHloText = R"(
HloModule module, is_scheduled=true

triton_gemm_dot {
  p0 = s8[1024,1024] parameter(0)
  p1 = f32[1024,1024] parameter(1)
  c0 = f32[1024,1024] convert(p0)
  ROOT dot.0 = f32[1024,1024] dot(c0, p1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

ENTRY entry {
  p0 = s8[1024,1024] parameter(0)
  p1 = f32[1024,1024] parameter(1)
  ROOT r = f32[1024,1024] fusion(p0, p1),
    kind=kCustom, calls=triton_gemm_dot
})";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> hlo_module,
                          ParseAndReturnVerifiedModule(kHloText));
  const HloComputation* triton_dot_computation =
      hlo_module->entry_computation()
          ->root_instruction()
          ->fused_instructions_computation();
  const GpuDeviceInfo dev_info = TestGpuDeviceInfo::RTXA6000DeviceInfo();
  llvm::LLVMContext llvm_ctx;
  llvm::Module llvm_module("module", llvm_ctx);
  mlir::MLIRContext mlir_context;

  // Fails if the tiling is too complex.
  AutotuneResult::TritonGemmKey config;
  config.set_block_m(512);
  config.set_block_n(512);
  config.set_block_k(32);
  config.set_split_k(1);
  config.set_num_stages(1);
  config.set_num_warps(2);
  EXPECT_THAT(
      TritonWrapper("test_fn", triton_dot_computation, kTritonGemmFusionKind,
                    se::CudaComputeCapability{se::CudaComputeCapability::AMPERE,
                                              /*minor=*/0},
                    dev_info, config, &llvm_module, &MatMul, mlir_context),
      tsl::testing::StatusIs(
          tsl::error::RESOURCE_EXHAUSTED,
          "Tiling complexity heuristic exceeded: 147456 > 9000"));

  // Succeeds if the tiling is not too complex.
  config.set_block_m(32);
  config.set_block_n(32);
  config.set_block_k(32);
  TF_CHECK_OK(
      TritonWrapper("test_fn", triton_dot_computation, kTritonGemmFusionKind,
                    se::CudaComputeCapability{se::CudaComputeCapability::AMPERE,
                                              /*minor=*/0},
                    dev_info, config, &llvm_module, &MatMul, mlir_context)
          .status());
}

// Triton compiler used to have an issue with reordering constants:
// https://github.com/openai/triton/issues/1864
TEST_F(TritonGemmTest, TritonCompilerDoesNotFailOnConstants) {
  TF_CHECK_OK(GetOptimizedModule(R"(
HloModule m, is_scheduled=true

triton_gemm___computation {
  parameter_0 = f32[92,11]{1,0} parameter(0)
  c = f32[] constant(0)
  b = f32[11,63] broadcast(c)
  ROOT _.1 = f32[92,63]{1,0} dot(parameter_0, b),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

ENTRY e {
  p0 = f32[92,11]{1,0} parameter(0)
  ROOT triton_gemm__ = f32[92,63]{1,0} fusion(p0), kind=kCustom,
    calls=triton_gemm___computation,
    backend_config={"kind":"__triton_gemm",
                    "triton_gemm_config":{"block_m":"16","block_n":"64",
                                          "block_k":"16","split_k":"1",
                                          "num_stages":"3","num_warps":"2"}}
})")
                  .status());
}

class TritonGemmTestAny : public TritonGemmTest {
 public:
  DebugOptions GetDebugOptionsForTest() override {
    DebugOptions debug_options = TritonGemmTest::GetDebugOptionsForTest();
    debug_options.set_xla_gpu_triton_gemm_any(true);
    return debug_options;
  }
};

TEST_F(TritonGemmTestAny, DoF32F32) {
  const std::string hlo_text = R"(
HloModule t

ENTRY e {
  p0 = f32[3,5] parameter(0)
  p1 = f32[5,7] parameter(1)
  ROOT _ = f32[3,7] dot(p0, p1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
})";

  MatchOptimizedHlo(hlo_text, R"(
; CHECK: fusion
; CHECK-SAME: kind=kCustom
; CHECK-SAME: block_m
)");

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{/*aabs=*/1e-3, /*arel=*/1e-3}));
}

TEST_F(TritonGemmTest, SameInput) {
  const std::string hlo_text = R"(
HloModule m

ENTRY e {
  p0 = pred[5,5]{1,0} parameter(0)
  c = f32[5,5]{1,0} convert(p0)
  ROOT r = f32[5,5]{1,0} dot(c, c),
    lhs_contracting_dims={1}, rhs_contracting_dims={1}
})";

  MatchOptimizedHlo(hlo_text, R"(
; CHECK: fusion(%p0), kind=kCustom
; CHECK-SAME: "block_m":
)");

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{/*aabs=*/1e-6, /*arel=*/1e-6}));
}

class TritonGemmLevel2Test : public TritonGemmTest {
 public:
  DebugOptions GetDebugOptionsForTest() override {
    DebugOptions debug_options = HloTestBase::GetDebugOptionsForTest();
    debug_options.set_xla_gpu_triton_fusion_level(2);
    return debug_options;
  }
};

TEST_F(TritonGemmLevel2Test, BinaryOperationWithSmallInputsIsFused) {
  const std::string kHloText = R"(
HloModule m

ENTRY e {
  p0 = s8[7,3] parameter(0)
  p1 = f32[3,16] parameter(1)
  p2 = f32[3,16] parameter(2)
  e = f32[3,16] exponential(p1)
  a = f32[3,16] add(e, p2)
  c = f32[7,3] convert(p0)
  ROOT d = f32[7,16] dot(c, a),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
})";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          GetOptimizedModule(kHloText));

  EXPECT_THAT(
      module->entry_computation()->root_instruction(),
      GmockMatch(m::Fusion(m::Parameter(), m::Parameter(), m::Parameter())
                     .WithFusionKind(HloInstruction::FusionKind::kCustom)));

  EXPECT_TRUE(RunAndCompare(kHloText, ErrorSpec{/*aabs=*/1e-1, /*arel=*/1e-3}));
}

TEST_F(TritonGemmLevel2Test, BinaryOperationWithLargeInputsIsNotFused) {
  const std::string kHloText = R"(
HloModule m

ENTRY e {
  p0 = f16[333,1000] parameter(0)
  p1 = f32[1000,333] parameter(1)
  p1n = f32[1000,333] negate(p1)
  p2 = f32[1000,333] parameter(2)
  p2n = f32[1000,333] negate(p2)
  s = f32[1000,333] subtract(p1n, p2n)
  c = f32[333,1000] convert(p0)
  ROOT d = f32[1000,1000] dot(s, c),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
})";

  MatchOptimizedHlo(kHloText, R"(
; CHECK: fused_computation
; CHECK: negate
; CHECK: negate
; CHECK: ROOT
; CHECK-SAME: subtract
; CHECK: ENTRY
; CHECK: kLoop
; CHECK: kCustom
)");

  EXPECT_TRUE(RunAndCompare(kHloText, ErrorSpec{/*aabs=*/1e-1, /*arel=*/1e-3}));
}

TEST_F(TritonGemmLevel2Test, BinaryOperationOnLargeParametersIsFused) {
  const std::string kHloText = R"(
HloModule m

ENTRY e {
  p0 = f16[1000,111] parameter(0)
  p1 = f32[111,10000] parameter(1)
  p2 = f32[111,10000] parameter(2)
  s = f32[111,10000] subtract(p1, p2)
  c = f32[1000,111] convert(p0)
  ROOT d = f32[10000,1000] dot(s, c),
    lhs_contracting_dims={0}, rhs_contracting_dims={1}
})";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          GetOptimizedModule(kHloText));

  EXPECT_THAT(
      module->entry_computation()->root_instruction(),
      GmockMatch(m::Fusion(m::Parameter(), m::Parameter(), m::Parameter())
                     .WithFusionKind(HloInstruction::FusionKind::kCustom)));

  EXPECT_TRUE(RunAndCompare(kHloText, ErrorSpec{/*aabs=*/1e-1, /*arel=*/1e-3}));
}

TEST_F(TritonGemmLevel2Test, LinkingLibdeviceTwiceWorks) {
  const std::string kHloText = R"(
HloModule m

ENTRY e {
  p0 = s8[7,3] parameter(0)
  c0 = f32[7,3] convert(p0)
  e0 = f32[7,3] exponential(c0)
  p1 = f32[3,16] parameter(1)
  e1 = f32[3,16] exponential(p1)
  d0 = f32[7,16] dot(c0, e1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
  d1 = f32[7,16] dot(e0, p1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
  ROOT a = f32[7,16] add(d0, d1)
})";

  MatchOptimizedHlo(kHloText, R"(
; CHECK: ENTRY
; CHECK-NEXT: parameter
; CHECK-NEXT: parameter
; CHECK-NEXT: kCustom
; CHECK-NEXT: kCustom
; CHECK-NEXT: ROOT
; CHECK-SAME: add
)");

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          GetOptimizedModule(kHloText));

  EXPECT_THAT(module->entry_computation()->root_instruction(),
              GmockMatch(m::Add(
                  m::Fusion(m::Parameter(), m::Parameter())
                      .WithFusionKind(HloInstruction::FusionKind::kCustom),
                  m::Fusion(m::Parameter(), m::Parameter())
                      .WithFusionKind(HloInstruction::FusionKind::kCustom))));

  EXPECT_TRUE(RunAndCompare(kHloText, ErrorSpec{/*aabs=*/1e-2, /*arel=*/1e-2}));
}

TEST_F(TritonGemmLevel2Test, BroadcastOfScalarConstantIsFused) {
  const std::string kHloText = R"(
HloModule m

ENTRY e {
  p0 = f16[70,30] parameter(0)
  p0c = f32[70,30] convert(p0)
  constant_3663 = f32[] constant(4321)
  bc0 = f32[30,5] broadcast(constant_3663)
  ROOT d = f32[70,5] dot(p0c, bc0),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
})";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          GetOptimizedModule(kHloText));
  EXPECT_THAT(
      module->entry_computation()->root_instruction(),
      GmockMatch(m::Fusion(m::Parameter())
                     .WithFusionKind(HloInstruction::FusionKind::kCustom)));

  EXPECT_TRUE(RunAndCompare(kHloText, ErrorSpec{/*aabs=*/2e-3, /*arel=*/2e-3}));
}

TEST_F(TritonGemmTest, SineOutputIsNotFused) {
  const std::string kHloText = R"(
HloModule m

ENTRY e {
  p0 = s8[7,101] parameter(0)
  p1 = f32[101,16] parameter(1)
  c = f32[7,101] convert(p0)
  d = f32[7,16] dot(c, p1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
  ROOT r = f32[7,16] sine(d)
})";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          GetOptimizedModule(kHloText));
  EXPECT_THAT(module->entry_computation()->root_instruction(),
              GmockMatch(m::Sin(
                  m::Fusion(m::Parameter(), m::Parameter())
                      .WithFusionKind(HloInstruction::FusionKind::kCustom))));

  EXPECT_TRUE(RunAndCompare(kHloText, ErrorSpec{/*aabs=*/1e-1, /*arel=*/1e-2}));
}

TEST_F(TritonGemmLevel2Test, NarrowingConvertOutputIsFused) {
  const std::string kHloText = R"(
HloModule m

ENTRY e {
  p0 = s8[22,80] parameter(0)
  p1 = f32[80,54] parameter(1)
  c = f32[22,80] convert(p0)
  d = f32[54,22] dot(p1, c),
    lhs_contracting_dims={0}, rhs_contracting_dims={1}
  ROOT r = f16[54,22] convert(d)
})";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          GetOptimizedModule(kHloText));
  EXPECT_THAT(
      module->entry_computation()->root_instruction(),
      GmockMatch(m::Fusion(m::Parameter(), m::Parameter())
                     .WithFusionKind(HloInstruction::FusionKind::kCustom)));

  EXPECT_TRUE(RunAndCompare(kHloText, ErrorSpec{/*aabs=*/3e-2, /*arel=*/3e-2}));
}

TEST_F(TritonGemmLevel2Test, ParameterAfterDotIsFused) {
  if (!GetCudaComputeCapability().IsAtLeast(
          se::CudaComputeCapability::AMPERE)) {
    GTEST_SKIP() << "No BF16 before Ampere.";
  }

  const std::string kHloText = R"(
HloModule m

ENTRY e {
  p0 = bf16[350,1280]{1,0} parameter(0)
  p1 = s16[1280,690]{0,1} parameter(1)
  p1c = bf16[1280,690]{0,1} convert(p1)
  dot.21 = bf16[350,690]{1,0} dot(p0, p1c),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
  p2 = bf16[350,690]{1,0} parameter(2)
  ROOT r = bf16[350,690]{1,0} multiply(p2, dot.21)
})";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          GetOptimizedModule(kHloText));
  const HloInstruction* instr = module->entry_computation()->root_instruction();
  if (!instr->IsCustomFusion()) {
    instr = instr->operand(0);
    ASSERT_TRUE(instr->IsCustomFusion());
  }
  EXPECT_THAT(
      instr,
      GmockMatch(m::Fusion(m::Parameter(), m::Parameter(), m::Parameter())
                     .WithFusionKind(HloInstruction::FusionKind::kCustom)));

  EXPECT_TRUE(RunAndCompare(kHloText, ErrorSpec{/*aabs=*/2e-2, /*arel=*/2e-2}));
}

TEST_F(TritonGemmLevel2Test, OutputFusionExecutesCorrectly) {
  if (!GetCudaComputeCapability().IsAtLeast(
          se::CudaComputeCapability::AMPERE)) {
    GTEST_SKIP() << "No BF16 before Ampere.";
  }

  const std::string kHloText = R"(
HloModule m

ENTRY e {
  p0 = f16[350,1280]{1,0} parameter(0)
  p0c = bf16[350,1280]{1,0} convert(p0)
  p1 = bf16[1280,690]{0,1} parameter(1)
  d = bf16[350,690]{1,0} dot(p0c, p1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
  p3 = bf16[350,690]{1,0} parameter(3)
  multiply.8811 = bf16[350,690]{1,0} multiply(d, p3)
  neg.484 = bf16[350,690]{1,0} negate(multiply.8811)
  p2 = bf16[350,690]{1,0} parameter(2)
  ROOT multiply.8808 = bf16[350,690]{1,0} multiply(neg.484, p2)
})";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          GetOptimizedModule(kHloText));
  const HloInstruction* instr = module->entry_computation()->root_instruction();
  if (!instr->IsCustomFusion()) {
    instr = instr->operand(0);
    ASSERT_TRUE(instr->IsCustomFusion());
  }
  EXPECT_THAT(
      instr,
      GmockMatch(m::Fusion(m::Parameter(), m::Parameter(), m::Parameter(),
                           m::Parameter())
                     .WithFusionKind(HloInstruction::FusionKind::kCustom)));

  EXPECT_TRUE(RunAndCompare(kHloText, ErrorSpec{/*aabs=*/2e-2, /*arel=*/2e-2}));
}

TEST_F(TritonGemmLevel2Test, SplitLHSOutputTransposeAloneIsNotFused) {
  if (!GetCudaComputeCapability().IsAtLeast(
          se::CudaComputeCapability::AMPERE)) {
    GTEST_SKIP() << "No BF16 before Ampere.";
  }

  const std::string kHloText = R"(
HloModule m

ENTRY e {
  p0 = s8[18,15000] parameter(0)
  p0c = bf16[18,15000] convert(p0)
  p1 = bf16[42,18] parameter(1)
  d = bf16[15000,42] dot(p0c, p1),
    lhs_contracting_dims={0}, rhs_contracting_dims={1}
  r1 = bf16[5,200,15,42] reshape(d)
  ROOT t1 = bf16[5,42,200,15] transpose(r1), dimensions={0,3,1,2}
})";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          GetOptimizedModule(kHloText));
  EXPECT_THAT(module->entry_computation()->root_instruction(),
              GmockMatch(m::Transpose(
                  m::Fusion(m::Parameter(), m::Parameter())
                      .WithFusionKind(HloInstruction::FusionKind::kCustom))));

  EXPECT_TRUE(RunAndCompare(kHloText, ErrorSpec{/*aabs=*/1e-3, /*arel=*/1e-3}));
}

TEST_F(TritonGemmLevel2Test, SplitLHSInputOutputIsFused) {
  if (!GetCudaComputeCapability().IsAtLeast(
          se::CudaComputeCapability::AMPERE)) {
    GTEST_SKIP() << "No BF16 before Ampere.";
  }

  const std::string kHloText = R"(
ENTRY e {
  p0t = (s8[5,18,20,150]) parameter(0)
  p0 = s8[5,18,20,150] get-tuple-element(p0t), index=0
  p0c = bf16[5,18,20,150] convert(p0)
  t0 = bf16[18,5,20,150] transpose(p0c), dimensions={1,0,2,3}
  r0 = bf16[18,15000] reshape(t0)
  p1 = bf16[42,18] parameter(1)
  d = bf16[15000,42] dot(r0, p1),
    lhs_contracting_dims={0}, rhs_contracting_dims={1}
  r1 = bf16[5,20,150,42] reshape(d)
  ROOT t1 = bf16[5,42,20,150] transpose(r1), dimensions={0,3,1,2}
})";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          GetOptimizedModule(kHloText));
  EXPECT_THAT(
      module->entry_computation()->root_instruction(),
      GmockMatch(m::Fusion(m::GetTupleElement(), m::Parameter())
                     .WithFusionKind(HloInstruction::FusionKind::kCustom)));

  EXPECT_TRUE(RunAndCompare(kHloText, ErrorSpec{/*aabs=*/1e-3, /*arel=*/1e-3}));
}

TEST_F(TritonGemmTest, Naming) {
  const char* hlo_text = R"(
HloModule t

ENTRY e {
  p0 = f16[15,19] parameter(0)
  p1 = s8[19,17] parameter(1)
  cp1 = f16[19,17] convert(p1)
  ROOT r = f16[15,17] dot(p0, cp1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
})";

  MatchOptimizedHlo(hlo_text, R"(
; CHECK: %triton_gemm_r_computation (
; CHECK: %triton_gemm_r =
; CHECK-SAME: fusion
)");
}

TEST_F(TritonGemmTestAny,
       ShouldNotLowerDotWithLhsWithoutNonContractingDimThroughTriton) {
  const std::string hlo_text = R"(
HloModule t

ENTRY e {
  parameter_0 = f32[32,40] parameter(0)
  parameter_1 = f32[32,40,64] parameter(1)
  ROOT dot = f32[32,64] dot(f32[32,40] parameter_0, f32[32,40,64] parameter_1), lhs_batch_dims={0}, lhs_contracting_dims={1}, rhs_batch_dims={0}, rhs_contracting_dims={1}
})";

  MatchOptimizedHlo(hlo_text, "CHECK-NOT: triton");

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{/*aabs=*/1e-6, /*arel=*/1e-6}));
}

TEST_F(TritonGemmTestAny,
       ShouldNotLowerDotWithRhsWithoutNonContractingDimThroughTriton) {
  const std::string hlo_text = R"(
HloModule t

ENTRY e {
  parameter_0 = f32[32,40,64] parameter(0)
  parameter_1 = f32[32,40] parameter(1)
  ROOT dot = f32[32,64] dot(f32[32,40,64] parameter_0, f32[32,40] parameter_1), lhs_batch_dims={0}, lhs_contracting_dims={1}, rhs_batch_dims={0}, rhs_contracting_dims={1}
})";

  MatchOptimizedHlo(hlo_text, "CHECK-NOT: triton");

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{/*aabs=*/1e-6, /*arel=*/1e-6}));
}

// This group of tests compares GPU results of dots already rewritten
// into Triton fusions.
using CompareTest = TritonGemmTest;

TEST_F(CompareTest, DifferentTilingsProduceSameResult) {
  const char* hlo_text_ref = R"(
HloModule t

triton_dot {
  p0 = s8[101,202] parameter(0)
  p0c = f32[101,202] convert(p0)
  p1 = f32[202,303] parameter(1)
  ROOT dot = f32[101,303] dot(p0c, p1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

ENTRY e {
  p0 = s8[101,202]{1,0} parameter(0)
  p1 = f32[202,303]{1,0} parameter(1)
  ROOT _ = f32[101,303] fusion(p0, p1), kind=kCustom, calls=triton_dot,
    backend_config={kind: "__triton_gemm", triton_gemm_config: {"block_m":16,"block_n":64,"block_k":32,"split_k":1,"num_stages":3,"num_warps":8}}
})";

  const char* hlo_text_triton = R"(
HloModule t

triton_dot {
  p0 = s8[101,202] parameter(0)
  p0c = f32[101,202] convert(p0)
  p1 = f32[202,303] parameter(1)
  ROOT dot = f32[101,303] dot(p0c, p1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

ENTRY e {
  p0 = s8[101,202]{1,0} parameter(0)
  p1 = f32[202,303]{1,0} parameter(1)
  ROOT _ = f32[101,303] fusion(p0, p1), kind=kCustom, calls=triton_dot,
    backend_config={kind: "__triton_gemm", triton_gemm_config: {"block_m":32,"block_n":128,"block_k":32,"split_k":1,"num_stages":2,"num_warps":4}}
})";

  EXPECT_TRUE(RunAndCompareTwoModules(hlo_text_ref, hlo_text_triton,
                                      ErrorSpec{/*aabs=*/1e-6, /*arel=*/1e-6},
                                      /*run_hlo_passes=*/false));
}

TEST_F(CompareTest, F16) {
  const char* hlo_text_ref = R"(
HloModule r

ENTRY e {
  arg0 = f16[5,7] parameter(0)
  arg1 = f16[7,33] parameter(1)
  ROOT custom-call = f16[5,33] custom-call(arg0, arg1),
    custom_call_target="__cublas$gemm",
    backend_config={"alpha_real":1,"beta":0,"dot_dimension_numbers":{"lhs_contracting_dimensions":[1],"rhs_contracting_dimensions":[0],"lhs_batch_dimensions":[],"rhs_batch_dimensions":[]},"alpha_imag":0,"precision_config":{"operand_precision":["DEFAULT","DEFAULT"]},"epilogue":"DEFAULT"}
}
)";

  const char* hlo_text_triton = R"(
HloModule t

triton_dot {
  p0 = f16[5,7] parameter(0)
  p1 = f16[7,33] parameter(1)
  ROOT dot = f16[5,33] dot(p0, p1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

ENTRY e {
  p0 = f16[5,7]{1,0} parameter(0)
  p1 = f16[7,33]{1,0} parameter(1)
  ROOT _ = f16[5,33] fusion(p0, p1), kind=kCustom, calls=triton_dot,
    backend_config={kind: "__triton_gemm", triton_gemm_config: {"block_m":32,"block_n":32,"block_k":32,"split_k":1,"num_stages":1,"num_warps":1}}
}
)";

  EXPECT_TRUE(RunAndCompareTwoModules(hlo_text_ref, hlo_text_triton,
                                      ErrorSpec{/*aabs=*/1e-6, /*arel=*/1e-6},
                                      /*run_hlo_passes=*/false));
}

TEST_F(CompareTest, F32) {
  const char* hlo_text_ref = R"(
HloModule r

ENTRY e {
  arg0 = f32[5,7] parameter(0)
  arg1 = f32[7,33] parameter(1)
  ROOT custom-call = f32[5,33] custom-call(arg0, arg1),
    custom_call_target="__cublas$gemm",
    backend_config={"alpha_real":1,"beta":0,"dot_dimension_numbers":{"lhs_contracting_dimensions":[1],"rhs_contracting_dimensions":[0],"lhs_batch_dimensions":[],"rhs_batch_dimensions":[]},"alpha_imag":0,"precision_config":{"operand_precision":["DEFAULT","DEFAULT"]},"epilogue":"DEFAULT"}
}
)";

  const char* hlo_text_triton = R"(
HloModule t

triton_dot {
  p0 = f32[5,7] parameter(0)
  p1 = f32[7,33] parameter(1)
  ROOT dot = f32[5,33] dot(p0, p1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

ENTRY e {
  p0 = f32[5,7]{1,0} parameter(0)
  p1 = f32[7,33]{1,0} parameter(1)
  ROOT _ = f32[5,33] fusion(p0, p1), kind=kCustom, calls=triton_dot,
    backend_config={kind: "__triton_gemm", triton_gemm_config: {"block_m":32,"block_n":32,"block_k":32,"split_k":1,"num_stages":1,"num_warps":1}}
}
)";

  EXPECT_TRUE(RunAndCompareTwoModules(hlo_text_ref, hlo_text_triton,
                                      ErrorSpec{/*aabs=*/1e-3, /*arel=*/1e-3},
                                      /*run_hlo_passes=*/false));
}

TEST_F(CompareTest, BF16TransposedLHS) {
  if (!GetCudaComputeCapability().IsAtLeast(
          se::CudaComputeCapability::AMPERE)) {
    GTEST_SKIP() << "No BF16 before Ampere.";
  }

  const char* hlo_text_ref = R"(
HloModule r

ENTRY e {
  arg0 = bf16[512,16]{1,0} parameter(0)
  arg1 = bf16[512,256]{1,0} parameter(1)
  ROOT custom-call = bf16[16,256]{1,0} custom-call(arg0, arg1),
    custom_call_target="__cublas$gemm",
    backend_config={"alpha_real":1,"beta":0,"dot_dimension_numbers":{"lhs_contracting_dimensions":[0],"rhs_contracting_dimensions":[0],"lhs_batch_dimensions":[],"rhs_batch_dimensions":[]},"alpha_imag":0,"precision_config":{"operand_precision":["DEFAULT","DEFAULT"]},"epilogue":"DEFAULT"}
}
)";

  const char* hlo_text_triton = R"(
HloModule t

triton_dot {
  arg0 = bf16[512,16]{1,0} parameter(0)
  arg1 = bf16[512,256]{1,0} parameter(1)
  ROOT dot = bf16[16,256]{1,0} dot(arg0, arg1),
    lhs_contracting_dims={0}, rhs_contracting_dims={0}
}

ENTRY e {
  arg0 = bf16[512,16]{1,0} parameter(0)
  arg1 = bf16[512,256]{1,0} parameter(1)
  ROOT _ = bf16[16,256]{1,0} fusion(arg0, arg1), kind=kCustom, calls=triton_dot,
    backend_config={kind: "__triton_gemm", triton_gemm_config: {"block_m":128,"block_n":32,"block_k":16,"split_k":1,"num_stages":2,"num_warps":4}}
}
)";

  EXPECT_TRUE(RunAndCompareTwoModules(hlo_text_ref, hlo_text_triton,
                                      ErrorSpec{/*aabs=*/1e-2, /*arel=*/1e-2},
                                      /*run_hlo_passes=*/false));
}

TEST_F(CompareTest, UsingOptinSharedMemoryOnAmpereProducesSameResult) {
  // On pre-Ampere GPUs the test would use a different amount of shared memory.
  if (!GetCudaComputeCapability().IsAtLeast(
          se::CudaComputeCapability::AMPERE)) {
    GTEST_SKIP() << "This test is for Ampere+ GPUs.";
  }
  const GpuDeviceInfo dev_info =
      GetGpuDeviceInfo(backend().default_stream_executor());
  constexpr int kBytesOfSharedMemoryTested = 64 * 1024;
  EXPECT_GE(dev_info.shared_memory_per_block_optin, kBytesOfSharedMemoryTested);

  const std::string kHloTextOptinShmem = R"(
HloModule t

triton_dot {
  param_0.1 = s8[332,441]{1,0} parameter(0)
  p0c = f16[332,441]{1,0} convert(param_0.1)
  param_1.1 = f16[441,39]{1,0} parameter(1)
  ROOT dot = f16[332,39]{1,0} dot(p0c, param_1.1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

ENTRY e {
  p0 = s8[332,441]{1,0} parameter(0)
  p1 = f16[441,39]{1,0} parameter(1)
  ROOT _ = f16[332,39]{1,0} fusion(p0, p1), kind=kCustom, calls=triton_dot,
    backend_config={kind: "__triton_gemm", triton_gemm_config: {"block_m":128,"block_n":128,"block_k":128,"split_k":1,"num_stages":2,"num_warps":32}}
})";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<VerifiedHloModule> hlo_module,
                          ParseAndReturnVerifiedModule(kHloTextOptinShmem));
  const HloComputation* triton_dot_computation =
      hlo_module->entry_computation()
          ->root_instruction()
          ->fused_instructions_computation();
  llvm::LLVMContext llvm_ctx;
  llvm::Module llvm_module("module", llvm_ctx);
  mlir::MLIRContext mlir_context;

  TF_ASSERT_OK_AND_ASSIGN(auto config,
                          hlo_module->entry_computation()
                              ->root_instruction()
                              ->backend_config<FusionBackendConfig>());
  TF_ASSERT_OK_AND_ASSIGN(
      const LaunchDimensions launch_dimensions,
      TritonWrapper("test_fn", triton_dot_computation, kTritonGemmFusionKind,
                    GetCudaComputeCapability(), dev_info,
                    config.triton_gemm_config(), &llvm_module, &MatMul,
                    mlir_context));
  // The config is chosen so that the used memory size is slightly above the
  // 48 kB boundary of standard / optin shared memory so that any GPU that
  // has the optin one should be able to execute the test.
  EXPECT_EQ(launch_dimensions.SharedMemBytes(), kBytesOfSharedMemoryTested);
  // Make sure the written config indeed has to use optin shared memory.
  EXPECT_GT(launch_dimensions.SharedMemBytes(),
            dev_info.shared_memory_per_block);

  const std::string kHloTextLowShmem = R"(
HloModule t

triton_dot {
  param_0.1 = s8[332,441]{1,0} parameter(0)
  p0c = f16[332,441]{1,0} convert(param_0.1)
  param_1.1 = f16[441,39]{1,0} parameter(1)
  ROOT dot = f16[332,39]{1,0} dot(p0c, param_1.1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

ENTRY e {
  p0 = s8[332,441]{1,0} parameter(0)
  p1 = f16[441,39]{1,0} parameter(1)
  ROOT _ = f16[332,39]{1,0} fusion(p0, p1), kind=kCustom, calls=triton_dot,
    backend_config={kind: "__triton_gemm", triton_gemm_config: {"block_m":32,"block_n":32,"block_k":32,"split_k":1,"num_stages":1,"num_warps":4}}
})";

  EXPECT_TRUE(RunAndCompareTwoModules(kHloTextLowShmem, kHloTextOptinShmem,
                                      ErrorSpec{/*aabs=*/1e-6, /*arel=*/1e-6},
                                      /*run_hlo_passes=*/false));
}

TEST_F(CompareTest, F16TransposedRHS) {
  const char* hlo_text_ref = R"(
HloModule r

ENTRY e {
  arg0 = f16[128,32]{1,0} parameter(0)
  arg1 = f16[64,32]{1,0} parameter(1)
  ROOT custom-call = f16[128,64]{1,0} custom-call(arg0, arg1),
    custom_call_target="__cublas$gemm",
    backend_config={"alpha_real":1,"beta":0,"dot_dimension_numbers":{"lhs_contracting_dimensions":[1],"rhs_contracting_dimensions":[1],"lhs_batch_dimensions":[],"rhs_batch_dimensions":[]},"alpha_imag":0,"precision_config":{"operand_precision":["DEFAULT","DEFAULT"]},"epilogue":"DEFAULT"}
}
)";

  const char* hlo_text_triton = R"(
HloModule t

triton_dot {
  arg0 = f16[128,32]{1,0} parameter(0)
  arg1 = f16[64,32]{1,0} parameter(1)
  ROOT dot = f16[128,64]{1,0} dot(arg0, arg1),
    lhs_contracting_dims={1}, rhs_contracting_dims={1}
}

ENTRY e {
  arg0 = f16[128,32]{1,0} parameter(0)
  arg1 = f16[64,32]{1,0} parameter(1)
  ROOT _ = f16[128,64]{1,0} fusion(arg0, arg1), kind=kCustom, calls=triton_dot,
    backend_config={kind: "__triton_gemm", triton_gemm_config: {"block_m":128,"block_n":32,"block_k":64,"split_k":1,"num_stages":2,"num_warps":4}}
}
)";

  EXPECT_TRUE(RunAndCompareTwoModules(hlo_text_ref, hlo_text_triton,
                                      ErrorSpec{/*aabs=*/1e-2, /*arel=*/1e-2},
                                      /*run_hlo_passes=*/false));
}

TEST_F(CompareTest, F32TransposedBoth) {
  const char* hlo_text_ref = R"(
HloModule r

ENTRY e {
  arg0 = f32[64,128]{1,0} parameter(0)
  arg1 = f32[1024,64]{1,0} parameter(1)
  ROOT custom-call = f32[128,1024]{1,0} custom-call(arg0, arg1),
    custom_call_target="__cublas$gemm",
    backend_config={"alpha_real":1,"beta":0,"dot_dimension_numbers":{"lhs_contracting_dimensions":[0],"rhs_contracting_dimensions":[1],"lhs_batch_dimensions":[],"rhs_batch_dimensions":[]},"alpha_imag":0,"precision_config":{"operand_precision":["DEFAULT","DEFAULT"]},"epilogue":"DEFAULT"}
}
)";

  const char* hlo_text_triton = R"(
HloModule t

triton_dot {
  arg0 = f32[64,128]{1,0} parameter(0)
  arg1 = f32[1024,64]{1,0} parameter(1)
  ROOT dot = f32[128,1024]{1,0} dot(arg0, arg1),
    lhs_contracting_dims={0}, rhs_contracting_dims={1}
}

ENTRY e {
  arg0 = f32[64,128]{1,0} parameter(0)
  arg1 = f32[1024,64]{1,0} parameter(1)
  ROOT _ = f32[128,1024]{1,0} fusion(arg0, arg1), kind=kCustom, calls=triton_dot,
    backend_config={kind: "__triton_gemm", triton_gemm_config: {"block_m":32,"block_n":32,"block_k":64,"split_k":1,"num_stages":2,"num_warps":4}}
}
)";

  EXPECT_TRUE(RunAndCompareTwoModules(hlo_text_ref, hlo_text_triton,
                                      ErrorSpec{/*aabs=*/1e-3, /*arel=*/1e-3},
                                      /*run_hlo_passes=*/false));
}

TEST_F(CompareTest, S8BF16) {
  if (!GetCudaComputeCapability().IsAtLeast(
          se::CudaComputeCapability::AMPERE)) {
    GTEST_SKIP() << "No BF16 before Ampere.";
  }
  const char* hlo_text_ref = R"(
HloModule r

fused_computation {
  param_0.1 = s8[144,256]{1,0} parameter(0)
  ROOT convert.4 = bf16[144,256]{1,0} convert(param_0.1)
}

ENTRY e {
  p0 = s8[144,256]{1,0} parameter(0)
  fusion = bf16[144,256]{1,0} fusion(p0), kind=kInput, calls=fused_computation
  p1 = bf16[256,122]{1,0} parameter(1)
  ROOT custom-call = bf16[144,122]{1,0} custom-call(fusion, p1),
    custom_call_target="__cublas$gemm",
    backend_config={"alpha_real":1,"beta":0,"dot_dimension_numbers":{"lhs_contracting_dimensions":[1],"rhs_contracting_dimensions":[0],"lhs_batch_dimensions":[],"rhs_batch_dimensions":[]},"alpha_imag":0,"precision_config":{"operand_precision":["DEFAULT","DEFAULT"]},"epilogue":"DEFAULT"}
}
)";

  const char* hlo_text_triton = R"(
HloModule t

triton_dot {
  param_0.1 = s8[144,256]{1,0} parameter(0)
  p0c = bf16[144,256]{1,0} convert(param_0.1)
  param_1.1 = bf16[256,122]{1,0} parameter(1)
  ROOT dot = bf16[144,122]{1,0} dot(p0c, param_1.1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

ENTRY e {
  p0 = s8[144,256]{1,0} parameter(0)
  p1 = bf16[256,122]{1,0} parameter(1)
  ROOT _ = bf16[144,122]{1,0} fusion(p0, p1), kind=kCustom, calls=triton_dot,
    backend_config={kind: "__triton_gemm", triton_gemm_config: {"block_m":64,"block_n":64,"block_k":64,"split_k":1,"num_stages":1,"num_warps":2}}
}
)";

  EXPECT_TRUE(RunAndCompareTwoModules(hlo_text_ref, hlo_text_triton,
                                      ErrorSpec{/*aabs=*/1e-6, /*arel=*/1e-6},
                                      /*run_hlo_passes=*/false));
}

TEST_F(CompareTest, SplitK) {
  if (!GetCudaComputeCapability().IsAtLeast(
          se::CudaComputeCapability::AMPERE)) {
    GTEST_SKIP() << "No BF16 before Ampere.";
  }
  const std::string hlo_text_ref = R"(
HloModule t, is_scheduled=true

triton_gemm_r {
  parameter_0 = s8[480,120]{1,0} parameter(0)
  convert.3 = bf16[480,120]{1,0} convert(parameter_0)
  parameter_1 = bf16[16,120]{1,0} parameter(1)
  ROOT r.1 = bf16[480,16]{1,0} dot(convert.3, parameter_1),
    lhs_contracting_dims={1}, rhs_contracting_dims={1}
}

ENTRY e {
  p1 = bf16[16,120]{1,0} parameter(1)
  p0 = s8[3,120,5,32]{3,2,1,0} parameter(0)
  bitcast.4 = s8[480,120]{1,0} bitcast(p0)
  ROOT triton_gemm_r = bf16[480,16]{1,0} fusion(bitcast.4, p1), kind=kCustom,
    calls=triton_gemm_r,
    backend_config={kind: "__triton_gemm", triton_gemm_config: {"block_m":64,"block_n":32,"block_k":64,"split_k":1,"num_stages":4,"num_warps":4}}
})";

  const std::string hlo_text_splitk = R"(
HloModule t, is_scheduled=true

triton_gemm_r {
  parameter_0 = s8[480,120]{1,0} parameter(0)
  convert.3 = bf16[480,120]{1,0} convert(parameter_0)
  bitcast.11 = bf16[480,4,30]{2,1,0} bitcast(convert.3)
  parameter_1 = bf16[16,120]{1,0} parameter(1)
  bitcast.12 = bf16[16,4,30]{2,1,0} bitcast(parameter_1)
  ROOT dot.1 = bf16[4,480,16]{2,1,0} dot(bitcast.11, bitcast.12),
    lhs_batch_dims={1}, lhs_contracting_dims={2},
    rhs_batch_dims={1}, rhs_contracting_dims={2}
}

add {
  rhs.1 = f32[] parameter(1)
  lhs.1 = f32[] parameter(0)
  ROOT add.1 = f32[] add(lhs.1, rhs.1)
}

fused_computation {
  param_0.2 = bf16[4,480,16]{2,1,0} parameter(0)
  convert.18 = f32[4,480,16]{2,1,0} convert(param_0.2)
  constant_1 = bf16[] constant(0)
  convert.17 = f32[] convert(constant_1)
  reduce.1 = f32[480,16]{1,0} reduce(convert.18, convert.17), dimensions={0},
    to_apply=add
  ROOT convert.16 = bf16[480,16]{1,0} convert(reduce.1)
}

ENTRY e {
  p1 = bf16[16,120]{1,0} parameter(1)
  p0 = s8[3,120,5,32]{3,2,1,0} parameter(0)
  bitcast.4 = s8[480,120]{1,0} bitcast(p0)
  triton_gemm_r = bf16[4,480,16]{2,1,0} fusion(bitcast.4, p1), kind=kCustom,
    calls=triton_gemm_r,
    backend_config={kind: "__triton_gemm", triton_gemm_config: {"block_m":32,"block_n":32,"block_k":128,"split_k":4,"num_stages":1,"num_warps":4}}
  ROOT fusion.1 = bf16[480,16]{1,0} fusion(triton_gemm_r), kind=kLoop,
    calls=fused_computation
})";

  EXPECT_TRUE(RunAndCompareTwoModules(hlo_text_ref, hlo_text_splitk,
                                      ErrorSpec{/*aabs=*/1e-6, /*arel=*/1e-6},
                                      /*run_hlo_passes=*/false));
}

TEST_F(CompareTest, SplitKBatch) {
  if (!GetCudaComputeCapability().IsAtLeast(
          se::CudaComputeCapability::AMPERE)) {
    GTEST_SKIP() << "No BF16 before Ampere.";
  }
  const std::string kHloTextRef = R"(
HloModule m, is_scheduled=true

triton_gemm_dot.24 {
  parameter_1 = bf16[1,1,800,5,128]{4,3,2,1,0} parameter(1)
  bitcast.3 = bf16[800,5,128]{2,1,0} bitcast(parameter_1)
  convert.3 = f32[800,5,128]{2,1,0} convert(bitcast.3)
  parameter_0 = f32[1,5,700,800]{3,2,1,0} parameter(0)
  bitcast.2 = f32[5,700,800]{2,1,0} bitcast(parameter_0)
  ROOT dot.26 = f32[5,128,700]{2,1,0} dot(convert.3, bitcast.2), lhs_batch_dims={1}, lhs_contracting_dims={0}, rhs_batch_dims={0}, rhs_contracting_dims={2}
}

ENTRY e {
  tmp_3 = f32[1,5,700,800]{3,2,1,0} parameter(0)
  tmp_0 = bf16[1,1,800,5,128]{4,3,2,1,0} parameter(1)
  ROOT triton_gemm_dot.24 = f32[5,128,700]{2,1,0} fusion(tmp_3, tmp_0),
    kind=kCustom, calls=triton_gemm_dot.24,
    backend_config={kind: "__triton_gemm", triton_gemm_config: {"block_m":64,"block_n":32,"block_k":64,"split_k":1,"num_stages":2,"num_warps":8}}
})";

  const std::string kHloTextSplitK = R"(
HloModule m, is_scheduled=true

triton_gemm_dot {
  parameter_1 = bf16[1,1,800,5,128]{4,3,2,1,0} parameter(1)
  bitcast.3 = bf16[800,5,128]{2,1,0} bitcast(parameter_1)
  convert.3 = f32[800,5,128]{2,1,0} convert(bitcast.3)
  bitcast = f32[8,100,5,128]{3,2,1,0} bitcast(convert.3)
  parameter_0 = f32[1,5,700,800]{3,2,1,0} parameter(0)
  bitcast.2 = f32[5,700,800]{2,1,0} bitcast(parameter_0)
  bitcast.1 = f32[5,700,8,100]{3,2,1,0} bitcast(bitcast.2)
  ROOT dot = f32[8,5,128,700]{3,2,1,0} dot(bitcast, bitcast.1), lhs_batch_dims={0,2}, lhs_contracting_dims={1}, rhs_batch_dims={2,0}, rhs_contracting_dims={3}
}

add {
  lhs = f32[] parameter(0)
  rhs = f32[] parameter(1)
  ROOT add = f32[] add(lhs, rhs)
}

ENTRY e {
  tmp_3 = f32[1,5,700,800]{3,2,1,0} parameter(0)
  tmp_0 = bf16[1,1,800,5,128]{4,3,2,1,0} parameter(1)
  triton_gemm_dot.24 = f32[8,5,128,700]{3,2,1,0} fusion(tmp_3, tmp_0),
    kind=kCustom, calls=triton_gemm_dot,
    backend_config={kind: "__triton_gemm", triton_gemm_config: {"block_m":64,"block_n":32,"block_k":64,"split_k":8,"num_stages":1,"num_warps":4}}
  constant = f32[] constant(0)
  ROOT reduce = f32[5,128,700]{2,1,0} reduce(triton_gemm_dot.24, constant), dimensions={0}, to_apply=add
})";

  EXPECT_TRUE(RunAndCompareTwoModules(kHloTextRef, kHloTextSplitK,
                                      ErrorSpec{/*aabs=*/1e-3, /*arel=*/1e-3},
                                      /*run_hlo_passes=*/false));
}

TEST_F(CompareTest, SplitKNontrivialBitcast) {
  if (!GetCudaComputeCapability().IsAtLeast(
          se::CudaComputeCapability::AMPERE)) {
    GTEST_SKIP() << "No BF16 before Ampere.";
  }
  const std::string kHloTextRef = R"(
HloModule module, is_scheduled=true

triton_gemm_dot.5316 {
  parameter_1 = bf16[16,4,128]{2,1,0} parameter(1)
  bitcast.2 = bf16[16,512]{1,0} bitcast(parameter_1)
  parameter_0 = s8[512,96]{1,0} parameter(0)
  convert.4 = bf16[512,96]{1,0} convert(parameter_0)
  ROOT dot.0 = bf16[16,96]{1,0} dot(bitcast.2, convert.4),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

ENTRY entry {
  parameter_0.1 = s8[96,4,128]{2,1,0} parameter(0)
  bitcast.6 = s8[512,96]{1,0} bitcast(parameter_0.1)
  parameter_1.1 = bf16[16,4,128]{2,1,0} parameter(1)
  ROOT triton_gemm_dot.5316 = bf16[16,96]{1,0} fusion(bitcast.6, parameter_1.1),
    kind=kCustom, calls=triton_gemm_dot.5316,
    backend_config={kind: "__triton_gemm", triton_gemm_config: {"block_m":32,"block_n":32,"block_k":256,"split_k":1,"num_stages":1,"num_warps":4}}
})";

  const std::string kHloTextSplitK = R"(
HloModule module, is_scheduled=true

triton_gemm_dot.5316 {
  parameter_1 = bf16[16,4,128]{2,1,0} parameter(1)
  bitcast.2 = bf16[16,512]{1,0} bitcast(parameter_1)
  bitcast.17 = bf16[16,16,32]{2,1,0} bitcast(bitcast.2)
  parameter_0 = s8[512,96]{1,0} parameter(0)
  convert.4 = bf16[512,96]{1,0} convert(parameter_0)
  bitcast.18 = bf16[16,32,96]{2,1,0} bitcast(convert.4)
  ROOT dot.4 = bf16[16,16,96]{2,1,0} dot(bitcast.17, bitcast.18),
    lhs_batch_dims={1}, lhs_contracting_dims={2},
    rhs_batch_dims={0}, rhs_contracting_dims={1}
}

triton_gemm_dot.5316.reduce_sub_computation.clone {
  rhs.1 = f32[] parameter(1)
  lhs.1 = f32[] parameter(0)
  ROOT add.1 = f32[] add(lhs.1, rhs.1)
}

fused_computation {
  param_0.2 = bf16[16,16,96]{2,1,0} parameter(0)
  convert.19 = f32[16,16,96]{2,1,0} convert(param_0.2)
  constant_1 = bf16[] constant(0)
  convert.18 = f32[] convert(constant_1)
  reduce.1 = f32[16,96]{1,0} reduce(convert.19, convert.18),
    dimensions={0}, to_apply=triton_gemm_dot.5316.reduce_sub_computation.clone
  ROOT convert.17 = bf16[16,96]{1,0} convert(reduce.1)
}

ENTRY entry {
  parameter_0.1 = s8[96,4,128]{2,1,0} parameter(0)
  bitcast.6 = s8[512,96]{1,0} bitcast(parameter_0.1)
  parameter_1.1 = bf16[16,4,128]{2,1,0} parameter(1)
  triton_gemm_dot.5316 = bf16[16,16,96]{2,1,0} fusion(bitcast.6, parameter_1.1),
    kind=kCustom, calls=triton_gemm_dot.5316,
    backend_config={kind: "__triton_gemm", triton_gemm_config: {"block_m":64,"block_n":32,"block_k":32,"split_k":16,"num_stages":1,"num_warps":4}}
  ROOT fusion.1 = bf16[16,96]{1,0} fusion(triton_gemm_dot.5316),
    kind=kLoop, calls=fused_computation
})";

  EXPECT_TRUE(RunAndCompareTwoModules(kHloTextRef, kHloTextSplitK,
                                      ErrorSpec{/*aabs=*/2, /*arel=*/1e-2},
                                      /*run_hlo_passes=*/false));
}

TEST_F(CompareTest, NonMajorMostOutputBatchWorksCorrectly) {
  const std::string kHloTextTest = R"(
HloModule m

triton_gemm_dot.6 {
  parameter_1 = f32[32,50,104]{2,1,0} parameter(1)
  parameter_0 = s8[32,26,104]{2,1,0} parameter(0)
  convert.22 = f32[32,26,104]{2,1,0} convert(parameter_0)
  ROOT dot.127 = f32[32,50,26]{2,0,1} dot(parameter_1, convert.22),
    lhs_batch_dims={0}, lhs_contracting_dims={2},
    rhs_batch_dims={0}, rhs_contracting_dims={2}
}

ENTRY e {
  p0 = s8[32,26,104]{2,1,0} parameter(0)
  p1 = f32[32,50,104]{2,1,0} parameter(1)
  ROOT triton_gemm_dot.6 = f32[32,50,26]{2,0,1} fusion(p0, p1),
    kind=kCustom, calls=triton_gemm_dot.6,
    backend_config={kind: "__triton_gemm", triton_gemm_config: {"block_m":64,"block_n":16,"block_k":32,"split_k":1,"num_stages":1,"num_warps":4}}
})";

  const std::string kHloTextRef = R"(
HloModule m

%triton_gemm_dot.127 {
  %parameter_1.1 = f32[32,50,104]{2,1,0} parameter(1)
  %parameter_0.1 = s8[32,26,104]{2,1,0} parameter(0)
  %convert.0 = f32[32,26,104]{2,1,0} convert(%parameter_0.1)
  ROOT %dot.0 = f32[32,50,26]{2,1,0} dot(%parameter_1.1, %convert.0),
    lhs_batch_dims={0}, lhs_contracting_dims={2},
    rhs_batch_dims={0}, rhs_contracting_dims={2}
}

%fused_computation {
  %param_0.1 = f32[32,50,26]{2,1,0} parameter(0)
  %transpose.1 = f32[50,32,26]{2,1,0} transpose(%param_0.1), dimensions={1,0,2}
  ROOT %bitcast.7 = f32[32,50,26]{2,0,1} bitcast(%transpose.1)
}

ENTRY e {
  %parameter_0 = s8[32,26,104]{2,1,0} parameter(0)
  %parameter_1 = f32[32,50,104]{2,1,0} parameter(1)
  %triton_gemm_dot.127 = f32[32,50,26]{2,1,0} fusion(%parameter_0, %parameter_1),
    kind=kCustom, calls=%triton_gemm_dot.127,
    backend_config={kind: "__triton_gemm", triton_gemm_config: {"block_m":32,"block_n":128,"block_k":64,"split_k":1,"num_stages":2,"num_warps":4}}
  ROOT %fusion.1 = f32[32,50,26]{2,0,1} fusion(%triton_gemm_dot.127), kind=kLoop, calls=%fused_computation
})";

  EXPECT_TRUE(RunAndCompareTwoModules(kHloTextRef, kHloTextTest,
                                      ErrorSpec{/*aabs=*/1e-6, /*arel=*/1e-6},
                                      /*run_hlo_passes=*/false));
}

TEST_F(CompareTest, TritonDotFusionCanHaveOnlyRHSParameter) {
  const std::string kHloTextTest = R"(
HloModule m, is_scheduled=true

triton_gemm___computation {
  parameter_0 = f32[92,11]{1,0} parameter(0)
  c = f16[] constant(321)
  b = f16[11,63] broadcast(c)
  cc = f32[11,63] convert(b)
  ROOT _.1 = f32[63,92]{1,0} dot(cc, parameter_0),
    lhs_contracting_dims={0}, rhs_contracting_dims={1}
}

ENTRY e {
  p0 = f32[92,11]{1,0} parameter(0)
  ROOT triton_gemm__ = f32[63,92]{1,0} fusion(p0), kind=kCustom,
    calls=triton_gemm___computation,
    backend_config={"kind":"__triton_gemm",
                    "triton_gemm_config":{"block_m":"16","block_n":"64",
                                          "block_k":"16","split_k":"1",
                                          "num_stages":"3","num_warps":"2"}}
})";

  const std::string kHloTextRef = R"(
HloModule m, is_scheduled=true

ENTRY e {
  constant_2 = f32[] constant(321)
  parameter_0 = f32[92,11]{1,0} parameter(0)
  broadcast.2 = f32[11,63]{1,0} broadcast(constant_2), dimensions={}
  ROOT custom-call = f32[63,92]{1,0} custom-call(broadcast.2, parameter_0),
    custom_call_target="__cublas$gemm",
    backend_config={"alpha_real":1,"beta":0,"dot_dimension_numbers":{"lhs_contracting_dimensions":["0"],"rhs_contracting_dimensions":["1"],"lhs_batch_dimensions":[],"rhs_batch_dimensions":[]},"alpha_imag":0,"precision_config":{"operand_precision":["DEFAULT","DEFAULT"]},"epilogue":"DEFAULT"}
})";

  EXPECT_TRUE(RunAndCompareTwoModules(kHloTextRef, kHloTextTest,
                                      ErrorSpec{/*aabs=*/1e-2, /*arel=*/1e-2},
                                      /*run_hlo_passes=*/false));
}

TEST_F(CompareTest, TritonDotFusionCanHaveNoParametersAtAll) {
  const std::string kHloTextTest = R"(
HloModule m, is_scheduled=true

triton_gemm___computation {
  c = f32[] constant(123)
  b = f32[11,63] broadcast(c)
  c2 = f32[] constant(945)
  b2 = f32[63,45] broadcast(c2)
  ROOT _.1 = f32[11,45]{1,0} dot(b, b2),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

ENTRY e {
  ROOT triton_gemm__ = f32[11,45]{1,0} fusion(), kind=kCustom,
    calls=triton_gemm___computation,
    backend_config={"kind":"__triton_gemm",
                    "triton_gemm_config":{"block_m":"16","block_n":"64",
                                          "block_k":"16","split_k":"1",
                                          "num_stages":"3","num_warps":"2"}}
})";

  const std::string kHloTextRef = R"(
HloModule m, is_scheduled=true

ENTRY triton_gemm___computation {
  constant_1 = f32[] constant(945)
  constant = f32[] constant(123)
  broadcast = f32[11,63]{1,0} broadcast(constant), dimensions={}
  broadcast.1 = f32[63,45]{1,0} broadcast(constant_1), dimensions={}
  ROOT custom-call = f32[11,45]{1,0} custom-call(broadcast, broadcast.1),
    custom_call_target="__cublas$gemm",
    backend_config={"alpha_real":1,"beta":0,"dot_dimension_numbers":{"lhs_contracting_dimensions":["1"],"rhs_contracting_dimensions":["0"],"lhs_batch_dimensions":[],"rhs_batch_dimensions":[]},"alpha_imag":0,"precision_config":{"operand_precision":["DEFAULT","DEFAULT"]},"epilogue":"DEFAULT"}
})";

  EXPECT_TRUE(RunAndCompareTwoModules(kHloTextRef, kHloTextTest,
                                      ErrorSpec{/*aabs=*/1e-6, /*arel=*/1e-6},
                                      /*run_hlo_passes=*/false));
}

TEST_F(CompareTest, TritonDotFusionCanHaveManyParameters) {
  const std::string kHloTextTest = R"(
HloModule m

triton_gemm_dot_computation {
  tmp_1 = pred[3,32]{1,0} parameter(0)
  tmp_2 = f32[3,32]{1,0} parameter(1)
  tmp_3 = f32[3,32]{1,0} parameter(2)
  tmp_4 = f32[3,32]{1,0} select(tmp_1, tmp_2, tmp_3)
  tmp_5 = f32[3,32]{1,0} parameter(3)
  tmp_6 = f32[3,32]{1,0} multiply(tmp_4, tmp_5)
  tmp_7 = f32[3,32]{1,0} parameter(4)
  tmp_8 = f32[3,32]{1,0} maximum(tmp_6, tmp_7)
  tmp_9 = f32[3,57]{1,0} parameter(9)
  tmp_10 = f32[3,57]{1,0} parameter(10)
  tmp_11 = f32[3,57]{1,0} multiply(tmp_9, tmp_10)
  tmp_12 = f32[3,57]{1,0} parameter(11)
  tmp_13 = f32[3,57]{1,0} add(tmp_11, tmp_12)
  tmp_14 = pred[3,57]{1,0} parameter(5)
  tmp_15 = f32[3,57]{1,0} parameter(6)
  tmp_16 = f32[3,57]{1,0} parameter(7)
  tmp_17 = f32[3,57]{1,0} select(tmp_14, tmp_15, tmp_16)
  tmp_18 = f32[3,57]{1,0} parameter(8)
  tmp_19 = f32[3,57]{1,0} multiply(tmp_17, tmp_18)
  tmp_20 = f32[3,57]{1,0} negate(tmp_19)
  tmp_21 = f32[3,57]{1,0} add(tmp_13, tmp_20)
  ROOT tmp_22 = f32[32,57]{0,1} dot(tmp_8, tmp_21), lhs_contracting_dims={0}, rhs_contracting_dims={0}
}

ENTRY e {
  tmp_1 = pred[3,32]{1,0} parameter(0)
  tmp_2 = f32[3,32]{1,0} parameter(1)
  tmp_3 = f32[3,32]{1,0} parameter(2)
  tmp_5 = f32[3,32]{1,0} parameter(3)
  tmp_7 = f32[3,32]{1,0} parameter(4)
  tmp_14 = pred[3,57]{1,0} parameter(5)
  tmp_15 = f32[3,57]{1,0} parameter(6)
  tmp_16 = f32[3,57]{1,0} parameter(7)
  tmp_18 = f32[3,57]{1,0} parameter(8)
  tmp_9 = f32[3,57]{1,0} parameter(9)
  tmp_10 = f32[3,57]{1,0} parameter(10)
  tmp_12 = f32[3,57]{1,0} parameter(11)
  ROOT r = f32[32,57]{0,1} fusion(tmp_1, tmp_2, tmp_3, tmp_5, tmp_7, tmp_14, tmp_15, tmp_16, tmp_18, tmp_9, tmp_10, tmp_12), kind=kCustom,
    calls=triton_gemm_dot_computation,
    backend_config={"kind":"__triton_gemm",
                    "triton_gemm_config":{"block_m":"64","block_n":"64",
                                          "block_k":"64","split_k":"1",
                                          "num_stages":"1","num_warps":"4"}}
})";

  const std::string kHloTextRef = R"(
HloModule m

fused_computation {
  param_5.1 = f32[3,57]{1,0} parameter(5)
  param_6 = f32[3,57]{1,0} parameter(6)
  multiply.4 = f32[3,57]{1,0} multiply(param_5.1, param_6)
  param_4.2 = f32[3,57]{1,0} parameter(4)
  add.3 = f32[3,57]{1,0} add(multiply.4, param_4.2)
  param_1.4 = pred[3,57]{1,0} parameter(1)
  param_2.2 = f32[3,57]{1,0} parameter(2)
  param_3.1 = f32[3,57]{1,0} parameter(3)
  select.2 = f32[3,57]{1,0} select(param_1.4, param_2.2, param_3.1)
  param_0.1 = f32[3,57]{1,0} parameter(0)
  multiply.3 = f32[3,57]{1,0} multiply(select.2, param_0.1)
  negate.1 = f32[3,57]{1,0} negate(multiply.3)
  ROOT add.2 = f32[3,57]{1,0} add(add.3, negate.1)
}

fused_computation.1 {
  param_2.4 = pred[3,32]{1,0} parameter(2)
  param_3.2 = f32[3,32]{1,0} parameter(3)
  param_4.3 = f32[3,32]{1,0} parameter(4)
  select.3 = f32[3,32]{1,0} select(param_2.4, param_3.2, param_4.3)
  param_1.7 = f32[3,32]{1,0} parameter(1)
  multiply.5 = f32[3,32]{1,0} multiply(select.3, param_1.7)
  param_0.3 = f32[3,32]{1,0} parameter(0)
  ROOT maximum.1 = f32[3,32]{1,0} maximum(multiply.5, param_0.3)
}

ENTRY e {
  tmp_18 = f32[3,57]{1,0} parameter(8)
  tmp_16 = f32[3,57]{1,0} parameter(7)
  tmp_15 = f32[3,57]{1,0} parameter(6)
  tmp_14 = pred[3,57]{1,0} parameter(5)
  tmp_12 = f32[3,57]{1,0} parameter(11)
  tmp_10 = f32[3,57]{1,0} parameter(10)
  tmp_9 = f32[3,57]{1,0} parameter(9)
  tmp_7 = f32[3,32]{1,0} parameter(4)
  tmp_5 = f32[3,32]{1,0} parameter(3)
  tmp_3 = f32[3,32]{1,0} parameter(2)
  tmp_2 = f32[3,32]{1,0} parameter(1)
  tmp_1 = pred[3,32]{1,0} parameter(0)
  fusion.1 = f32[3,32]{1,0} fusion(tmp_7, tmp_5, tmp_1, tmp_2, tmp_3), kind=kLoop, calls=fused_computation.1
  fusion = f32[3,57]{1,0} fusion(tmp_18, tmp_14, tmp_15, tmp_16, tmp_12, /*index=5*/tmp_9, tmp_10), kind=kLoop, calls=fused_computation
  ROOT custom-call = f32[32,57]{0,1} custom-call(fusion.1, fusion),
    custom_call_target="__cublas$gemm",
    backend_config={"alpha_real":1,"beta":0,"dot_dimension_numbers":{"lhs_contracting_dimensions":["0"],"rhs_contracting_dimensions":["0"],"lhs_batch_dimensions":[],"rhs_batch_dimensions":[]},"alpha_imag":0,"precision_config":{"operand_precision":["DEFAULT","DEFAULT"]},"epilogue":"DEFAULT"}
})";

  EXPECT_TRUE(RunAndCompareTwoModules(kHloTextRef, kHloTextTest,
                                      ErrorSpec{/*aabs=*/1e-4, /*arel=*/1e-4},
                                      /*run_hlo_passes=*/false));
}

TEST_F(CompareTest, PredToBF16ConversionWorks) {
  if (!GetCudaComputeCapability().IsAtLeast(
          se::CudaComputeCapability::AMPERE)) {
    GTEST_SKIP() << "No BF16 before Ampere.";
  }
  const std::string kHloTextTest = R"(
HloModule m, is_scheduled=true

triton_gemm_computation {
  parameter_0 = bf16[92,11]{1,0} parameter(0)
  parameter_1 = s32[11,63]{1,0} parameter(1)
  parameter_2 = s32[11,63]{1,0} parameter(2)
  f1.1 = pred[11,63]{1,0} compare(parameter_1, parameter_2), direction=GE
  c.1 = bf16[11,63]{1,0} convert(f1.1)
  ROOT _.1 = bf16[92,63]{1,0} dot(parameter_0, c.1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

ENTRY e {
  p0 = bf16[92,11]{1,0} parameter(0)
  p1 = s32[11,63]{1,0} parameter(1)
  p2 = s32[11,63]{1,0} parameter(2)
  ROOT triton_gemm__ = bf16[92,63]{1,0} fusion(p0, p1, p2), kind=kCustom,
    calls=triton_gemm_computation,
    backend_config={"kind":"__triton_gemm",
                    "triton_gemm_config":{"block_m":"32","block_n":"16",
                                          "block_k":"32","split_k":"1",
                                          "num_stages":"1","num_warps":"4"}}
})";

  const std::string kHloTextRef = R"(
HloModule m, is_scheduled=true

fused_computation {
  p0 = s32[11,63]{1,0} parameter(0)
  p1 = s32[11,63]{1,0} parameter(1)
  f.1 = pred[11,63]{1,0} compare(p0, p1), direction=GE
  ROOT convert.1 = bf16[11,63]{1,0} convert(f.1)
}

ENTRY e {
  p2 = s32[11,63]{1,0} parameter(2)
  p1 = s32[11,63]{1,0} parameter(1)
  p0 = bf16[92,11]{1,0} parameter(0)
  fusion = bf16[11,63]{1,0} fusion(p1, p2), kind=kLoop, calls=fused_computation
  ROOT custom-call = bf16[92,63]{1,0} custom-call(p0, fusion),
    custom_call_target="__cublas$gemm",
    backend_config={"alpha_real":1,"beta":0,"dot_dimension_numbers":
      {"lhs_contracting_dimensions":["1"],"rhs_contracting_dimensions":["0"],
      "lhs_batch_dimensions":[],"rhs_batch_dimensions":[]},
      "alpha_imag":0,"precision_config":
      {"operand_precision":["DEFAULT","DEFAULT"]},"epilogue":"DEFAULT"}
})";

  EXPECT_TRUE(RunAndCompareTwoModules(kHloTextRef, kHloTextTest,
                                      ErrorSpec{/*aabs=*/1e-6, /*arel=*/1e-6},
                                      /*run_hlo_passes=*/false));
}

class TritonSoftmaxTest : public GpuCodegenTest {
 public:
  DebugOptions GetDebugOptionsForTest() override {
    DebugOptions debug_options = GpuCodegenTest::GetDebugOptionsForTest();
    debug_options.set_xla_gpu_enable_triton_softmax_fusion(true);
    return debug_options;
  }

  se::CudaComputeCapability GetCudaComputeCapability() {
    return backend()
        .default_stream_executor()
        ->GetDeviceDescription()
        .cuda_compute_capability();
  }
};

TEST_F(TritonSoftmaxTest, CanFuseAndEmitExactSoftmaxF32) {
  const std::string hlo_text = R"(
HloModule softmax
max_computation {
  arg_0 = f32[] parameter(0)
  arg_1 = f32[] parameter(1)
  ROOT maximum = f32[] maximum(arg_0, arg_1)
}
add_computation {
  arg_0.1 = f32[] parameter(0)
  arg_1.1 = f32[] parameter(1)
  ROOT add = f32[] add(arg_0.1, arg_1.1)
}
ENTRY main {
  param_0 = f32[127,125]{1,0} parameter(0)
  constant_neg_inf = f32[] constant(-inf)
  reduce = f32[127]{0} reduce(param_0, constant_neg_inf), dimensions={1}, to_apply=max_computation
  broadcast = f32[127,125]{1,0} broadcast(reduce), dimensions={0}
  subtract = f32[127,125]{1,0} subtract(param_0, broadcast)
  exponential = f32[127,125]{1,0} exponential(subtract)
  constant_zero = f32[] constant(0)
  second_reduce = f32[127]{0} reduce(exponential, constant_zero), dimensions={1}, to_apply=add_computation
  second_broadcast = f32[127,125]{1,0} broadcast(second_reduce), dimensions={0}
  ROOT divide = f32[127,125]{1,0} divide(exponential, second_broadcast)
}
)";

  MatchOptimizedHlo(hlo_text, R"(
; CHECK:    ENTRY
; CHECK:      %[[P0:.*]] = f32[127,125]{1,0} parameter(0)
; CHECK:      ROOT
; CHECK-SAME: fusion(%[[P0]])
; CHECK-SAME:   kind=kCustom
; CHECK-SAME:   __triton_softmax
)");
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec(1e-6, 1e-6)));
}


TEST_F(TritonSoftmaxTest, CanFuseAndEmitFirstSoftmaxDiamondF16) {
  const std::string hlo_text = R"(
HloModule softmax
max_computation {
  arg_0 = f16[] parameter(0)
  arg_1 = f16[] parameter(1)
  ROOT maximum = f16[] maximum(arg_0, arg_1)
}
add_computation {
  arg_0.1 = f16[] parameter(0)
  arg_1.1 = f16[] parameter(1)
  ROOT add = f16[] add(arg_0.1, arg_1.1)
}
ENTRY main {
  param_0 = f16[127,125]{1,0} parameter(0)
  constant_neg_inf = f16[] constant(-inf)
  reduce = f16[127]{0} reduce(param_0, constant_neg_inf), dimensions={1}, to_apply=max_computation
  broadcast = f16[127,125]{1,0} broadcast(reduce), dimensions={0}
  ROOT subtract = f16[127,125]{1,0} subtract(param_0, broadcast)
}
)";

  MatchOptimizedHlo(hlo_text, R"(
; CHECK:    ENTRY
; CHECK:      %[[P0:.*]] = f16[127,125]{1,0} parameter(0)
; CHECK:      ROOT
; CHECK-SAME: fusion(%[[P0]])
; CHECK-SAME:   kind=kCustom
; CHECK-SAME:   __triton_softmax
)");
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec(2e-3, 1e-5)));
}

TEST_F(
    TritonSoftmaxTest,
    CanFuseAndEmitSoftmaxWithBatchDimMergingAndSplittingBitcastsOnEveryEdge) {
  const std::string hlo_text = R"(
HloModule softmax
max_computation {
  arg_0 = f32[] parameter(0)
  arg_1 = f32[] parameter(1)
  ROOT maximum = f32[] maximum(arg_0, arg_1)
}
add_computation {
  arg_0.1 = f32[] parameter(0)
  arg_1.1 = f32[] parameter(1)
  ROOT add = f32[] add(arg_0.1, arg_1.1)
}

ENTRY main {
  param_0 = f32[2,65,125] parameter(0)
  bitcasted_param_0 = f32[65,2,125] reshape(param_0)
  constant_neg_inf = f32[] constant(-inf)
  reduce = f32[65,2]{1,0} reduce(bitcasted_param_0, constant_neg_inf), dimensions={2}, to_apply=max_computation
  bitcasted_reduce = f32[130] reshape(reduce)
  broadcast = f32[130,125]{1,0} broadcast(bitcasted_reduce), dimensions={0}
  bitcasted_broadcast = f32[65,2,125] reshape(broadcast)
  subtract = f32[65,2,125]{2,1,0} subtract(bitcasted_param_0, bitcasted_broadcast)
  bitcasted_subtract = f32[130,125] reshape(subtract)
  exponential = f32[130,125]{1,0} exponential(bitcasted_subtract)
  constant_zero = f32[] constant(0)
  bitcasted_exponential = f32[2,65,125] reshape(exponential)
  second_reduce = f32[2,65]{1,0} reduce(bitcasted_exponential, constant_zero), dimensions={2}, to_apply=add_computation
  second_bitcasted_reduce = f32[130] reshape(second_reduce)
  second_broadcast = f32[130,125]{1,0} broadcast(second_bitcasted_reduce), dimensions={0}
  second_bitcasted_broadcast = f32[2,65,125] reshape(second_broadcast)
  ROOT divide = f32[2,65,125]{2,1,0} divide(bitcasted_exponential, second_bitcasted_broadcast)
})";

  MatchOptimizedHlo(hlo_text, R"(
; CHECK:    ENTRY
; CHECK:      %[[P0:.*]] = f32[2,65,125]{2,1,0} parameter(0)
; CHECK:      ROOT
; CHECK-SAME: fusion(%[[P0]])
; CHECK-SAME:   kind=kCustom
; CHECK-SAME:   __triton_softmax
)");
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec(1e-6, 1e-6)));
}

TEST_F(TritonSoftmaxTest,
       CanFuseAndEmitDiamondWithMultipleBroadcastDimensions) {
  const std::string hlo_text = R"(
HloModule softmax
max_computation {
  arg_0 = f32[] parameter(0)
  arg_1 = f32[] parameter(1)
  ROOT maximum = f32[] maximum(arg_0, arg_1)
}
ENTRY main {
  param_0 = f32[1,3,125,125]{3,2,1,0} parameter(0)
  reshape = f32[3,125,125]{2,1,0} reshape(f32[1,3,125,125]{3,2,1,0} param_0)
  constant_neg_inf = f32[] constant(-inf)
  reduce = f32[3,125]{1,0} reduce(f32[3,125,125]{2,1,0} reshape, f32[] constant_neg_inf), dimensions={2}, to_apply=max_computation
  broadcast = f32[1,3,125,125]{3,2,1,0} broadcast(f32[3,125]{1,0} reduce), dimensions={1,2}
  ROOT subtract = f32[1,3,125,125]{3,2,1,0} subtract(f32[1,3,125,125]{3,2,1,0} param_0, f32[1,3,125,125]{3,2,1,0} broadcast)
})";

  MatchOptimizedHlo(hlo_text, R"(
; CHECK:    ENTRY
; CHECK:      %[[P0:.*]] = f32[1,3,125,125]{3,2,1,0} parameter(0)
; CHECK:      ROOT
; CHECK-SAME: fusion(%[[P0]])
; CHECK-SAME:   kind=kCustom
; CHECK-SAME:   __triton_softmax
)");
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec(1e-6, 1e-6)));
}

TEST_F(TritonSoftmaxTest,
       CanFuseAndEmitSoftmaxWithIntermediateUnaryElementwise) {
  const std::string hlo_text = R"(
HloModule softmax
max_computation {
  arg_0 = f32[] parameter(0)
  arg_1 = f32[] parameter(1)
  ROOT maximum = f32[] maximum(arg_0, arg_1)
}
add_computation {
  arg_0.1 = f32[] parameter(0)
  arg_1.1 = f32[] parameter(1)
  ROOT add = f32[] add(arg_0.1, arg_1.1)
}
ENTRY main {
  param_0 = f32[127,125]{1,0} parameter(0)
  constant_neg_inf = f32[] constant(-inf)
  reduce = f32[127]{0} reduce(param_0, constant_neg_inf), dimensions={1}, to_apply=max_computation
  broadcast = f32[127,125]{1,0} broadcast(reduce), dimensions={0}
  subtract = f32[127,125]{1,0} subtract(param_0, broadcast)
  abs = f32[127,125]{1,0} abs(subtract)
  exponential = f32[127,125]{1,0} exponential(abs)
  constant_zero = f32[] constant(0)
  second_reduce = f32[127]{0} reduce(exponential, constant_zero), dimensions={1}, to_apply=add_computation
  second_broadcast = f32[127,125]{1,0} broadcast(second_reduce), dimensions={0}
  ROOT divide = f32[127,125]{1,0} divide(exponential, second_broadcast)
}
)";

  MatchOptimizedHlo(hlo_text, R"(
; CHECK:    ENTRY
; CHECK:      %[[P0:.*]] = f32[127,125]{1,0} parameter(0)
; CHECK:      ROOT
; CHECK-SAME: fusion(%[[P0]])
; CHECK-SAME:   kind=kCustom
; CHECK-SAME:   __triton_softmax
)");
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec(1e-6, 1e-6)));
}

TEST_F(
    TritonSoftmaxTest,
    CanFuseAndEmitTwoDiamondsWithSecondDiamondProducerEqualToFirstDiamondRoot) {  // NOLINT(whitespace/line_length)
  const std::string hlo_text = R"(
HloModule softmax
max_computation {
  arg_0 = f32[] parameter(0)
  arg_1 = f32[] parameter(1)
  ROOT maximum = f32[] maximum(arg_0, arg_1)
}
add_computation {
  arg_0.1 = f32[] parameter(0)
  arg_1.1 = f32[] parameter(1)
  ROOT add = f32[] add(arg_0.1, arg_1.1)
}
ENTRY main {
  param_0 = f32[127,125]{1,0} parameter(0)
  constant_neg_inf = f32[] constant(-inf)
  reduce = f32[127]{0} reduce(param_0, constant_neg_inf), dimensions={1}, to_apply=max_computation
  broadcast = f32[127,125]{1,0} broadcast(reduce), dimensions={0}
  subtract = f32[127,125]{1,0} subtract(param_0, broadcast)
  constant_zero = f32[] constant(0)
  second_reduce = f32[127]{0} reduce(subtract, constant_zero), dimensions={1}, to_apply=add_computation
  second_broadcast = f32[127,125]{1,0} broadcast(second_reduce), dimensions={0}
  ROOT divide = f32[127,125]{1,0} divide(subtract, second_broadcast)
}
)";

  MatchOptimizedHlo(hlo_text, R"(
; CHECK:    ENTRY
; CHECK:      %[[P0:.*]] = f32[127,125]{1,0} parameter(0)
; CHECK:      ROOT
; CHECK-SAME: fusion(%[[P0]])
; CHECK-SAME:   kind=kCustom
; CHECK-SAME:   __triton_softmax
)");
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec(1e-6, 1e-6)));
}

TEST_F(TritonSoftmaxTest,
       CanFuseAndEmitDiamondWithTrailingUnaryElementwiseAtTheRoot) {
  const std::string hlo_text = R"(
HloModule softmax
max_computation {
  arg_0 = f32[] parameter(0)
  arg_1 = f32[] parameter(1)
  ROOT maximum = f32[] maximum(arg_0, arg_1)
}
ENTRY main {
  param_0 = f32[127,125]{1,0} parameter(0)
  constant_neg_inf = f32[] constant(-inf)
  reduce = f32[127]{0} reduce(param_0, constant_neg_inf), dimensions={1}, to_apply=max_computation
  broadcast = f32[127,125]{1,0} broadcast(reduce), dimensions={0}
  subtract = f32[127,125]{1,0} subtract(param_0, broadcast)
  ROOT abs = f32[127,125]{1,0} abs(subtract)
}
)";

  MatchOptimizedHlo(hlo_text, R"(
; CHECK:    ENTRY
; CHECK:      %[[P0:.*]] = f32[127,125]{1,0} parameter(0)
; CHECK:      ROOT
; CHECK-SAME: fusion(%[[P0]])
; CHECK-SAME:   kind=kCustom
; CHECK-SAME:   __triton_softmax
)");
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec(1e-6, 1e-6)));
}

TEST_F(TritonSoftmaxTest, CanFuseAndEmitDiamondWithUnaryElementwisePrefix) {
  const std::string hlo_text = R"(
HloModule softmax
max_computation {
  arg_0 = f32[] parameter(0)
  arg_1 = f32[] parameter(1)
  ROOT maximum = f32[] maximum(arg_0, arg_1)
}
ENTRY main {
  param_0 = f32[127,125]{1,0} parameter(0)
  abs = f32[127,125]{1,0} abs(param_0)
  constant_neg_inf = f32[] constant(-inf)
  reduce = f32[127]{0} reduce(abs, constant_neg_inf), dimensions={1}, to_apply=max_computation
  broadcast = f32[127,125]{1,0} broadcast(reduce), dimensions={0}
  ROOT subtract = f32[127,125]{1,0} subtract(param_0, broadcast)
}
)";

  MatchOptimizedHlo(hlo_text, R"(
; CHECK:    ENTRY
; CHECK:      %[[P0:.*]] = f32[127,125]{1,0} parameter(0)
; CHECK:      ROOT
; CHECK-SAME: fusion(%[[P0]])
; CHECK-SAME:   kind=kCustom
; CHECK-SAME:   __triton_softmax
)");
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec(1e-6, 1e-6)));
}

TEST_F(TritonSoftmaxTest,
       CanFuseAndEmitSoftmaxDiamondWithLastDimensionBitcastAfterReduce) {
  const std::string hlo_text = R"(
HloModule softmax
max_computation {
  arg_0 = f32[] parameter(0)
  arg_1 = f32[] parameter(1)
  ROOT maximum = f32[] maximum(arg_0, arg_1)
}

ENTRY main {
  param_0 = f32[3,127,125]{2,1,0} parameter(0)
  constant_neg_inf = f32[] constant(-inf)
  reduce = f32[3,127]{1,0} reduce(param_0, constant_neg_inf), dimensions={2}, to_apply=max_computation
  bitcasted_reduce = f32[381]{0} reshape(reduce)
  broadcast = f32[381,125]{1,0} broadcast(bitcasted_reduce), dimensions={0}
  bitcasted_broadcast = f32[3,127,125]{2,1,0} reshape(broadcast)
  ROOT subtract = f32[3,127,125]{2,1,0} subtract(param_0, bitcasted_broadcast)
}
)";

  MatchOptimizedHlo(hlo_text, R"(
; CHECK:    ENTRY
; CHECK:      %[[P0:.*]] = f32[3,127,125]{2,1,0} parameter(0)
; CHECK:      ROOT
; CHECK-SAME: fusion(%[[P0]])
; CHECK-SAME:   kind=kCustom
; CHECK-SAME:   __triton_softmax
)");
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec(1e-6, 1e-6)));
}

TEST_F(
    TritonSoftmaxTest,
    CanFuseAndEmitConvertInvolvingBF16InputIntoSoftmaxDiamondCorrectlyForAmpereAndVoltaComputeCapability) {  // NOLINT(whitespace/line_length)
  const std::string hlo_text = R"(
HloModule softmax
max_computation {
  arg_0 = f32[] parameter(0)
  arg_1 = f32[] parameter(1)
  ROOT maximum = f32[] maximum(arg_0, arg_1)
}
ENTRY main {
  param_0 = bf16[127,125]{1,0} parameter(0)
  param_0_f32 = f32[127,125]{1,0} convert(param_0)
  constant_neg_inf = f32[] constant(-inf)
  reduce = f32[127]{0} reduce(param_0_f32, constant_neg_inf), dimensions={1}, to_apply=max_computation
  broadcast = f32[127,125]{1,0} broadcast(reduce), dimensions={0}
  ROOT subtract = f32[127,125]{1,0} subtract(param_0_f32, broadcast)
}
)";

  if (GetCudaComputeCapability().IsAtLeast(se::CudaComputeCapability::AMPERE)) {
    MatchOptimizedHlo(hlo_text, R"(
; CHECK:    ENTRY
; CHECK:      %[[P0:.*]] = bf16[127,125]{1,0} parameter(0)
; CHECK:      ROOT
; CHECK-SAME: fusion(%[[P0]])
; CHECK-SAME:   kind=kCustom
; CHECK-SAME:   __triton_softmax
)");
  } else {
    MatchOptimizedHlo(hlo_text, R"(
; CHECK:    ENTRY
; CHECK:      %[[P0:.*]] = bf16[127,125]{1,0} parameter(0)
; CHECK:      %[[CONVERT:.*]] = f32[127,125]{1,0} convert(%[[P0]])
; CHECK:      ROOT
; CHECK-SAME: fusion(%[[CONVERT]])
; CHECK-SAME:   kind=kCustom
; CHECK-SAME:   __triton_softmax
)");
  }
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec(1e-6, 1e-6)));
}

}  // namespace
}  // namespace gpu
}  // namespace xla
