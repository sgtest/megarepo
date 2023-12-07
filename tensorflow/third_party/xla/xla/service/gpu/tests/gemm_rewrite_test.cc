/* Copyright 2019 The TensorFlow Authors. All Rights Reserved.

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

#include <string>
#include <tuple>
#include <utility>
#include <vector>

#include <gtest/gtest.h>
#include "absl/functional/any_invocable.h"
#include "absl/strings/str_replace.h"
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/hlo/ir/hlo_module.h"
#include "xla/service/gpu/gemm_rewriter.h"
#include "xla/service/gpu/gpu_executable.h"
#include "xla/service/gpu/tests/gpu_codegen_test.h"
#include "xla/service/hlo_module_config.h"
#include "xla/service/pattern_matcher.h"
#include "xla/service/pattern_matcher_gmock.h"
#include "xla/statusor.h"
#include "xla/test.h"
#include "xla/tests/filecheck.h"
#include "xla/xla.pb.h"
#include "tsl/lib/core/status_test_util.h"

#if GOOGLE_CUDA
#include "third_party/gpus/cuda/include/cuda.h"
#endif

namespace xla {
namespace gpu {

namespace {

template <class... Ts>
struct Overload : Ts... {
  using Ts::operator()...;
};
template <class... Ts>
Overload(Ts...) -> Overload<Ts...>;

namespace m = ::xla::match;

class GemmRewriteTest : public GpuCodegenTest {
  const auto& device_desc() {
    return backend().default_stream_executor()->GetDeviceDescription();
  }

 public:
  const se::GpuComputeCapability& GpuComputeComp() {
    return device_desc().gpu_compute_capability();
  }
  se::GpuComputeCapability CudaHopperOrRocm() {
#if GOOGLE_CUDA
    return se::CudaComputeCapability{se::CudaComputeCapability::HOPPER, 0};
#elif TENSORFLOW_USE_ROCM
    return device_desc().rocm_compute_capability();
#endif
  }

  enum class Switch : uint32_t {
    False,  // check always fails
    True,   // check always succeeds
  };
  // switch based on architecture only
  bool CudaOrRocmCheck(Switch cuda_set, Switch rocm_set) {
    return std::visit(
        Overload{[cuda_set](const se::CudaComputeCapability&) {
                   return cuda_set == Switch::False ? false : true;
                 },
                 [rocm_set](const se::RocmComputeCapability&) {
                   return rocm_set == Switch::False ? false : true;
                 }},
        GpuComputeComp());
  }
  // major version check for CUDA and true/false for rocm
  bool CudaOrRocmCheck(int cuda_major, Switch rocm_set) {
    return CudaOrRocmCheck(cuda_major, 0, rocm_set);
  }
  // full version check for CUDA and true/false for rocm
  bool CudaOrRocmCheck(int cuda_major, int cuda_minor, Switch rocm_set) {
    return std::visit(
        Overload{
            [cuda_major, cuda_minor](const se::CudaComputeCapability& cc) {
              return cc.IsAtLeast(cuda_major, cuda_minor);
            },
            [rocm_set](const se::RocmComputeCapability&) {
              return rocm_set == Switch::False ? false : true;
            },
        },
        GpuComputeComp());
  }
  // most generic check: passes if NULL function is specified
  bool CudaOrRocmCheck(
      absl::AnyInvocable<bool(const se::CudaComputeCapability&)> cuda_fun,
      absl::AnyInvocable<bool(const se::RocmComputeCapability&)> rocm_fun) {
    return std::visit(
        Overload{[&cuda_fun](const se::CudaComputeCapability& cc) {
                   return (cuda_fun ? cuda_fun(cc) : true);
                 },
                 [&rocm_fun](const se::RocmComputeCapability& cc) {
                   return (rocm_fun ? rocm_fun(cc) : true);
                 }},
        GpuComputeComp());
  }

  DebugOptions GetDebugOptionsForTest() override {
    DebugOptions debug_options = GpuCodegenTest::GetDebugOptionsForTest();
    // These tests test the cuBLAS rewriter so we have to make sure that we use
    // cuBLAS for them.
    debug_options.set_xla_gpu_enable_triton_gemm(false);
    return debug_options;
  }

  bool SkipGpuBlasLtTest() {
    return CudaOrRocmCheck(
        [](se::CudaComputeCapability) {  // never skip gpublas-lt tests for CUDA
          return false;
        },
        [this](se::RocmComputeCapability rocm) {
          bool blaslt = GetDebugOptionsForTest().xla_gpu_enable_cublaslt();
          return (blaslt && !rocm.has_hipblaslt());
        });
  }
};

TEST_F(GemmRewriteTest, CheckCustomCallTarget) {
  if (SkipGpuBlasLtTest()) {
    GTEST_SKIP() << "BlasLt is not supported on this GPU architecture";
  }

  const char* hlo_text = R"(
HloModule SimpleGemm

ENTRY AddDotsFunc {
  x = f32[2,3] parameter(0)
  y = f32[3,4] parameter(1)
  ROOT dot_a = f32[2,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

)";
  DebugOptions debug_options = GetDebugOptionsForTest();
  if (debug_options.xla_gpu_enable_cublaslt()) {
    MatchOptimizedHlo(hlo_text,
                      R"(; CHECK: custom_call_target="__cublas$lt$matmul")");
  } else {
    MatchOptimizedHlo(hlo_text,
                      R"(; CHECK: custom_call_target="__cublas$gemm")");
  }
}

#if GOOGLE_CUDA || TENSORFLOW_USE_ROCM
TEST_F(GemmRewriteTest, TestBatchedAutotuning) {
  if (CudaOrRocmCheck(se::CudaComputeCapability::AMPERE, Switch::False)) {
    GTEST_SKIP()
        << "There is no autotuning starting with the Nvidia Ampere generation";
  }
  const char* hlo_text = R"(
HloModule ComplexDotMultipleNonContracting

ENTRY %test {
  %lhs = f32[7,17,10,13]{3,2,1,0} parameter(0)
  %rhs = f32[7,9,10,13,6]{4,3,2,1,0} parameter(1)
  ROOT %dot = f32[10,7,17,9,6]{4,3,2,1,0} dot(%lhs, %rhs), lhs_batch_dims={2,0}, rhs_batch_dims={2,0}, lhs_contracting_dims={3}, rhs_contracting_dims={3}
}

)";

  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK: selected_algorithm
      )");
}
#endif

TEST_F(GemmRewriteTest, SimpleRewriteDeterministic) {
  if (SkipGpuBlasLtTest()) {
    GTEST_SKIP() << "BlasLt is not supported on this GPU architecture";
  }

  const char* hlo_text = R"(
HloModule SimpleGemm

ENTRY AddDotsFunc {
  x = f32[128,128] parameter(0)
  y = f32[128,128] parameter(1)
  ROOT dot_a = f32[128,128] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
}
)";

  ErrorSpec error_spec = [&] {
    DebugOptions debug_options = GetDebugOptionsForTest();
    if (debug_options.xla_gpu_enable_cublaslt()) {
      return ErrorSpec{1e-3, 1e-3};
    } else {
      return ErrorSpec{1e-3, 1e-3};
    }
  }();

  auto get_module = [&]() {
    HloModuleConfig config;
    DebugOptions debug_options = GetDebugOptionsForTest();
    debug_options.set_xla_gpu_deterministic_ops(true);
    config.set_debug_options(debug_options);
    return ParseAndReturnVerifiedModule(hlo_text, config);
  };

  TF_ASSERT_OK_AND_ASSIGN(
      std::unique_ptr<HloModule> optimized_module,
      backend().compiler()->RunHloPasses(
          *get_module(), backend().default_stream_executor(),
          backend().default_stream_executor()->GetAllocator()));

  StatusOr<bool> filecheck_result = RunFileCheck(optimized_module->ToString(),
                                                 R"(
; CHECK:    custom_call_target="__cublas${{(lt\$matmul|gemm)}}"
    )");
  TF_ASSERT_OK(filecheck_result.status());
  EXPECT_TRUE(filecheck_result.value());
  EXPECT_TRUE(RunAndCompare(*get_module(), error_spec));
}

TEST_F(GemmRewriteTest, BF16GemmCodeGen) {
  const char* hlo_text = R"(
HloModule bf16codegendgemm

ENTRY bf16gemm {
  %parameter.1 = bf16[3]{0} parameter(0)
  %parameter.2 = bf16[3]{0} parameter(1)
  ROOT %dot.3 = bf16[] dot(bf16[3]{0} %parameter.1, bf16[3]{0} %parameter.2), lhs_contracting_dims={0}, rhs_contracting_dims={0}, operand_precision={highest,highest}
}
  )";

  MatchOptimizedHlo(hlo_text, R"(
; CHECK:  [[P1:%[^ ]+]] = bf16[3]{0} parameter(1)
; CHECK:  [[INSTR_1:%[^ ]+]] = f32[3]{0} convert([[P1]])
; CHECK:  [[P0:%[^ ]+]] = bf16[3]{0} parameter(0)
; CHECK:  [[INSTR_3:%[^ ]+]] = f32[3]{0} convert([[P0]])
; CHECK:  [[INSTR_4:%[^ ]+]] = f32[3]{0} multiply([[INSTR_1]], [[INSTR_3]])
; CHECK:  [[INSTR_5:%[^ ]+]] = f32[] constant(0)
; CHECK:  [[INSTR_6:%[^ ]+]] = f32[] reduce([[INSTR_4]], [[INSTR_5]]), dimensions={0}, to_apply=[[INSTR_7:%[^ ]+]]
; CHECK:  ROOT [[INSTR_8:%[^ ]+]] = bf16[] convert([[INSTR_6]])
  )");

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
}

TEST_F(GemmRewriteTest, BF16Transpose) {
  const char* hlo_text = R"(
HloModule broadcast

ENTRY broadcast {
  p = bf16[9] parameter(0)
  ROOT out = bf16[1,9] broadcast(p), dimensions={1}
}
)";

  MatchOptimizedHlo(hlo_text, R"(
; CHECK: bf16[1,9]{1,0} bitcast
; CHECK: bf16[1,9]{1,0} copy
)");

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
}

#if GOOGLE_CUDA || TENSORFLOW_USE_ROCM
// A test fixture class for tests which should have similar results with legacy
// cublas and cublasLt
class ParameterizedGemmRewriteTest
    : public GemmRewriteTest,
      public ::testing::WithParamInterface<bool> {
 public:
  ParameterizedGemmRewriteTest() {
    const bool kUsingCublasLt = GetParam();
    replacements_[kCustomCallTargetPlaceholder] =
        kUsingCublasLt ? "__cublas$lt$matmul" : "__cublas$gemm";
  }
  DebugOptions GetDebugOptionsForTest() override {
    DebugOptions debug_options = GemmRewriteTest::GetDebugOptionsForTest();
    debug_options.set_xla_gpu_enable_cublaslt(GetParam());
    debug_options.set_xla_gpu_enable_triton_gemm(false);
    return debug_options;
  }
  void MatchOptimizedHlo(absl::string_view hlo, const absl::string_view pattern,
                         bool print_operand_shape = false) {
    GemmRewriteTest::MatchOptimizedHlo(
        hlo, absl::StrReplaceAll(pattern, replacements_), print_operand_shape);
  }
  absl::string_view CustomCallTarget() {
    return replacements_[kCustomCallTargetPlaceholder];
  }

 protected:
  void SetUp() override {
    if (SkipGpuBlasLtTest()) {
      GTEST_SKIP() << "BlasLt is not supported on this GPU architecture";
    }
  }

 protected:
  absl::flat_hash_map<absl::string_view, absl::string_view> replacements_;

 private:
  static constexpr const char* kCustomCallTargetPlaceholder{
      "<<CUBLAS_CUSTOM_CALL_TARGET_PLACEHOLDER>>"};
};

TEST_P(ParameterizedGemmRewriteTest, Simple) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[2,3] parameter(0)
  y = f32[3,4] parameter(1)
  ROOT dot_a = f32[2,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %test (x: f32[2,3], y: f32[3,4]) -> f32[2,4] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[2,3]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[3,4]{1,0} parameter(1)
; CHECK-NEXT:    [[GEMM:%[^ ]+]] = {{.*}} custom-call([[P0]], [[P1]]),
; CHECK:           custom_call_target="<<CUBLAS_CUSTOM_CALL_TARGET_PLACEHOLDER>>",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
)");
}

TEST_P(ParameterizedGemmRewriteTest, SimpleRewrite) {
  const char* hlo_text = R"(
HloModule SimpleGemm

ENTRY AddDotsFunc {
  x = f32[2,3] parameter(0)
  y = f32[3,4] parameter(1)
  ROOT dot_a = f32[2,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %AddDotsFunc (x: f32[2,3], y: f32[3,4]) -> f32[2,4] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[2,3]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[3,4]{1,0} parameter(1)
; CHECK-NEXT:    [[GEMM:%[^ ]+]] = {{.*}} custom-call([[P0]], [[P1]]),
; CHECK:           custom_call_target="<<CUBLAS_CUSTOM_CALL_TARGET_PLACEHOLDER>>",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
)");
}

TEST_P(ParameterizedGemmRewriteTest, MultipleContractingDims) {
  const char* hlo_text = R"(
HloModule MultipleContractingCheckGemm

ENTRY AddDotsFunc {
  x = f32[3,4,2] parameter(0)
  y = f32[3,4,5] parameter(1)
  ROOT dot_a = f32[2,5] dot(x, y), lhs_contracting_dims={0,1}, rhs_contracting_dims={0,1}, operand_precision={highest,highest}
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-NOT:     copy
;
; CHECK-LABEL: ENTRY %AddDotsFunc (x: f32[3,4,2], y: f32[3,4,5]) -> f32[2,5] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[3,4,2]{2,1,0} parameter(0)
; CHECK-DAG:     [[P1:%[^ ]+]] = f32[3,4,5]{2,1,0} parameter(1)
; CHECK-DAG:     [[BITCAST0:%[^ ]+]] = f32[2,12]{0,1} bitcast([[P0]])
; CHECK-DAG:     [[BITCAST1:%[^ ]+]] = f32[12,5]{1,0} bitcast([[P1]])
; CHECK-NEXT:    [[GEMM:%[^ ]+]] = {{.*}} custom-call([[BITCAST0]], [[BITCAST1]]),
; CHECK:           custom_call_target="<<CUBLAS_CUSTOM_CALL_TARGET_PLACEHOLDER>>",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["HIGHEST","HIGHEST"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
)");
}

TEST_P(ParameterizedGemmRewriteTest, ArgTransposeFoldCheck) {
  const char* hlo_text = R"(
HloModule ArgTransposeFoldGemm

ENTRY AddDotsFunc {
  x = f32[3,2] parameter(0)
  y = f32[3,4] parameter(1)
  x_transposed = f32[2,3] transpose(x), dimensions={1, 0}
  ROOT dot_a = f32[2,4] dot(x_transposed, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %AddDotsFunc (x: f32[3,2], y: f32[3,4]) -> f32[2,4] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[3,2]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[3,4]{1,0} parameter(1)
; CHECK-NEXT:    [[GEMM:%[^ ]+]] = {{.*}} custom-call([[P0]], [[P1]]),
; CHECK:           custom_call_target="<<CUBLAS_CUSTOM_CALL_TARGET_PLACEHOLDER>>",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["0"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
)");
}

TEST_P(ParameterizedGemmRewriteTest, BatchedArgRowColTransposeFoldCheck) {
  const char* hlo_text = R"(
HloModule BatchedArgRowColTransposeFoldGemm

ENTRY AddDotsFunc {
  x = f32[5,3,2] parameter(0)
  y = f32[5,3,4] parameter(1)
  x_transposed = f32[5,2,3] transpose(x), dimensions={0, 2, 1}
  ROOT dot_a = f32[5,2,4] dot(x_transposed, y), lhs_contracting_dims={2}, rhs_contracting_dims={1}, lhs_batch_dims={0}, rhs_batch_dims={0}
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-3, 1e-3}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %AddDotsFunc (x: f32[5,3,2], y: f32[5,3,4]) -> f32[5,2,4] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[5,3,2]{2,1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[5,3,4]{2,1,0} parameter(1)
; CHECK-NEXT:    [[GEMM:%[^ ]+]] = {{.*}} custom-call([[P0]], [[P1]]),
; CHECK:           custom_call_target="<<CUBLAS_CUSTOM_CALL_TARGET_PLACEHOLDER>>",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":["0"]
; CHECK-DAG:           "rhs_batch_dimensions":["0"]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
)");
}

TEST_P(ParameterizedGemmRewriteTest, BatchRowTransposeFoldCheck) {
  const char* hlo_text = R"(
HloModule BatchRowTransposeFoldCheck

ENTRY AddDotsFunc {
  x = f32[2,5,3] parameter(0)
  y = f32[5,3,4] parameter(1)
  x_transposed = f32[5,2,3] transpose(x), dimensions={1, 0, 2}
  ROOT dot_a = f32[5,2,4] dot(x_transposed, y), lhs_contracting_dims={2}, rhs_contracting_dims={1}, lhs_batch_dims={0}, rhs_batch_dims={0}
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{2.5e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %AddDotsFunc (x: f32[2,5,3], y: f32[5,3,4]) -> f32[5,2,4] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[2,5,3]{2,1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[5,3,4]{2,1,0} parameter(1)
; CHECK-NEXT:    [[GEMM:%[^ ]+]] = {{.*}} custom-call([[P0]], [[P1]]),
; CHECK:           custom_call_target="<<CUBLAS_CUSTOM_CALL_TARGET_PLACEHOLDER>>",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["2"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":["1"]
; CHECK-DAG:           "rhs_batch_dimensions":["0"]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
)");
}

TEST_P(ParameterizedGemmRewriteTest, BatchFromMinorDimTransposeIsNotFolded) {
  const char* hlo_text = R"(
HloModule BatchFromMinorDimTransposeDoesntFold

ENTRY AddDotsFunc {
  x = f32[3,2,5] parameter(0)
  y = f32[5,3,4] parameter(1)
  x_transposed = f32[5,2,3] transpose(x), dimensions={2, 1, 0}
  ROOT dot_a = f32[5,2,4] dot(x_transposed, y), lhs_contracting_dims={2}, rhs_contracting_dims={1}, lhs_batch_dims={0}, rhs_batch_dims={0}
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{2.5e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %AddDotsFunc (x: f32[3,2,5], y: f32[5,3,4]) -> f32[5,2,4] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[3,2,5]{2,1,0} parameter(0)
; CHECK-DAG:     [[P1:%[^ ]+]] = f32[5,3,4]{2,1,0} parameter(1)
; CHECK-DAG:     [[FUSION:%[^ ]+]] = f32[5,2,3]{2,1,0} transpose([[P0]])
; CHECK-NEXT:    [[GEMM:%[^ ]+]] = {{.*}} custom-call([[FUSION]], [[P1]]),
; CHECK:           custom_call_target="<<CUBLAS_CUSTOM_CALL_TARGET_PLACEHOLDER>>",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["2"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":["0"]
; CHECK-DAG:           "rhs_batch_dimensions":["0"]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
)");
}

TEST_P(ParameterizedGemmRewriteTest, LargeBatch) {
  const char* hlo_text = R"(
HloModule BatchedArgRowColTransposeFoldGemm

ENTRY AddDotsFunc {
  x = f32[20000,4,3,2] parameter(0)
  y = f32[20000,4,3,4] parameter(1)
  ROOT dot_a = f32[20000,4,2,4] dot(x, y), lhs_contracting_dims={2}, rhs_contracting_dims={2}, lhs_batch_dims={0,1}, rhs_batch_dims={0,1}
}

)";

  // Batch sizes larger than 2^16-1 are not supported by cublasLt. Ensure that
  // the custom_call_target is __cublas$gemm.
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-3, 1e-3}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %AddDotsFunc (x: f32[20000,4,3,2], y: f32[20000,4,3,4]) -> f32[20000,4,2,4] {
; CHECK:    [[P0:%[^ ]+]] = f32[20000,4,3,2]{3,2,1,0} parameter(0)
; CHECK:    [[BC0:%[^ ]+]] = f32[80000,3,2]{2,1,0} bitcast([[P0]])
; CHECK:    [[P1:%[^ ]+]] = f32[20000,4,3,4]{3,2,1,0} parameter(1)
; CHECK:    [[BC1:%[^ ]+]] = f32[80000,3,4]{2,1,0} bitcast([[P1]])
; CHECK:    [[GEMM:%[^ ]+]] = (f32[80000,2,4]{2,1,0}, s8[{{[0-9]+}}]{0}) custom-call([[BC0]], [[BC1]]),
; CHECK:           custom_call_target="__cublas$gemm",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":["0"]
; CHECK-DAG:           "rhs_batch_dimensions":["0"]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK:           }
; CHECK:   [[OUT:%[^ ]+]] = f32[80000,2,4]{2,1,0} get-tuple-element([[GEMM]]), index=0
; CHECK:   ROOT {{[^ ]+}} = f32[20000,4,2,4]{3,2,1,0} bitcast([[OUT]])
)");
}

TEST_P(ParameterizedGemmRewriteTest, InstrTransposeFoldCheck) {
  const char* hlo_text = R"(
HloModule InstrTransposeFoldGemm

ENTRY AddDotsFunc {
  x = f32[2,3] parameter(0)
  y = f32[3,4] parameter(1)
  dot_a = f32[2,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  ROOT out = f32[4,2] transpose(dot_a), dimensions={1, 0}
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %AddDotsFunc (x: f32[2,3], y: f32[3,4]) -> f32[4,2] {
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[3,4]{1,0} parameter(1)
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[2,3]{1,0} parameter(0)
; CHECK-NEXT:    [[GEMM:%[^ ]+]] = {{.*}} custom-call([[P1]], [[P0]]),
; CHECK:           custom_call_target="<<CUBLAS_CUSTOM_CALL_TARGET_PLACEHOLDER>>",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["0"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
)");
}

TEST_P(ParameterizedGemmRewriteTest, BatchedInstrLayoutTransposed) {
  const char* hlo_text = R"(
HloModule BatchedInstrLayoutCheck

ENTRY AddDotsFunc {
  x = f32[5,2,3] parameter(0)
  y = f32[5,3,4] parameter(1)
  dot_a = f32[5,2,4] dot(x, y), lhs_contracting_dims={2}, rhs_contracting_dims={1}, lhs_batch_dims={0}, rhs_batch_dims={0}
  ROOT out = f32[2,5,4] transpose(dot_a), dimensions={1, 0, 2}
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{2.5e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %AddDotsFunc (x: f32[5,2,3], y: f32[5,3,4]) -> f32[2,5,4] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[5,2,3]{2,1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[5,3,4]{2,1,0} parameter(1)
; CHECK-NEXT:    [[GEMM:%[^ ]+]] = {{.*}} custom-call([[P0]], [[P1]]),
; CHECK:           custom_call_target="<<CUBLAS_CUSTOM_CALL_TARGET_PLACEHOLDER>>",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["2"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":["0"]
; CHECK-DAG:           "rhs_batch_dimensions":["0"]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
; CHECK:         ROOT [[OUT:%[^ ]+]] = f32[2,5,4]{2,1,0} bitcast
)");
}

TEST_P(ParameterizedGemmRewriteTest, BatchedInstrLayoutBatchNotInMinorDim) {
  const char* hlo_text = R"(
HloModule BatchedInstrLayoutBatchNotInMinorDim

ENTRY AddDotsFunc {
  x = f32[5,2,3] parameter(0)
  y = f32[5,3,4] parameter(1)
  dot_a = f32[5,2,4] dot(x, y), lhs_contracting_dims={2}, rhs_contracting_dims={1}, lhs_batch_dims={0}, rhs_batch_dims={0}
  ROOT out = f32[2,4,5] transpose(dot_a), dimensions={1, 2, 0}
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{2.5e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %AddDotsFunc (x: f32[5,2,3], y: f32[5,3,4]) -> f32[2,4,5] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[5,2,3]{2,1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[5,3,4]{2,1,0} parameter(1)
; CHECK-NEXT:    [[GEMM:%[^ ]+]] = {{.*}} custom-call([[P0]], [[P1]]),
; CHECK:           custom_call_target="<<CUBLAS_CUSTOM_CALL_TARGET_PLACEHOLDER>>",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["2"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":["0"]
; CHECK-DAG:           "rhs_batch_dimensions":["0"]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
; CHECK:         ROOT [[OUT:%[^ ]+]] = f32[2,4,5]{2,1,0} [[OP:[^ ]+]]
)");
}

TEST_P(ParameterizedGemmRewriteTest, AlphaSimpleRewrite) {
  const char* hlo_text = R"(
HloModule AlphaSimpleRewrite

ENTRY AddDotsFunc {
  x = f32[2,2] parameter(0)
  y = f32[2,2] parameter(1)
  k = f32[] constant(3.0)
  k_broadcast = f32[2, 2] broadcast(k), dimensions={}
  dot_a = f32[2,2] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}, operand_precision={highest,highest}
  ROOT dot_a_multiplied = f32[2, 2] multiply(dot_a, k_broadcast)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %AddDotsFunc (x: f32[2,2], y: f32[2,2]) -> f32[2,2] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[2,2]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[2,2]{1,0} parameter(1)
; CHECK-NEXT:    [[GEMM:%[^ ]+]] = {{.*}} custom-call([[P0]], [[P1]]),
; CHECK:           custom_call_target="<<CUBLAS_CUSTOM_CALL_TARGET_PLACEHOLDER>>",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":3
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["HIGHEST","HIGHEST"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
)");
}

TEST_P(ParameterizedGemmRewriteTest, ComplexAlphaSimpleRewrite) {
  if (CudaOrRocmCheck(
          [](se::CudaComputeCapability) { return false; },
          [this](se::RocmComputeCapability rocm) {
            return GetDebugOptionsForTest().xla_gpu_enable_cublaslt();
          })) {
    GTEST_SKIP() << "TODO: Unsupported C64 gpublas-lt datatype on ROCM";
  }
  const char* hlo_text = R"(
HloModule ComplexAlphaSimpleRewrite

ENTRY AddDotsFunc {
  x = c64[2,2] parameter(0)
  y = c64[2,2] parameter(1)
  k = c64[] constant((3.0, 3.0))
  k_broadcast = c64[2, 2] broadcast(k), dimensions={}
  dot_a = c64[2,2] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  ROOT dot_a_multiplied = c64[2, 2] multiply(dot_a, k_broadcast)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-4, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %AddDotsFunc (x: c64[2,2], y: c64[2,2]) -> c64[2,2] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = c64[2,2]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = c64[2,2]{1,0} parameter(1)
; CHECK-NEXT:    [[GEMM:%[^ ]+]] = {{.*}} custom-call([[P0]], [[P1]]),
; CHECK:           custom_call_target="<<CUBLAS_CUSTOM_CALL_TARGET_PLACEHOLDER>>",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":3
; CHECK-DAG:         "alpha_imag":3
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
)");
}

TEST_P(ParameterizedGemmRewriteTest, AlphaMultipleUsersNoRewrite) {
  const char* hlo_text = R"(
HloModule AlphaMultipleUsersNoRewrite

ENTRY AddDotsFunc {
  x = f32[2,2] parameter(0)
  y = f32[2,2] parameter(1)
  k = f32[] constant(3.0)
  k_broadcast = f32[2, 2] broadcast(k), dimensions={}
  dot_a = f32[2,2] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}, operand_precision={highest,highest}
  dot_a_multiplied = f32[2, 2] multiply(dot_a, k_broadcast)
  ROOT out = f32[2,2] add(dot_a_multiplied, dot_a)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK:    {{[^ ]+}} = {{.*}} custom-call({{[^,]+}}, {{[^)]+}}),
; CHECK:           custom_call_target="<<CUBLAS_CUSTOM_CALL_TARGET_PLACEHOLDER>>",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["HIGHEST","HIGHEST"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
)");
}

TEST_P(ParameterizedGemmRewriteTest, AlphaVectorNoRewrite) {
  const char* hlo_text = R"(
HloModule AlphaVectorNoRewrite

ENTRY AddDotsFunc {
  x = f32[2,2] parameter(0)
  y = f32[2,2] parameter(1)
  alpha = f32[2] constant({1, 2})
  alpha_broadcast = f32[2,2] broadcast(alpha), dimensions={1}
  dot = f32[2,2] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  ROOT dot_a_multiplied = f32[2, 2] multiply(dot, alpha_broadcast)
}
)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %AddDotsFunc (x: f32[2,2], y: f32[2,2]) -> f32[2,2] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[2,2]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[2,2]{1,0} parameter(1)
; CHECK-NEXT:    [[GEMM:%[^ ]+]] = {{.*}} custom-call([[P0]], [[P1]]),
; CHECK:           custom_call_target="<<CUBLAS_CUSTOM_CALL_TARGET_PLACEHOLDER>>",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
)");
}

TEST_P(ParameterizedGemmRewriteTest, BF16Gemm) {
  const char* hlo_text = R"(
HloModule bf16gemm

ENTRY bf16gemm {
  %parameter.1 = bf16[12,4]{1,0} parameter(0)
  %parameter.2 = bf16[4,8]{1,0} parameter(1)
  ROOT %dot.8 = bf16[12,8] dot(bf16[12,4] %parameter.1, bf16[4,8] %parameter.2), lhs_contracting_dims={1}, rhs_contracting_dims={0}
}
  )";
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));

  if (CudaOrRocmCheck(se::CudaComputeCapability::AMPERE, Switch::True)) {
    MatchOptimizedHlo(hlo_text,
                      R"(
; CHECK: {{.*}} custom-call(bf16[16,8]{1,0} {{.*}}, bf16[8,8]{1,0} {{.*}}), custom_call_target="<<CUBLAS_CUSTOM_CALL_TARGET_PLACEHOLDER>>"
  )",
                      /*print_operand_shape=*/true);
  } else {
    MatchOptimizedHlo(hlo_text,
                      R"(
; CHECK: {{.*}} custom-call(bf16[12,4]{1,0} [[P0:%[^ ]+]], bf16[4,8]{1,0} [[P1:%[^ ]+]]), custom_call_target="<<CUBLAS_CUSTOM_CALL_TARGET_PLACEHOLDER>>"
  )",
                      /*print_operand_shape=*/true);
  }
}

TEST_P(ParameterizedGemmRewriteTest, BF16GemmStrided) {
  const char* hlo_text = R"(
HloModule bf16gemm

ENTRY bf16gemm {
  %parameter.1 = bf16[3,3,4] parameter(0)
  %parameter.2 = bf16[3,3,2] parameter(1)
  ROOT %dot.3 = bf16[3,4,2]{2,1,0} dot(bf16[3,3,4]{2,1,0} %parameter.1, bf16[3,3,2]{2,1,0} %parameter.2), lhs_batch_dims={0}, lhs_contracting_dims={1}, rhs_batch_dims={0}, rhs_contracting_dims={1}, operand_precision={highest,highest}
}

  )";
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));

  if (CudaOrRocmCheck(se::CudaComputeCapability::AMPERE, Switch::True)) {
    MatchOptimizedHlo(hlo_text,
                      R"(
    ; CHECK: {{.*}} custom-call(bf16[3,8,8]{2,1,0} {{.*}}, bf16[3,8,8]{2,1,0} {{.*}}), custom_call_target="<<CUBLAS_CUSTOM_CALL_TARGET_PLACEHOLDER>>"
    )",
                      /*print_operand_shape=*/true);
  } else if (GetParam()) {
    MatchOptimizedHlo(hlo_text,
                      R"(
    ; CHECK: ROOT [[OUT:%[^ ]+]] = bf16[3,4,2]{2,1,0} custom-call(bf16[3,3,4]{2,1,0} [[A:%[^ ]+]], bf16[3,3,2]{2,1,0} [[B:%[^ ]+]]), custom_call_target="<<CUBLAS_CUSTOM_CALL_TARGET_PLACEHOLDER>>"
    )",
                      /*print_operand_shape=*/true);
  } else {
    MatchOptimizedHlo(hlo_text,
                      R"(
    ; CHECK: {{.*}} custom-call(bf16[3,3,4]{2,1,0} [[A:%[^ ]+]], bf16[3,3,2]{2,1,0} [[B:%[^ ]+]]), custom_call_target="<<CUBLAS_CUSTOM_CALL_TARGET_PLACEHOLDER>>"
    )",
                      /*print_operand_shape=*/true);
  }
}

TEST_P(ParameterizedGemmRewriteTest, Int8Gemm) {
  if (CudaOrRocmCheck(Switch::False, Switch::True)) {
    GTEST_SKIP() << "DoBlasGemmWithAlgorithm is not yet implemented on ROCm";
  }

  const char* hlo_text = R"(
HloModule int8gemm

ENTRY int8gemm {
  %parameter.1 = s8[12,4]{1,0} parameter(0)
  %parameter.2 = s8[4,8]{1,0} parameter(1)
  ROOT %dot.8 = s32[12,8] dot(s8[12,4] %parameter.1, s8[4,8] %parameter.2), lhs_contracting_dims={1}, rhs_contracting_dims={0}
}
  )";
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));

  if (CudaOrRocmCheck(se::CudaComputeCapability::VOLTA, Switch::True)) {
    MatchOptimizedHlo(hlo_text,
                      R"(
; CHECK: {{.*}} custom-call(s8[12,4]{1,0} [[A:%[^ ]+]], s8[4,8]{0,1} [[B:%[^ ]+]]), custom_call_target="__cublas$gemm"
  )",
                      /*print_operand_shape=*/true);
  } else {
    MatchOptimizedHlo(hlo_text,
                      R"(
; CHECK: {{.*}} dot(s32[12,4]{1,0} [[A:%[^ ]+]], s32[4,8]{1,0} [[B:%[^ ]+]]), lhs_contracting_dims={1}, rhs_contracting_dims={0}

  )",
                      /*print_operand_shape=*/true);
  }
}

TEST_F(GemmRewriteTest, Int8GemmRankGreaterThanTwo) {
  if (CudaOrRocmCheck(Switch::False, Switch::True)) {
    GTEST_SKIP() << "DoBlasGemmWithAlgorithm is not yet implemented on ROCm";
  }

  const char* hlo_text = R"(
HloModule int8gemm

ENTRY main.4 {
  Arg_0.1 = s8[1,8,2]{2,1,0} parameter(0)
  Arg_1.2 = s8[2,4]{1,0} parameter(1)
  ROOT dot.3 = s32[1,8,4]{2,1,0} dot(Arg_0.1, Arg_1.2),
  lhs_contracting_dims={2}, rhs_contracting_dims={0}
}
  )";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));

  if (CudaOrRocmCheck(se::CudaComputeCapability::VOLTA, Switch::True)) {
    MatchOptimizedHlo(hlo_text,
                      R"(
; CHECK: [[GEMM:%[^ ]+]] = (s32[8,4]{1,0}, s8[{{[0-9]+}}]{0}) custom-call(s8[8,4]{1,0} %fusion.1, s8[4,4]{0,1} %bitcast.13), custom_call_target="__cublas$gemm",
  )",
                      /*print_operand_shape=*/true);
  }
}

TEST_P(ParameterizedGemmRewriteTest, Int8GemmNoAlphaRewrite) {
  if (CudaOrRocmCheck(Switch::False, Switch::True)) {
    GTEST_SKIP() << "DoBlasGemmWithAlgorithm is not yet implemented on ROCm";
  }

  const char* hlo_text = R"(
HloModule int8gemm

ENTRY int8gemm {
  %parameter.1 = s8[12,4]{1,0} parameter(0)
  %parameter.2 = s8[4,8]{1,0} parameter(1)
  k = s32[] constant(2)
  k_broadcast = s32[12,8] broadcast(k), dimensions={}
  %dot.8 = s32[12,8] dot(s8[12,4] %parameter.1, s8[4,8] %parameter.2), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  ROOT dot_multiplied = s32[12,8] multiply(%dot.8, k_broadcast)
}
  )";
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));

  if (CudaOrRocmCheck(se::CudaComputeCapability::VOLTA, Switch::True)) {
    MatchOptimizedHlo(hlo_text,
                      R"(
; CHECK: {{.*}} custom-call(s8[12,4]{1,0} [[A:%[^ ]+]], s8[4,8]{0,1} [[B:%[^ ]+]]),
; CHECK:           custom_call_target="__cublas$gemm",
; CHECK:           backend_config={
; CHECK-DAG:       "alpha_real":1
; CHECK-DAG:       "alpha_imag":0
  )",
                      /*print_operand_shape=*/true);
  } else {
    MatchOptimizedHlo(hlo_text,
                      R"(
; CHECK: {{.*}} dot(s32[12,4]{1,0} [[A:%[^ ]+]], s32[4,8]{1,0} [[B:%[^ ]+]]), lhs_contracting_dims={1}, rhs_contracting_dims={0}

  )",
                      /*print_operand_shape=*/true);
  }
}

TEST_P(ParameterizedGemmRewriteTest, Int8GemmNoBetaRewrite) {
  if (CudaOrRocmCheck(Switch::False, Switch::True)) {
    GTEST_SKIP() << "DoBlasGemmWithAlgorithm is not yet implemented on ROCm";
  }
  const char* hlo_text = R"(
HloModule int8gemm

ENTRY int8gemm {
  %parameter.1 = s8[12,4]{1,0} parameter(0)
  %parameter.2 = s8[4,8]{1,0} parameter(1)
  bias = s32[12,8] parameter(2)
  %dot.8 = s32[12,8] dot(s8[12,4] %parameter.1, s8[4,8] %parameter.2), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  ROOT out = s32[12,8] add(%dot.8, bias)
}
  )";
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));

  if (CudaOrRocmCheck(se::CudaComputeCapability::VOLTA, Switch::True)) {
    MatchOptimizedHlo(hlo_text,
                      R"(
; CHECK: {{.*}} custom-call(s8[12,4]{1,0} [[A:%[^ ]+]], s8[4,8]{0,1} [[B:%[^ ]+]]),
; CHECK:           custom_call_target="__cublas$gemm",
; CHECK:           backend_config={
; CHECK-DAG:       "alpha_real":1
; CHECK-DAG:       "alpha_imag":0
; CHECK-DAG:       "beta":0
  )",
                      /*print_operand_shape=*/true);
  } else {
    MatchOptimizedHlo(hlo_text,
                      R"(
; CHECK: {{.*}} dot(s32[12,4]{1,0} [[A:%[^ ]+]], s32[4,8]{1,0} [[B:%[^ ]+]]), lhs_contracting_dims={1}, rhs_contracting_dims={0}

  )",
                      /*print_operand_shape=*/true);
  }
}

TEST_P(ParameterizedGemmRewriteTest, Int8GemmNotMultipleOfFour) {
  if (CudaOrRocmCheck(Switch::False, Switch::True)) {
    GTEST_SKIP() << "DoBlasGemmWithAlgorithm is not yet implemented on ROCm";
  }

  const char* hlo_text = R"(
HloModule int8gemm

ENTRY int8gemm {
  %parameter.1 = s8[13,4]{1,0} parameter(0)
  %parameter.2 = s8[4,9]{1,0} parameter(1)
  ROOT %dot.9 = s32[13,9] dot(s8[13,4] %parameter.1, s8[4,9] %parameter.2), lhs_contracting_dims={1}, rhs_contracting_dims={0}
}
  )";
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));

  if (CudaOrRocmCheck(se::CudaComputeCapability::VOLTA, Switch::True)) {
    MatchOptimizedHlo(hlo_text,
                      R"(
; CHECK: {{.*}} custom-call(s8[16,4]{1,0} [[A:%[^ ]+]], s8[4,12]{0,1} [[B:%[^ ]+]]), custom_call_target="__cublas$gemm"
  )",
                      /*print_operand_shape=*/true);
  } else {
    MatchOptimizedHlo(hlo_text,
                      R"(
; CHECK: {{.*}} dot(s32[13,4]{1,0} [[A:%[^ ]+]], s32[4,9]{1,0} [[B:%[^ ]+]]), lhs_contracting_dims={1}, rhs_contracting_dims={0}

  )",
                      /*print_operand_shape=*/true);
  }
}

TEST_P(ParameterizedGemmRewriteTest, GemmTypeCombinationCheck) {
  if (CudaOrRocmCheck(Switch::False, Switch::True)) {
    GTEST_SKIP() << "DoBlasGemmWithAlgorithm is not yet implemented on ROCm";
  }

  std::vector<std::tuple<absl::string_view, absl::string_view, bool>>
      type_combinations = {{"s8", "s8", true},
                           {"s32", "s32", true},
                           {"bf16", "bf16", true},
                           {"f16", "f16", true},
                           {"f32", "f32", true},
                           {"f64", "f64", true},
                           {"c64", "c64", true},
                           {"c128", "c128", true},
                           // add mix type gemm
                           {"s8", "s32", true},
                           {"s8", "f32", true},
                           {"f16", "f32", true},
                           {"bf16", "f32", true}};

  if (CudaOrRocmCheck(se::CudaComputeCapability::VOLTA, Switch::True)) {
    // For compute capabilities before volta, we always do upcasting, so it
    // would be impossible for this test to fail. That is why we only add these
    // cases when the compute capability is at least Volta.
    std::vector<std::tuple<absl::string_view, absl::string_view, bool>>
        more_type_combinations = {
            {"s8", "bf16", false},  {"s8", "f16", false},
            {"s8", "f64", false},   {"s8", "c64", false},
            {"s8", "c128", false},

            {"s32", "f32", false},  {"s32", "f64", false},
            {"s32", "c64", false},  {"s32", "c128", false},

            {"f16", "bf16", false}, {"f16", "f64", false},
            {"f16", "c64", false},  {"f16", "c128", false},

            {"bf16", "f16", false}, {"bf16", "f64", false},
            {"bf16", "c64", false}, {"bf16", "c128", false},

            {"f32", "f64", false},  {"f32", "c64", false},
            {"f32", "c128", false},

            {"f64", "c64", false},  {"f64", "c128", false},
        };
    type_combinations.insert(type_combinations.end(),
                             more_type_combinations.begin(),
                             more_type_combinations.end());
  }

  for (const auto& type_combination : type_combinations) {
    absl::flat_hash_map<absl::string_view, absl::string_view> replacements;
    replacements["<<ABType>>"] = std::get<0>(type_combination);
    replacements["<<DType>>"] = std::get<1>(type_combination);
    const char* hlo_template = R"(
  HloModule type_combo

  ENTRY type_combo {
    %parameter.1 = <<ABType>>[4,4]{1,0} parameter(0)
    %parameter.2 = <<ABType>>[4,4]{1,0} parameter(1)
    ROOT %dot = <<DType>>[4,4] dot(%parameter.1, %parameter.2), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  }
    )";
    const auto hlo_text = absl::StrReplaceAll(hlo_template, replacements);
    if (std::get<2>(type_combination)) {
      EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-3, 1e-3}));
    } else {
      EXPECT_FALSE(RunAndCompare(hlo_text, ErrorSpec{1e-3, 1e-3}));
    }
  }
}

TEST_P(ParameterizedGemmRewriteTest, UpcastingBf16ToF64) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  Arg_0.1 = bf16[4,3]{1,0} parameter(0)
  Arg_1.2 = bf16[3,6]{1,0} parameter(1)
  ROOT dot.3 = f64[4,6]{1,0} dot(Arg_0.1, Arg_1.2), lhs_contracting_dims={1}, rhs_contracting_dims={0}
}
)";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  GemmRewriter pass(GpuComputeComp());
  TF_ASSERT_OK_AND_ASSIGN(bool changed, this->RunHloPass(&pass, module.get()));
  EXPECT_TRUE(changed);

  // This is a type combination which is not supported by cublasLt, expect
  // GemmRewriter to choose legacy cublas.
  EXPECT_THAT(
      module->entry_computation()->root_instruction(),
      GmockMatch(m::GetTupleElement(m::CustomCall({"__cublas$gemm"}), 0)));
}

TEST_P(ParameterizedGemmRewriteTest, UpcastingC64ToC128) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  Arg_0.1 = c64[4,3]{1,0} parameter(0)
  Arg_1.2 = c64[3,6]{1,0} parameter(1)
  ROOT dot.3 = c128[4,6]{1,0} dot(Arg_0.1, Arg_1.2), lhs_contracting_dims={1}, rhs_contracting_dims={0}
}
)";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  GemmRewriter pass(GpuComputeComp());
  TF_ASSERT_OK_AND_ASSIGN(bool changed, this->RunHloPass(&pass, module.get()));
  EXPECT_TRUE(changed);

  // This is a type combination which is not supported by cublasLt, expect
  // GemmRewriter to choose legacy cublas.
  EXPECT_THAT(
      module->entry_computation()->root_instruction(),
      GmockMatch(m::GetTupleElement(m::CustomCall({"__cublas$gemm"}), 0)));
}

TEST_P(ParameterizedGemmRewriteTest, UpcastingF16ToF32) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  Arg_0.1 = f16[4,3]{1,0} parameter(0)
  Arg_1.2 = f16[3,6]{1,0} parameter(1)
  ROOT dot.3 = f32[4,6]{1,0} dot(Arg_0.1, Arg_1.2), lhs_contracting_dims={1}, rhs_contracting_dims={0}, operand_precision={highest, highest}
}
)";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  GemmRewriter pass(GpuComputeComp());
  TF_ASSERT_OK_AND_ASSIGN(bool changed, this->RunHloPass(&pass, module.get()));
  EXPECT_TRUE(changed);

  if (GetParam()) {
    EXPECT_THAT(module->entry_computation()->root_instruction(),
                GmockMatch(m::CustomCall({CustomCallTarget()})));
  } else {
    EXPECT_THAT(
        module->entry_computation()->root_instruction(),
        GmockMatch(m::GetTupleElement(m::CustomCall({CustomCallTarget()}), 0)));
  }
}

TEST_P(ParameterizedGemmRewriteTest, UpcastingF16ToF64) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  Arg_0.1 = f16[4,3]{1,0} parameter(0)
  Arg_1.2 = f16[3,6]{1,0} parameter(1)
  ROOT dot.3 = f64[4,6]{1,0} dot(Arg_0.1, Arg_1.2), lhs_contracting_dims={1}, rhs_contracting_dims={0}
}
)";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  GemmRewriter pass(GpuComputeComp());
  TF_ASSERT_OK_AND_ASSIGN(bool changed, this->RunHloPass(&pass, module.get()));
  EXPECT_TRUE(changed);

  // This is a type combination which is not supported by cublasLt, expect
  // GemmRewriter to choose legacy cublas.
  EXPECT_THAT(
      module->entry_computation()->root_instruction(),
      GmockMatch(m::GetTupleElement(m::CustomCall({"__cublas$gemm"}), 0)));
}

TEST_P(ParameterizedGemmRewriteTest, UpcastingF32ToF64) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  Arg_0.1 = f32[4,3]{1,0} parameter(0)
  Arg_1.2 = f32[3,6]{1,0} parameter(1)
  ROOT dot.3 = f64[4,6]{1,0} dot(Arg_0.1, Arg_1.2), lhs_contracting_dims={1}, rhs_contracting_dims={0}
}
)";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  GemmRewriter pass(GpuComputeComp());
  TF_ASSERT_OK_AND_ASSIGN(bool changed, this->RunHloPass(&pass, module.get()));
  EXPECT_TRUE(changed);

  // This is a type combination which is not supported by cublasLt, expect
  // GemmRewriter to choose legacy cublas.
  EXPECT_THAT(
      module->entry_computation()->root_instruction(),
      GmockMatch(m::GetTupleElement(m::CustomCall({"__cublas$gemm"}), 0)));
}

TEST_P(ParameterizedGemmRewriteTest, DoNotUpconvertOutput) {
  const char* hlo_text = R"(
HloModule test

ENTRY main {
  param_0 = f16[240,88]{1,0} parameter(0)
  param_1 = f16[88,4]{1,0} parameter(1)
  dot = f16[240,4]{1,0} dot(param_0, param_1), lhs_contracting_dims={1}, rhs_contracting_dims={0}, operand_precision={highest,highest}
  constant_255 = f16[] constant(255)
  broadcast = f16[240,4]{1,0} broadcast(constant_255), dimensions={}
  multiply = f16[240,4]{1,0} multiply(dot, broadcast)
  ROOT result = f32[240,4]{1,0} convert(multiply)
}
)";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  GemmRewriter pass(GpuComputeComp());
  TF_ASSERT_OK_AND_ASSIGN(bool changed, this->RunHloPass(&pass, module.get()));
  EXPECT_TRUE(changed);

  // input fp16 and output fp32 combination is supported by legacy cublas and
  // cublasLt, expect GemmRewriter to fuse the convert into gemm.
  if (GetParam()) {
    EXPECT_THAT(module->entry_computation()->root_instruction(),
                GmockMatch(m::Convert(m::CustomCall({CustomCallTarget()}))));
  } else {
    EXPECT_THAT(module->entry_computation()->root_instruction(),
                GmockMatch(m::Convert(m::GetTupleElement(
                    m::CustomCall({CustomCallTarget()}), 0))));
  }
}

TEST_P(ParameterizedGemmRewriteTest, UnsupportedMixTypeGemm) {
  const char* hlo_text = R"(
HloModule test

ENTRY main {
  param_0 = f32[240,88]{1,0} parameter(0)
  param_1 = f32[88,4]{1,0} parameter(1)
  dot = f32[240,4]{1,0} dot(param_0, param_1), lhs_contracting_dims={1}, rhs_contracting_dims={0}, operand_precision={highest,highest}
  constant_255 = f32[] constant(255)
  broadcast = f32[240,4]{1,0} broadcast(constant_255), dimensions={}
  multiply = f32[240,4]{1,0} multiply(dot, broadcast)
  ROOT result = u8[240,4]{1,0} convert(multiply)
}
)";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  GemmRewriter pass(GpuComputeComp());
  TF_ASSERT_OK_AND_ASSIGN(bool changed, this->RunHloPass(&pass, module.get()));
  EXPECT_TRUE(changed);

  // u8 is not supported by legacy cublas and cublasLt, expect
  // GemmRewriter to not fuse the convert into gemm.
  if (GetParam()) {
    EXPECT_THAT(module->entry_computation()->root_instruction(),
                GmockMatch(m::Convert(m::CustomCall({CustomCallTarget()}))));
  } else {
    EXPECT_THAT(module->entry_computation()->root_instruction(),
                GmockMatch(m::Convert(m::GetTupleElement(
                    m::CustomCall({CustomCallTarget()}), 0))));
  }
}

TEST_P(ParameterizedGemmRewriteTest, CheckIsGemmAliasedBeforeFusion) {
  const char* hlo_text = R"(
HloModule test

ENTRY main {
  Arg_0.1 = f16[8,16]{1,0} parameter(0)
  Arg_1.2 = f16[16,32]{1,0} parameter(1)
  dot.8 = f16[8,32]{1,0} dot(Arg_0.1, Arg_1.2), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  Arg_2.3 = f16[8,32]{1,0} parameter(2)
  constant.5 = f16[] constant(1)
  broadcast.6 = f16[8,32]{1,0} broadcast(constant.5), dimensions={}
  add.7 = f16[8,32]{1,0} add(Arg_2.3, broadcast.6)
  add.9 = f16[8,32]{1,0} add(dot.8, add.7)
  convert.10 = f32[8,32]{1,0} convert(add.9)
}
)";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  GemmRewriter pass(GpuComputeComp());
  TF_ASSERT_OK_AND_ASSIGN(bool changed, this->RunHloPass(&pass, module.get()));
  EXPECT_TRUE(changed);

  // input fp16 and output fp32 combination is supported by legacy cublas and
  // cublasLt, but gemm output is already aliased with one of the input expect
  // GemmRewriter to not fuse the convert into gemm.
  if (GetParam()) {
    EXPECT_THAT(module->entry_computation()->root_instruction(),
                GmockMatch(m::Convert(m::CustomCall({CustomCallTarget()}))));
  } else {
    EXPECT_THAT(module->entry_computation()->root_instruction(),
                GmockMatch(m::Convert(m::GetTupleElement(
                    m::CustomCall({CustomCallTarget()}), 0))));
  }
}

INSTANTIATE_TEST_SUITE_P(CublasTestsBothLegacyAndLt,
                         ParameterizedGemmRewriteTest, ::testing::Bool());
#endif

// A test fixture class for tests which are specific to legacy cublas
class LegacyCublasGemmRewriteTest : public GemmRewriteTest {
 public:
  DebugOptions GetDebugOptionsForTest() override {
    DebugOptions debug_options = GemmRewriteTest::GetDebugOptionsForTest();
    debug_options.set_xla_gpu_enable_triton_gemm(false);
    debug_options.set_xla_gpu_enable_cublaslt(false);
    return debug_options;
  }
};

// Test that the alpha and beta fields of the GemmBackendConfig are updated.
// A bias must be present for the beta value to be set.
// In order to have a bias add fused, the bias term must be overwritable.
// We assume that we may not overwrite parameters of a computation. Hence, we
// use the third parameter to create a new value which can be overwritten and
// will be used as the bias. This negate(param_2) has no semantic use, it simply
// exists so that bias may be overwritten.
TEST_F(LegacyCublasGemmRewriteTest, AlphaBetaRewrite) {
  const char* hlo_text = R"(
HloModule NonZeroAlphaBeta

ENTRY AddDotsFunc {
  x = f32[2,2] parameter(0)
  y = f32[2,2] parameter(1)
  param_2 = f32[2,2] parameter(2)
  bias = f32[2,2] negate(param_2)
  k = f32[] constant(3.0)
  k_broadcast = f32[2, 2] broadcast(k), dimensions={}
  dot_a = f32[2,2] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}, operand_precision={highest,highest}
  dot_a_multiplied = f32[2, 2] multiply(dot_a, k_broadcast)
  ROOT out = f32[2,2] add(dot_a_multiplied, bias)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %AddDotsFunc (x: f32[2,2], y: f32[2,2], param_2: f32[2,2]) -> f32[2,2] {
; CHECK-DAG:     [[X:%[^ ]+]] = f32[2,2]{1,0} parameter(0)
; CHECK-DAG:     [[Y:%[^ ]+]] = f32[2,2]{1,0} parameter(1)
; CHECK:         [[O:%[^ ]+]] = (f32[2,2]{1,0}, s8[{{[0-9]+}}]{0}) custom-call([[X]], [[Y]], {{[^,)]+}}),
; CHECK:           custom_call_target="__cublas$gemm",
; CHECK:           output_to_operand_aliasing={
; CHECK-SAME:        {0}: (2, {})
; CHECK-SAME:      }
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":3
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["HIGHEST","HIGHEST"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
; CHECK:         ROOT [[OUT:%[^ ]+]] = f32[2,2]{1,0} get-tuple-element([[O]]), index=0
)");
}

TEST_F(LegacyCublasGemmRewriteTest, BiasMultipleUsersNoOverwrite) {
  const char* hlo_text = R"(
HloModule BiasMultipleUsersNoOverwrite

ENTRY AddDotsFunc {
  x = f32[2,2] parameter(0)
  y = f32[2,2] parameter(1)
  bias = f32[2,2] parameter(2)
  k = f32[] constant(3.0)
  k_broadcast = f32[2, 2] broadcast(k), dimensions={}
  dot_a = f32[2,2] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}, operand_precision={highest,highest}
  dot_a_multiplied = f32[2, 2] multiply(dot_a, k_broadcast)
  biased_out = f32[2,2] add(dot_a_multiplied, bias)
  ROOT out = f32[2,2] add(biased_out, bias)
}
)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %AddDotsFunc (x: f32[2,2], y: f32[2,2], bias: f32[2,2]) -> f32[2,2] {
; CHECK-DAG:     [[P0:%[^ ]+]] = f32[2,2]{1,0} parameter(0)
; CHECK-DAG:     [[P1:%[^ ]+]] = f32[2,2]{1,0} parameter(1)
; CHECK-NEXT:    [[GEMM:%[^ ]+]] = (f32[2,2]{1,0}, s8[{{[0-9]+}}]{0}) custom-call([[P0]], [[P1]]),
; CHECK:           custom_call_target="__cublas$gemm",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":3
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["HIGHEST","HIGHEST"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
)");
}

TEST_F(LegacyCublasGemmRewriteTest, BiasParameterNoOverwrite) {
  const char* hlo_text = R"(
HloModule BiasParameterNoOverwrite

ENTRY AddDotsFunc {
  x = f32[2,2] parameter(0)
  y = f32[2,2] parameter(1)
  bias = f32[2,2] parameter(2)
  dot_a = f32[2,2] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  ROOT out = f32[2,2] add(dot_a, bias)
}
)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %AddDotsFunc (x: f32[2,2], y: f32[2,2], bias: f32[2,2]) -> f32[2,2] {
; CHECK-DAG:     [[P0:%[^ ]+]] = f32[2,2]{1,0} parameter(0)
; CHECK-DAG:     [[P1:%[^ ]+]] = f32[2,2]{1,0} parameter(1)
; CHECK-NEXT:    [[GEMM:%[^ ]+]] = (f32[2,2]{1,0}, s8[{{[0-9]+}}]{0}) custom-call([[P0]], [[P1]]),
; CHECK:           custom_call_target="__cublas$gemm",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
)");
}

TEST_F(LegacyCublasGemmRewriteTest, BiasTupleParameterOverwrite) {
  const char* hlo_text = R"(
HloModule BiasTupleParameterOverwrite

ENTRY AddDotsFunc {
  x = f32[2,2] parameter(0)
  y = f32[2,2] parameter(1)
  param_2 = (f32[2,2], f32[3,3]) parameter(2)
  bias = f32[2,2] get-tuple-element(param_2), index=0
  dot_a = f32[2,2] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  ROOT out = f32[2,2] add(dot_a, bias)
}
)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %AddDotsFunc (x: f32[2,2], y: f32[2,2], param_2: (f32[2,2], f32[3,3])) -> f32[2,2] {
; CHECK-DAG:     [[P0:%[^ ]+]] = f32[2,2]{1,0} parameter(0)
; CHECK-DAG:     [[P1:%[^ ]+]] = f32[2,2]{1,0} parameter(1)
; CHECK-DAG:     [[P2:%[^ ]+]] = (f32[2,2]{1,0}, f32[3,3]{1,0}) parameter(2)
; CHECK-DAG:     [[BIAS:%[^ ]+]] = f32[2,2]{1,0} get-tuple-element([[P2]]), index=0
; CHECK-DAG:     [[BIAS_COPY:%[^ ]+]] = f32[2,2]{1,0} copy([[BIAS]])
; CHECK-NEXT:    [[GEMM:%[^ ]+]] = (f32[2,2]{1,0}, s8[{{[0-9]+}}]{0}) custom-call([[P0]], [[P1]], [[BIAS_COPY]]),
; CHECK:           custom_call_target="__cublas$gemm",
; CHECK:           output_to_operand_aliasing={
; CHECK-SAME:        {0}: (2, {})
; CHECK-SAME:      }
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
)");
}

TEST_F(LegacyCublasGemmRewriteTest, AliasedBiasOverwrite) {
  const char* hlo_text = R"(
HloModule AliasedBiasOverwrite, input_output_alias={ {}: (2, {}, must-alias) }

ENTRY AddDotsFunc {
  x = f32[2,2] parameter(0)
  y = f32[2,2] parameter(1)
  bias = f32[2,2] parameter(2)
  k = f32[] constant(3.0)
  k_broadcast = f32[2, 2] broadcast(k), dimensions={}
  dot_a = f32[2,2] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}, operand_precision={highest,highest}
  dot_a_multiplied = f32[2, 2] multiply(dot_a, k_broadcast)
  ROOT out = f32[2,2] add(dot_a_multiplied, bias)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %AddDotsFunc (x: f32[2,2], y: f32[2,2], bias: f32[2,2]) -> f32[2,2] {
; CHECK-DAG:     [[X:%[^ ]+]] = f32[2,2]{1,0} parameter(0)
; CHECK-DAG:     [[Y:%[^ ]+]] = f32[2,2]{1,0} parameter(1)
; CHECK-DAG:     [[BIAS:%[^ ]+]] = f32[2,2]{1,0} parameter(2)
; CHECK:         [[GEMM:%[^ ]+]] = (f32[2,2]{1,0}, s8[{{[0-9]+}}]{0}) custom-call([[X]], [[Y]], [[BIAS]]),
; CHECK:           custom_call_target="__cublas$gemm",
; CHECK:           output_to_operand_aliasing={
; CHECK-SAME:        {0}: (2, {})
; CHECK-SAME:      }
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":3
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["HIGHEST","HIGHEST"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
)");
}

TEST_F(LegacyCublasGemmRewriteTest, LargerBiasMultipleUsersNoRewrite) {
  const char* hlo_text = R"(
HloModule LargerBiasMultipleUsersNoRewrite

ENTRY AddDotsFunc {
  x = f32[1024,1024] parameter(0)
  y = f32[1024,1024] parameter(1)
  bias = f32[1024,1024] parameter(2)
  dot_a = f32[1024,1024] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  biased_out = f32[1024,1024] add(dot_a, bias)
  ROOT out = f32[1024,1024] add(biased_out, bias)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-3, 1e-3}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %AddDotsFunc (x: f32[1024,1024], y: f32[1024,1024], bias: f32[1024,1024]) -> f32[1024,1024] {
; CHECK-DAG:     [[P0:%[^ ]+]] = f32[1024,1024]{1,0} parameter(0)
; CHECK-DAG:     [[P1:%[^ ]+]] = f32[1024,1024]{1,0} parameter(1)
; CHECK-NEXT:    [[GEMM:%[^ ]+]] = (f32[1024,1024]{1,0}, s8[{{[0-9]+}}]{0}) custom-call([[P0]], [[P1]]),
; CHECK:           custom_call_target="__cublas$gemm",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
)");
}

// In order to have a bias add fused, the bias term must be overwritable.
// We assume that we may not overwrite parameters of a computation. Hence, we
// use the third parameter to create a new value which can be overwritten and
// will be used as the bias. This negate(param_2) has no semantic use, it simply
// exists so that bias may be overwritten.
TEST_F(LegacyCublasGemmRewriteTest, BF16GemmWithBias) {
  const char* hlo_text = R"(
HloModule BF16GemmWithBias

ENTRY BF16GemmWithBias {
  x = bf16[8,8]{1,0} parameter(0)
  y = bf16[8,8]{1,0} parameter(1)
  dot.5 = bf16[8,8]{1,0} dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  param_2 = bf16[8,8]{1,0} parameter(2)
  bias = bf16[8,8]{1,0} negate(param_2)
  ROOT add.6 = bf16[8,8]{1,0} add(dot.5, bias)
}
  )";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{2e-3, 2e-3}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %BF16GemmWithBias (x: bf16[8,8], y: bf16[8,8], param_2: bf16[8,8]) -> bf16[8,8] {
; CHECK-DAG:    [[X:%[^ ]+]] = bf16[8,8]{1,0} parameter(0)
; CHECK-DAG:    [[Y:%[^ ]+]] = bf16[8,8]{1,0} parameter(1)
; CHECK:        [[GEMM:%[^ ]+]] = (bf16[8,8]{1,0}, s8[{{[0-9]+}}]{0}) custom-call([[X]], [[Y]], {{[^,)]+}}),
; CHECK:           custom_call_target="__cublas$gemm",
; CHECK:           output_to_operand_aliasing={
; CHECK-SAME:        {0}: (2, {})
; CHECK-SAME:      }
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
)");
}

// In order to have a bias add fused, the bias term must be overwritable.
// We assume that we may not overwrite parameters of a computation. Hence, we
// use the third parameter to create a new value which can be overwritten and
// will be used as the bias. This negate(param_2) has no semantic use, it simply
// exists so that bias may be overwritten.
TEST_F(LegacyCublasGemmRewriteTest, MatrixBias) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[2,3] parameter(0)
  y = f32[3,4] parameter(1)
  param_2 = f32[2,4] parameter(2)
  bias = f32[2,4] negate(param_2)
  dot_a = f32[2,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  ROOT out = f32[2,4] add(dot_a, bias)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %test (x: f32[2,3], y: f32[3,4], param_2: f32[2,4]) -> f32[2,4] {
; CHECK-DAG:     [[P0:%[^ ]+]] = f32[2,3]{1,0} parameter(0)
; CHECK-DAG:     [[P1:%[^ ]+]] = f32[3,4]{1,0} parameter(1)
; CHECK:         [[GEMM:%[^ ]+]] = (f32[2,4]{1,0}, s8[{{[0-9]+}}]{0}) custom-call([[P0]], [[P1]], {{[^,)]+}}),
; CHECK:           custom_call_target="__cublas$gemm",
; CHECK:           output_to_operand_aliasing={
; CHECK-SAME:        {0}: (2, {})
; CHECK-SAME:      }
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
)");
}

TEST_F(LegacyCublasGemmRewriteTest, MatrixBiasWhereBiasIsNotAParameter) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  w = f32[2,3] parameter(0)
  x = f32[3,4] parameter(1)
  first_dot = f32[2,4] dot(w, x), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  y = f32[2,3] parameter(2)
  z = f32[3,4] parameter(3)
  second_dot = f32[2,4] dot(y, z), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  ROOT out = f32[2,4] add(second_dot, first_dot)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %test (w: f32[2,3], x: f32[3,4], y: f32[2,3], z: f32[3,4]) -> f32[2,4] {
; CHECK-DAG:     [[P0:%[^ ]+]] = f32[2,3]{1,0} parameter(0)
; CHECK-DAG:     [[P1:%[^ ]+]] = f32[3,4]{1,0} parameter(1)
; CHECK-DAG:     [[P2:%[^ ]+]] = f32[2,3]{1,0} parameter(2)
; CHECK-DAG:     [[P3:%[^ ]+]] = f32[3,4]{1,0} parameter(3)
; CHECK-NEXT:    [[FIRST_GEMM:%[^ ]+]] = (f32[2,4]{1,0}, s8[{{[0-9]+}}]{0}) custom-call([[P0]], [[P1]]),
; CHECK:           custom_call_target="__cublas$gemm",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
; CHECK:         [[FIRST_GEMM_OUT:%[^ ]+]] = f32[2,4]{1,0} get-tuple-element([[FIRST_GEMM]]), index=0
; CHECK-NEXT:    [[SECOND_GEMM:%[^ ]+]] = (f32[2,4]{1,0}, s8[{{[0-9]+}}]{0}) custom-call([[P2]], [[P3]], [[FIRST_GEMM_OUT]]),
; CHECK:           custom_call_target="__cublas$gemm",
; CHECK:           output_to_operand_aliasing={
; CHECK-SAME:        {0}: (2, {})
; CHECK-SAME:      }
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
)");
}

#if GOOGLE_CUDA || TENSORFLOW_USE_ROCM
// Test gemm matrix bias add fusion with mix type
TEST_F(LegacyCublasGemmRewriteTest, MatrixBiasMixType) {
  if (CudaOrRocmCheck(Switch::False, Switch::True)) {
    GTEST_SKIP()
        << "TODO: DoBlasGemmWithAlgorithm is not yet implemented on ROCm";
  }
  std::vector<std::tuple<absl::string_view, absl::string_view>>
      type_combinations = {
          {"f16", "f32"},
          {"bf16", "f32"},
      };

  const char* hlo_text_template = R"(
HloModule test

ENTRY test {
  x = <<ABType>>[16,32] parameter(0)
  y = <<ABType>>[32,16] parameter(1)
  z = <<DType>>[16,16] parameter(2)
  dot_a = <<ABType>>[16,16] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  bias = <<DType>>[16,16] negate(z)
  convert = <<DType>>[16,16] convert(dot_a)
  ROOT out = <<DType>>[16,16] add(convert, bias)
}

)";
  for (const auto& type_combination : type_combinations) {
    absl::flat_hash_map<absl::string_view, absl::string_view> replacements;
    replacements["<<ABType>>"] = std::get<0>(type_combination);
    replacements["<<DType>>"] = std::get<1>(type_combination);
    const auto hlo_text = absl::StrReplaceAll(hlo_text_template, replacements);
    EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-3, 1e-3}));
    TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> optimized_module,
                            GetOptimizedModule(hlo_text));
    EXPECT_THAT(optimized_module->entry_computation()->root_instruction(),
                GmockMatch(m::GetTupleElement(
                    m::CustomCall(m::Parameter(0), m::Parameter(1),
                                  m::Negate(m::Parameter(2))),
                    0)));
  }
}

// Test batch gemm matrix bias add fusion with mix type
TEST_F(LegacyCublasGemmRewriteTest, MatrixBiasMixTypeBatched) {
  if (CudaOrRocmCheck(Switch::False, Switch::True)) {
    GTEST_SKIP()
        << "TODO: DoBlasGemmWithAlgorithm is not yet implemented on ROCm";
  }
  std::vector<std::tuple<absl::string_view, absl::string_view>>
      type_combinations = {
          {"f16", "f32"},
          {"bf16", "f32"},
      };

  const char* hlo_text_template = R"(
HloModule test

ENTRY test {
  x = <<ABType>>[4,16,32] parameter(0)
  y = <<ABType>>[4,32,16] parameter(1)
  z = <<DType>>[4,16,16] parameter(2)
  dot_a = <<ABType>>[4,16,16] dot(x, y), lhs_contracting_dims={2}, rhs_contracting_dims={1}, lhs_batch_dims={0}, rhs_batch_dims={0}
  bias = <<DType>>[4,16,16] negate(z)
  convert = <<DType>>[4,16,16] convert(dot_a)
  ROOT out = <<DType>>[4,16,16] add(convert, bias)
})";
  for (const auto& type_combination : type_combinations) {
    absl::flat_hash_map<absl::string_view, absl::string_view> replacements;
    replacements["<<ABType>>"] = std::get<0>(type_combination);
    replacements["<<DType>>"] = std::get<1>(type_combination);
    const auto hlo_text = absl::StrReplaceAll(hlo_text_template, replacements);
    EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-3, 1e-3}));

    TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> optimized_module,
                            GetOptimizedModule(hlo_text));
    EXPECT_THAT(optimized_module->entry_computation()->root_instruction(),
                GmockMatch(m::GetTupleElement(
                    m::CustomCall(m::Parameter(0), m::Parameter(1),
                                  m::Negate(m::Parameter(2))),
                    0)));
  }
}
#endif

// Test batch gemm matrix bias add fusion with mix type that is not supported
TEST_F(LegacyCublasGemmRewriteTest, MatrixBiasMixTypeNotSupported) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = bf16[16,32] parameter(0)
  y = bf16[32,16] parameter(1)
  z = f64[16,16] parameter(2)
  dot_a = bf16[16,16] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  bias = f64[16,16] negate(z)
  convert = f64[16,16] convert(dot_a)
  ROOT out = f64[16,16] add(convert, bias)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-3, 1e-3}));
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> optimized_module,
                          GetOptimizedModule(hlo_text));
  EXPECT_THAT(
      optimized_module->entry_computation()->root_instruction(),
      GmockMatch(m::Fusion(
          m::Parameter(2),
          m::GetTupleElement(m::CustomCall({"__cublas$gemm"}, m::Parameter(0),
                                           m::Parameter(1)),
                             0))));
}

// Test batch gemm matrix bias add fusion with mix type that is not supported
// cuz there are consumers of bias add
TEST_F(LegacyCublasGemmRewriteTest, MatrixBiasMixTypeAddWithMoreConsumers) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = bf16[16,32] parameter(0)
  y = bf16[32,16] parameter(1)
  z = f32[16,16] parameter(2)
  dot_a = bf16[16,16] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  bias = f32[16,16] negate(z)
  convert = f32[16,16] convert(dot_a)
  add_bias = f32[16,16] add(convert, bias)
  ROOT out = f32[16,16] negate(add_bias)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-3, 1e-3}));
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> optimized_module,
                          GetOptimizedModule(hlo_text));
  EXPECT_THAT(
      optimized_module->entry_computation()->root_instruction(),
      GmockMatch(m::Fusion(
          m::Parameter(2),
          m::GetTupleElement(m::CustomCall({"__cublas$gemm"}, m::Parameter(0),
                                           m::Parameter(1)),
                             0))));
}

TEST_F(LegacyCublasGemmRewriteTest, MergeBitcastAndAdd) {
  const char* hlo_text = R"(
HloModule test
ENTRY test {
  x = f32[2,2] parameter(0)
  y = f32[2,2] parameter(1)
  bias = f32[4] parameter(2)
  dot = f32[2,2] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  ROOT out = f32[4] add(f32[4] bitcast(dot), bias)
}
)";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  GemmRewriter pass(GpuComputeComp());
  TF_ASSERT_OK_AND_ASSIGN(bool changed, this->RunHloPass(&pass, module.get()));
  EXPECT_TRUE(changed);

  EXPECT_THAT(
      module->entry_computation()->root_instruction(),
      GmockMatch(
          m::Bitcast(
              m::GetTupleElement(
                  m::CustomCall(
                      {"__cublas$gemm"}, m::Parameter(0), m::Parameter(1),
                      m::Bitcast(m::Parameter(2)).WithShape(F32, {2, 2})),
                  0))
              .WithShape(F32, {4})));
}

// In order to have a bias add fused, the bias term must be overwritable.
// We assume that we may not overwrite parameters of a computation. Hence, we
// use the third parameter to create a new value which can be overwritten and
// will be used as the bias. This negate(param_2) has no semantic use, it simply
// exists so that bias may be overwritten.
TEST_F(LegacyCublasGemmRewriteTest, FoldConstantBias) {
  const char* hlo_text = R"(
HloModule test
ENTRY test {
  x = f32[2,2] parameter(0)
  y = f32[2,2] parameter(1)
  bias = f32[2,2] broadcast(f32[2] constant({0, 0})), dimensions={0}

  dot1 = f32[2,2] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  param_2 = f32[2,2] parameter(2)
  bias1 = f32[2,2] negate(param_2)
  sum1 = add(dot1, bias1)

  dot2 = f32[2,2] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  sum2 = add(dot2, f32[2,2] reshape(bias))

  dot3 = f32[2,2] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  bias3 = f32[2,2] transpose(bias), dimensions={1,0}
  sum3 = add(dot3, bias3)

  dot4 = f32[2,2] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  sum4 = add(dot4, f32[2,2] bitcast(bias))

  ROOT root = tuple(sum1, sum2, sum3, sum4)
}
)";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  GemmRewriter pass(GpuComputeComp());
  TF_ASSERT_OK_AND_ASSIGN(bool changed, this->RunHloPass(&pass, module.get()));
  SCOPED_TRACE(module->ToString());
  EXPECT_TRUE(changed);

  EXPECT_THAT(
      module->entry_computation()->root_instruction(),
      GmockMatch(m::Tuple(
          m::GetTupleElement(m::CustomCall(m::Parameter(0), m::Parameter(1),
                                           m::Negate(m::Parameter(2))),
                             0),
          m::GetTupleElement(
              m::CustomCall(m::Parameter(0), m::Parameter(1), m::Constant()),
              0),
          m::GetTupleElement(
              m::CustomCall(m::Parameter(0), m::Parameter(1), m::Constant()),
              0),
          m::GetTupleElement(
              m::CustomCall(m::Parameter(0), m::Parameter(1), m::Constant()),
              0))));
}

#if GOOGLE_CUDA || TENSORFLOW_USE_ROCM
// A test fixture class for tests which are specific to cublasLt
class CublasLtGemmRewriteTest : public GemmRewriteTest {
 public:
  DebugOptions GetDebugOptionsForTest() override {
    DebugOptions debug_options = GemmRewriteTest::GetDebugOptionsForTest();
    debug_options.set_xla_gpu_enable_cublaslt(true);
    debug_options.set_xla_gpu_enable_triton_gemm(false);
    return debug_options;
  }

 protected:
  void SetUp() override {
    if (SkipGpuBlasLtTest()) {
      GTEST_SKIP() << "BlasLt is not supported on this GPU architecture";
    }
  }
};

TEST_F(CublasLtGemmRewriteTest, AlphaBetaRewrite) {
  const char* hlo_text = R"(
HloModule NonZeroAlphaBeta

ENTRY AddDotsFunc {
  x = f32[2,2] parameter(0)
  y = f32[2,2] parameter(1)
  bias = f32[2,2] parameter(2)
  k = f32[] constant(3.0)
  k_broadcast = f32[2, 2] broadcast(k), dimensions={}
  dot_a = f32[2,2] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}, operand_precision={highest,highest}
  dot_a_multiplied = f32[2, 2] multiply(dot_a, k_broadcast)
  ROOT out = f32[2,2] add(dot_a_multiplied, bias)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %AddDotsFunc (x: f32[2,2], y: f32[2,2], bias: f32[2,2]) -> f32[2,2] {
; CHECK-DAG:     [[X:%[^ ]+]] = f32[2,2]{1,0} parameter(0)
; CHECK-DAG:     [[Y:%[^ ]+]] = f32[2,2]{1,0} parameter(1)
; CHECK-DAG:     [[BIAS:%[^ ]+]] = f32[2,2]{1,0} parameter(2)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[2,2]{1,0} custom-call([[X]], [[Y]], [[BIAS]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":3
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["HIGHEST","HIGHEST"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
)");
}

TEST_F(CublasLtGemmRewriteTest, BiasMultipleUsersNoOverwrite) {
  const char* hlo_text = R"(
HloModule BiasMultipleUsersNoOverwrite

ENTRY AddDotsFunc {
  x = f32[2,2] parameter(0)
  y = f32[2,2] parameter(1)
  bias = f32[2,2] parameter(2)
  k = f32[] constant(3.0)
  k_broadcast = f32[2, 2] broadcast(k), dimensions={}
  dot_a = f32[2,2] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}, operand_precision={highest,highest}
  dot_a_multiplied = f32[2, 2] multiply(dot_a, k_broadcast)
  biased_out = f32[2,2] add(dot_a_multiplied, bias)
  ROOT out = f32[2,2] add(biased_out, bias)
}
)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %AddDotsFunc (x: f32[2,2], y: f32[2,2], bias: f32[2,2]) -> f32[2,2] {
; CHECK-DAG:     [[P0:%[^ ]+]] = f32[2,2]{1,0} parameter(0)
; CHECK-DAG:     [[P1:%[^ ]+]] = f32[2,2]{1,0} parameter(1)
; CHECK-DAG:     [[BIAS:%[^ ]+]] = f32[2,2]{1,0} parameter(2)
; CHECK-NEXT:    [[GEMM:%[^ ]+]] = f32[2,2]{1,0} custom-call([[P0]], [[P1]], [[BIAS]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK-NOT:       output_to_operand_aliasing
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":3
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["HIGHEST","HIGHEST"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
)");
}

TEST_F(CublasLtGemmRewriteTest, LargerBiasMultipleUsersNoRewrite) {
  const char* hlo_text = R"(
HloModule LargerBiasMultipleUsersNoRewrite

ENTRY AddDotsFunc {
  x = f32[1024,1024] parameter(0)
  y = f32[1024,1024] parameter(1)
  bias = f32[1024,1024] parameter(2)
  dot_a = f32[1024,1024] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  biased_out = f32[1024,1024] add(dot_a, bias)
  ROOT out = f32[1024,1024] add(biased_out, bias)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-3, 1e-3}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %AddDotsFunc (x: f32[1024,1024], y: f32[1024,1024], bias: f32[1024,1024]) -> f32[1024,1024] {
; CHECK-DAG:     [[P0:%[^ ]+]] = f32[1024,1024]{1,0} parameter(0)
; CHECK-DAG:     [[P1:%[^ ]+]] = f32[1024,1024]{1,0} parameter(1)
; CHECK-DAG:     [[BIAS:%[^ ]+]] = f32[1024,1024]{1,0} parameter(2)
; CHECK-NEXT:    [[GEMM:%[^ ]+]] = f32[1024,1024]{1,0} custom-call([[P0]], [[P1]], [[BIAS]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[1024,1024]{1,0} add([[GEMM]], [[BIAS]])
)");
}

TEST_F(CublasLtGemmRewriteTest, BF16GemmWithBias) {
  const char* hlo_text = R"(
HloModule test

ENTRY BF16GemmWithBias {
  x = bf16[8,8]{1,0} parameter(0)
  y = bf16[8,8]{1,0} parameter(1)
  dot.5 = bf16[8,8]{1,0} dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  bias = bf16[8,8]{1,0} parameter(2)
  ROOT add.6 = bf16[8,8]{1,0} add(dot.5, bias)
}
  )";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-3, 1e-3}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %BF16GemmWithBias (x: bf16[8,8], y: bf16[8,8], bias: bf16[8,8]) -> bf16[8,8] {
; CHECK-DAG:    [[X:%[^ ]+]] = bf16[8,8]{1,0} parameter(0)
; CHECK-DAG:    [[Y:%[^ ]+]] = bf16[8,8]{1,0} parameter(1)
; CHECK-DAG:    [[BIAS:%[^ ]+]] = bf16[8,8]{1,0} parameter(2)
; CHECK-NEXT:   ROOT [[GEMM:%[^ ]+]] = bf16[8,8]{1,0} custom-call([[X]], [[Y]], [[BIAS]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
)");
}

TEST_F(CublasLtGemmRewriteTest, MatrixBias) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[2,3] parameter(0)
  y = f32[3,4] parameter(1)
  z = f32[2,4] parameter(2)
  dot_a = f32[2,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  ROOT out = f32[2,4] add(dot_a, z)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %test (x: f32[2,3], y: f32[3,4], z: f32[2,4]) -> f32[2,4] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[2,3]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[3,4]{1,0} parameter(1)
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[2,4]{1,0} parameter(2)
; CHECK-NEXT:    ROOT [[GEMM:%[^ ]+]] = f32[2,4]{1,0} custom-call([[P0]], [[P1]], [[P2]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
)");
}

TEST_F(CublasLtGemmRewriteTest, MatrixBiasWhereBiasIsNotAParameter) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  w = f32[2,3] parameter(0)
  x = f32[3,4] parameter(1)
  first_dot = f32[2,4] dot(w, x), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  y = f32[2,3] parameter(2)
  z = f32[3,4] parameter(3)
  second_dot = f32[2,4] dot(y, z), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  ROOT out = f32[2,4] add(second_dot, first_dot)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %test (w: f32[2,3], x: f32[3,4], y: f32[2,3], z: f32[3,4]) -> f32[2,4] {
; CHECK-DAG:     [[P0:%[^ ]+]] = f32[2,3]{1,0} parameter(0)
; CHECK-DAG:     [[P1:%[^ ]+]] = f32[3,4]{1,0} parameter(1)
; CHECK-DAG:     [[P2:%[^ ]+]] = f32[2,3]{1,0} parameter(2)
; CHECK-DAG:     [[P3:%[^ ]+]] = f32[3,4]{1,0} parameter(3)
; CHECK-NEXT:    [[FIRST_GEMM:%[^ ]+]] = f32[2,4]{1,0} custom-call([[P0]], [[P1]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
; CHECK-NEXT:    ROOT [[SECOND_GEMM:%[^ ]+]] = f32[2,4]{1,0} custom-call([[P2]], [[P3]], [[FIRST_GEMM]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           output_to_operand_aliasing={{{{}: \(2, {}\)}}},
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
)");
}

TEST_F(CublasLtGemmRewriteTest, VectorBias) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[2,3] parameter(0)
  y = f32[3,4] parameter(1)
  z = f32[4] parameter(2)
  dot_a = f32[2,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  z_bcast = f32[2,4] broadcast(z), dimensions={1}
  ROOT out = f32[2,4] add(dot_a, z_bcast)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %test (x: f32[2,3], y: f32[3,4], z: f32[4]) -> f32[2,4] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[2,3]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[3,4]{1,0} parameter(1)
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[4]{0} parameter(2)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[2,4]{1,0} custom-call([[P0]], [[P1]], [[P2]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"BIAS"
; CHECK:           }
)");
}

// Epilogue Fusion disabled when GEMM has multiple users.
TEST_F(CublasLtGemmRewriteTest, VectorBiasMultipleUsers) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[4,4] parameter(0)
  y = f32[4,4] parameter(1)
  z = f32[4] parameter(2)
  c = f32[] constant(5)
  dot_a = f32[4,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}, operand_precision={highest,highest}
  z_bcast = f32[4,4] broadcast(z), dimensions={1}
  add_a = f32[4,4] add(dot_a, z_bcast)
  c_bcast = f32[4,4] broadcast(c), dimensions={}
  dot_b = f32[4,4] dot(dot_a, c_bcast), lhs_contracting_dims={1}, rhs_contracting_dims={0}, operand_precision={highest,highest}
  ROOT out = f32[4,4] dot(add_a, dot_b), lhs_contracting_dims={1}, rhs_contracting_dims={0}, operand_precision={highest,highest}
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK:        [[FUSED_COMPUTATION:%[^ ]+]] ([[DUMMY0:[^ ]+]]: f32[4,4], [[DUMMY1:[^ ]+]]: f32[4]) -> f32[4,4] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[4,4]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[4]{0} parameter(1)
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[4,4]{1,0} broadcast([[P1]]), dimensions={1}
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[4,4]{1,0} add([[P0]], [[P2]])
}

; CHECK-LABEL: ENTRY %test (x: f32[4,4], y: f32[4,4], z: f32[4]) -> f32[4,4] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[4,4]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[4,4]{1,0} parameter(1)
; CHECK-NEXT:    [[MATMUL0:%[^ ]+]] = f32[4,4]{1,0} custom-call([[P0]], [[P1]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["HIGHEST","HIGHEST"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[4]{0} parameter(2)
; CHECK-NEXT:    [[FUSION:%[^ ]+]] = f32[4,4]{1,0} fusion([[MATMUL0]], [[P2]]), kind=kLoop, calls=[[FUSED_COMPUTATION]]
; CHECK-NEXT:    [[C0:%[^ ]+]] = f32[] constant(5)
; CHECK-NEXT:    [[C0_BCAST:%[^ ]+]] = f32[4,4]{1,0} broadcast([[C0]]), dimensions={}
; CHECK-NEXT:    [[MATMUL1:%[^ ]+]] = f32[4,4]{1,0} custom-call([[MATMUL0]], [[C0_BCAST]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["HIGHEST","HIGHEST"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[4,4]{1,0} custom-call([[FUSION]], [[MATMUL1]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["HIGHEST","HIGHEST"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
      )");
}

TEST_F(CublasLtGemmRewriteTest, BatchedVectorBias) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[2,3,4] parameter(0)
  y = f32[4,5,6] parameter(1)
  z = f32[3,5,6] parameter(2)
  dot_a = f32[2,3,5,6] dot(x, y), lhs_contracting_dims={2}, rhs_contracting_dims={0}, operand_precision={highest,highest}
  z_bcast = f32[2,3,5,6] broadcast(z), dimensions={1,2,3}
  ROOT out = f32[2,3,5,6] add(dot_a, z_bcast)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: f32[2,3,4], y: f32[4,5,6], z: f32[3,5,6]) -> f32[2,3,5,6] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[2,3,4]{2,1,0} parameter(0)
; CHECK-NEXT:    [[P0_BITCAST:%[^ ]+]] = f32[6,4]{1,0} bitcast([[P0]])
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[4,5,6]{2,1,0} parameter(1)
; CHECK-NEXT:    [[P1_BITCAST:%[^ ]+]] = f32[4,30]{1,0}
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[3,5,6]{2,1,0} parameter(2)
; CHECK-NEXT:    [[BROADCAST:%[^ ]+]] = f32[2,3,5,6]{3,2,1,0} broadcast([[P2]]), dimensions={1,2,3}
; CHECK-NEXT:    [[BITCAST:%[^ ]+]] = f32[6,30]{1,0} bitcast([[BROADCAST]])
; CHECK-NEXT:    [[MATMUL:%[^ ]+]] = f32[6,30]{1,0} custom-call([[P0_BITCAST]], [[P1_BITCAST]], [[BITCAST]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           output_to_operand_aliasing={{[{][{]}}}: (2, {})},
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["HIGHEST","HIGHEST"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[2,3,5,6]{3,2,1,0} bitcast([[MATMUL]])
      )");
}

TEST_F(CublasLtGemmRewriteTest, BatchedSharedVectorBias) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[2,3,4] parameter(0)
  y = f32[4,5,6] parameter(1)
  z = f32[6] parameter(2)
  dot_a = f32[2,3,5,6] dot(x, y), lhs_contracting_dims={2}, rhs_contracting_dims={0}, operand_precision={highest,highest}
  z_bcast = f32[2,3,5,6] broadcast(z), dimensions={3}
  ROOT out = f32[2,3,5,6] add(dot_a, z_bcast)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: f32[2,3,4], y: f32[4,5,6], z: f32[6]) -> f32[2,3,5,6] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[2,3,4]{2,1,0} parameter(0)
; CHECK-NEXT:    [[P0_BITCAST:%[^ ]+]] = f32[6,4]{1,0} bitcast([[P0]])
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[4,5,6]{2,1,0} parameter(1)
; CHECK-NEXT:    [[P1_BITCAST:%[^ ]+]] = f32[4,30]{1,0}
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[6]{0} parameter(2)
; CHECK-NEXT:    [[BROADCAST:%[^ ]+]] = f32[2,3,5,6]{3,2,1,0} broadcast([[P2]]), dimensions={3}
; CHECK-NEXT:    [[BITCAST:%[^ ]+]] = f32[6,30]{1,0} bitcast([[BROADCAST]])
; CHECK-NEXT:    [[MATMUL:%[^ ]+]] = f32[6,30]{1,0} custom-call([[P0_BITCAST]], [[P1_BITCAST]], [[BITCAST]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           output_to_operand_aliasing={{[{][{]}}}: (2, {})},
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["HIGHEST","HIGHEST"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[2,3,5,6]{3,2,1,0} bitcast([[MATMUL]])
      )");
}

TEST_F(CublasLtGemmRewriteTest, VectorBiasIncorrectAxisFusedAsMatrix) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[2,3] parameter(0)
  y = f32[3,4] parameter(1)
  z = f32[2] parameter(2)
  dot_a = f32[2,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  z_bcast = f32[2,4] broadcast(z), dimensions={0}
  add = f32[2,4] add(dot_a, z_bcast)
  ROOT out = f32[4,2] transpose(add), dimensions={1,0}
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %test (x: f32[2,3], y: f32[3,4], z: f32[2]) -> f32[4,2] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[2,3]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[3,4]{1,0} parameter(1)
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[2]{0} parameter(2)
; CHECK-NEXT:    [[MATMUL:%[^ ]+]] = f32[2,4]{0,1} custom-call([[P0]], [[P1]], [[P2]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"BIAS"
; CHECK:           }
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[4,2]{1,0} bitcast([[MATMUL]])
)");
}

TEST_F(CublasLtGemmRewriteTest, VectorBiasSliced) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[4,3] parameter(0)
  y = f32[3,4] parameter(1)
  z = f32[3] parameter(2)
  dot_a = f32[4,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  slice_a = f32[2,3] slice(dot_a), slice={[0:2], [0:3]}
  z_bcast = f32[2,3] broadcast(z), dimensions={1}
  ROOT out = f32[2,3] add(slice_a, z_bcast)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: f32[4,3], y: f32[3,4], z: f32[3]) -> f32[2,3] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[4,3]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[3,4]{1,0} parameter(1)
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[3]{0} parameter(2)
; CHECK-NEXT:    [[MATMUL:%[^ ]+]] = f32[4,4]{1,0} custom-call([[P0]], [[P1]], [[P2]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"BIAS"
; CHECK:           }
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[2,3]{1,0} slice([[MATMUL]]), slice={[0:2], [0:3]}
      )");
}

// Epilogue Fusion disabled when slice has multiple users.
TEST_F(CublasLtGemmRewriteTest, VectorBiasSlicedMultipleUsers) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[2,3] parameter(0)
  y = f32[3,4] parameter(1)
  z = f32[2] parameter(2)
  c = f32[] constant(5)
  dot_a = f32[2,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  slice_a = f32[2,2] slice(dot_a), slice={[0:2], [0:2]}
  z_bcast = f32[2,2] broadcast(z), dimensions={1}
  add_a = f32[2,2] add(slice_a, z_bcast)
  c_bcast = f32[2,2] broadcast(c), dimensions={}
  dot_b = f32[2,2] dot(slice_a, c_bcast), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  ROOT out = f32[2,2] dot(add_a, dot_b), lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK:        [[FUSED_COMPUTATION:%[^ ]+]] ([[DUMMY0:[^ ]+]]: f32[2], [[DUMMY1:[^ ]+]]: f32[2,4]) -> f32[2,2] {
; CHECK-DAG:     [[P0:%[^ ]+]] = f32[2]{0} parameter(0)
; CHECK-DAG:     [[P1:%[^ ]+]] = f32[2,4]{1,0} parameter(1)
; CHECK-DAG:     [[SLICE:%[^ ]+]] = f32[2,2]{1,0} slice([[P1]]), slice={[0:2], [0:2]}
; CHECK-NEXT:    [[P0_BCAST:%[^ ]+]] = f32[2,2]{1,0} broadcast([[P0]]), dimensions={1}
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[2,2]{1,0} add([[SLICE]], [[P0_BCAST]])
}

; CHECK-LABEL: ENTRY %test (x: f32[2,3], y: f32[3,4], z: f32[2]) -> f32[2,2] {
; CHECK-DAG:     [[P0:%[^ ]+]] = f32[2,3]{1,0} parameter(0)
; CHECK-DAG:     [[P1:%[^ ]+]] = f32[3,4]{1,0} parameter(1)
; CHECK-DAG:     [[P2:%[^ ]+]] = f32[2]{0} parameter(2)
; CHECK-NEXT:    [[MATMUL0:%[^ ]+]] = f32[2,4]{1,0} custom-call([[P0]], [[P1]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
; CHECK-NEXT:    [[FUSION:%[^ ]+]] = f32[2,2]{1,0} fusion([[P2]], [[MATMUL0]]), kind=kLoop, calls=[[FUSED_COMPUTATION]]
; CHECK-NEXT:    [[SLICE:%[^ ]+]] = f32[2,2]{1,0} slice([[MATMUL0]]), slice={[0:2], [0:2]}
; CHECK-NEXT:    [[C0:%[^ ]+]] = f32[] constant(5)
; CHECK-NEXT:    [[C0_BCAST:%[^ ]+]] = f32[2,2]{1,0} broadcast([[C0]]), dimensions={}
; CHECK-NEXT:    [[MATMUL1:%[^ ]+]] = f32[2,2]{1,0} custom-call([[SLICE]], [[C0_BCAST]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[2,2]{1,0} custom-call([[FUSION]], [[MATMUL1]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
      )");
}

TEST_F(CublasLtGemmRewriteTest, VectorBiasTransposed) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[2,3] parameter(0)
  y = f32[3,4] parameter(1)
  z = f32[2] parameter(2)
  dot_a = f32[2,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  z_bcast = f32[2,4] parameter(3)
  ROOT out = f32[2,4] add(dot_a, z_bcast)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK:    [[P0:%[^ ]+]] = f32[2,3]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[3,4]{1,0} parameter(1)
; CHECK-NEXT:    [[P2_BCAST:%[^ ]+]] = f32[2,4]{1,0} parameter(3)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[2,4]{1,0} custom-call([[P0]], [[P1]], [[P2_BCAST]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
)");
}

TEST_F(CublasLtGemmRewriteTest, VectorBiasThenMatrixBias) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[2,3] parameter(0)
  y = f32[3,4] parameter(1)
  z = f32[4] parameter(2)
  z2 = f32[2,4] parameter(3)
  dot_a = f32[2,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  z_bcast = f32[2,4] broadcast(z), dimensions={1}
  add0 = f32[2,4] add(dot_a, z_bcast)
  ROOT add1 = f32[2,4] add(add0, z2)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %test (x: f32[2,3], y: f32[3,4], z: f32[4], z2: f32[2,4]) -> f32[2,4] {
; CHECK-DAG:     [[P0:%[^ ]+]] = f32[2,3]{1,0} parameter(0)
; CHECK-DAG:     [[P1:%[^ ]+]] = f32[3,4]{1,0} parameter(1)
; CHECK-DAG:     [[VECTOR_BIAS:%[^ ]+]] = f32[4]{0} parameter(2)
; CHECK-DAG:     [[MATRIX_BIAS:%[^ ]+]] = f32[2,4]{1,0} parameter(3)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[2,4]{1,0} custom-call([[P0]], [[P1]], [[MATRIX_BIAS]], [[VECTOR_BIAS]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"BIAS"
; CHECK:           }
)");
}

TEST_F(CublasLtGemmRewriteTest, BF16VectorBias) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = bf16[16,24] parameter(0)
  y = bf16[24,32] parameter(1)
  z = bf16[32] parameter(2)
  dot_a = bf16[16,32] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  z_bcast = bf16[16,32] broadcast(z), dimensions={1}
  ROOT out = bf16[16,32] add(dot_a, z_bcast)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{3e-3, 1e-3}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: bf16[16,24], y: bf16[24,32], z: bf16[32]) -> bf16[16,32] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = bf16[16,24]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = bf16[24,32]{1,0} parameter(1)
; CHECK-NEXT:    [[P2:%[^ ]+]] = bf16[32]{0} parameter(2)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = bf16[16,32]{1,0} custom-call([[P0]], [[P1]], [[P2]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"BIAS"
      )");
}

TEST_F(CublasLtGemmRewriteTest, BF16VectorBiasPadded) {
  if (!CudaOrRocmCheck(se::CudaComputeCapability::AMPERE, Switch::True)) {
    GTEST_SKIP() << "Padding of GEMM bf16 operands only implemented on "
                    "architectures with bf16 Tensor Cores.";
  }
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = bf16[2,3] parameter(0)
  y = bf16[3,4] parameter(1)
  z = bf16[4] parameter(2)
  dot_a = bf16[2,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  z_bcast = bf16[2,4] broadcast(z), dimensions={1}
  ROOT out = bf16[2,4] add(dot_a, z_bcast)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-3, 1e-3}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: bf16[2,3], y: bf16[3,4], z: bf16[4]) -> bf16[2,4] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = bf16[2,3]{1,0} parameter(0)
; CHECK-NEXT:    [[C0:%[^ ]+]] = bf16[] constant(0)
; CHECK-NEXT:    [[P0_PADDED:%[^ ]+]] = bf16[8,8]{1,0} pad([[P0]], [[C0]]), padding=0_6x0_5
; CHECK-NEXT:    [[P1:%[^ ]+]] = bf16[3,4]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_PADDED:%[^ ]+]] = bf16[8,8]{1,0} pad([[P1]], [[C0]]), padding=0_5x0_4
; CHECK-NEXT:    [[P2:%[^ ]+]] = bf16[4]{0} parameter(2)
; CHECK-NEXT:    [[MATMUL:%[^ ]+]] = bf16[8,8]{1,0} custom-call([[P0_PADDED]], [[P1_PADDED]], [[P2]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"BIAS"
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = bf16[2,4]{1,0} slice([[MATMUL]]), slice={[0:2], [0:4]}
      )");
}

TEST_F(CublasLtGemmRewriteTest, ReluActivation) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[2,3] parameter(0)
  y = f32[3,4] parameter(1)
  dot_a = f32[2,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  c = f32[] constant(0)
  c_bcast = f32[2,4] broadcast(c), dimensions={}
  ROOT out = f32[2,4] maximum(dot_a, c_bcast)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: f32[2,3], y: f32[3,4]) -> f32[2,4] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[2,3]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[3,4]{1,0} parameter(1)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[2,4]{1,0} custom-call([[P0]], [[P1]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"RELU"
; CHECK:           }
      )");
}

TEST_F(CublasLtGemmRewriteTest, BatchedReluActivation) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[2,3,4] parameter(0)
  y = f32[4,5,6] parameter(1)
  dot_a = f32[2,3,5,6] dot(x, y), lhs_contracting_dims={2}, rhs_contracting_dims={0}, operand_precision={highest,highest}
  c = f32[] constant(0)
  c_bcast = f32[2,3,5,6] broadcast(c), dimensions={}
  ROOT out = f32[2,3,5,6] maximum(dot_a, c_bcast)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: f32[2,3,4], y: f32[4,5,6]) -> f32[2,3,5,6] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[2,3,4]{2,1,0} parameter(0)
; CHECK-NEXT:    [[P0_BITCAST:%[^ ]+]] = f32[6,4]{1,0} bitcast([[P0]])
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[4,5,6]{2,1,0} parameter(1)
; CHECK-NEXT:    [[P1_BITCAST:%[^ ]+]] = f32[4,30]{1,0}
; CHECK-NEXT:    [[MATMUL:%[^ ]+]] = f32[6,30]{1,0} custom-call([[P0_BITCAST]], [[P1_BITCAST]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["HIGHEST","HIGHEST"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"RELU"
; CHECK:           }
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[2,3,5,6]{3,2,1,0} bitcast([[MATMUL]])
      )");
}

TEST_F(CublasLtGemmRewriteTest, ReluActivationSliced) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[2,3] parameter(0)
  y = f32[3,4] parameter(1)
  dot_a = f32[2,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  c = f32[] constant(0)
  c_bcast = f32[2,2] broadcast(c), dimensions={}
  slice_a = f32[2,2] slice(dot_a), slice={[0:2], [0:2]}
  ROOT out = f32[2,2] maximum(slice_a, c_bcast)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: f32[2,3], y: f32[3,4]) -> f32[2,2] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[2,3]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[3,4]{1,0} parameter(1)
; CHECK-NEXT:    [[MATMUL:%[^ ]+]] = f32[2,4]{1,0} custom-call([[P0]], [[P1]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"RELU"
; CHECK:           }
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[2,2]{1,0} slice([[MATMUL]]), slice={[0:2], [0:2]}
      )");
}

TEST_F(CublasLtGemmRewriteTest, MatrixBiasReluActivation) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[2,3] parameter(0)
  y = f32[3,4] parameter(1)
  z = f32[2,4] parameter(2)
  dot_a = f32[2,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  add = f32[2,4] add(dot_a, z)
  c = f32[] constant(0)
  c_bcast = f32[2,4] broadcast(c), dimensions={}
  ROOT out = f32[2,4] maximum(add, c_bcast)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: f32[2,3], y: f32[3,4], z: f32[2,4]) -> f32[2,4] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[2,3]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[3,4]{1,0} parameter(1)
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[2,4]{1,0} parameter(2)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[2,4]{1,0} custom-call([[P0]], [[P1]], [[P2]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"RELU"
; CHECK:           }
      )");
}

TEST_F(CublasLtGemmRewriteTest, SquareMatrixBiasReluActivation) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[4,4] parameter(0)
  y = f32[4,4] parameter(1)
  z = f32[4,4] parameter(2)
  dot_a = f32[4,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  add = f32[4,4] add(dot_a, z)
  c = f32[] constant(0)
  c_bcast = f32[4,4] broadcast(c), dimensions={}
  ROOT out = f32[4,4] maximum(add, c_bcast)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: f32[4,4], y: f32[4,4], z: f32[4,4]) -> f32[4,4] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[4,4]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[4,4]{1,0} parameter(1)
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[4,4]{1,0} parameter(2)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[4,4]{1,0} custom-call([[P0]], [[P1]], [[P2]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"RELU"
; CHECK:           }
      )");
}

TEST_F(CublasLtGemmRewriteTest, VectorBiasReluActivation) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[2,3] parameter(0)
  y = f32[3,4] parameter(1)
  z = f32[4] parameter(2)
  dot_a = f32[2,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  z_bcast = f32[2,4] broadcast(z), dimensions={1}
  add = f32[2,4] add(dot_a, z_bcast)
  c = f32[] constant(0)
  c_bcast = f32[2,4] broadcast(c), dimensions={}
  ROOT out = f32[2,4] maximum(add, c_bcast)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: f32[2,3], y: f32[3,4], z: f32[4]) -> f32[2,4] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[2,3]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[3,4]{1,0} parameter(1)
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[4]{0} parameter(2)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[2,4]{1,0} custom-call([[P0]], [[P1]], [[P2]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"BIAS_RELU"
; CHECK:           }
      )");
}

TEST_F(CublasLtGemmRewriteTest, BatchedVectorBiasReluActivation) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[2,3,4] parameter(0)
  y = f32[4,5,6] parameter(1)
  z = f32[3,5,6] parameter(2)
  dot_a = f32[2,3,5,6] dot(x, y), lhs_contracting_dims={2}, rhs_contracting_dims={0}, operand_precision={highest,highest}
  z_bcast = f32[2,3,5,6] broadcast(z), dimensions={1,2,3}
  add = f32[2,3,5,6] add(dot_a, z_bcast)
  c = f32[] constant(0)
  c_bcast = f32[2,3,5,6] broadcast(c), dimensions={}
  ROOT out = f32[2,3,5,6] maximum(add, c_bcast)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: f32[2,3,4], y: f32[4,5,6], z: f32[3,5,6]) -> f32[2,3,5,6] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[2,3,4]{2,1,0} parameter(0)
; CHECK-NEXT:    [[P0_BITCAST:%[^ ]+]] = f32[6,4]{1,0} bitcast([[P0]])
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[4,5,6]{2,1,0} parameter(1)
; CHECK-NEXT:    [[P1_BITCAST:%[^ ]+]] = f32[4,30]{1,0}
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[3,5,6]{2,1,0} parameter(2)
; CHECK-NEXT:    [[BROADCAST:%[^ ]+]] = f32[2,3,5,6]{3,2,1,0} broadcast([[P2]]), dimensions={1,2,3}
; CHECK-NEXT:    [[BITCAST:%[^ ]+]] = f32[6,30]{1,0} bitcast([[BROADCAST]])
; CHECK-NEXT:    [[MATMUL:%[^ ]+]] = f32[6,30]{1,0} custom-call([[P0_BITCAST]], [[P1_BITCAST]], [[BITCAST]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["HIGHEST","HIGHEST"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"RELU"
; CHECK:           }
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[2,3,5,6]{3,2,1,0} bitcast([[MATMUL]])
      )");
}

TEST_F(CublasLtGemmRewriteTest, VectorBiasTransposedReluActivation) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[2,3] parameter(0)
  y = f32[3,4] parameter(1)
  z = f32[2] parameter(2)
  dot_a = f32[2,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  z_bcast = f32[2,4] broadcast(z), dimensions={0}
  add = f32[2,4] add(dot_a, z_bcast)
  c = f32[] constant(0)
  c_bcast = f32[2,4] broadcast(c), dimensions={}
  maximum = f32[2,4] maximum(add, c_bcast)
  ROOT out = f32[4,2] transpose(maximum), dimensions={1,0}
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: f32[2,3], y: f32[3,4], z: f32[2]) -> f32[4,2] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[2,3]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[3,4]{1,0} parameter(1)
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[2]{0} parameter(2)
; CHECK-NEXT:    [[MATMUL:%[^ ]+]] = f32[2,4]{0,1} custom-call([[P0]], [[P1]], [[P2]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:       "alpha_real":1
; CHECK-DAG:       "alpha_imag":0
; CHECK-DAG:       "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"BIAS_RELU"
; CHECK:           }
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[4,2]{1,0} bitcast([[MATMUL]])
      )");
}

TEST_F(CublasLtGemmRewriteTest, VectorBiasThenMatrixBiasReluActivation) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[2,3] parameter(0)
  y = f32[3,4] parameter(1)
  z_vec = f32[4] parameter(2)
  z_matrix = f32[2,4] parameter(3)
  dot_a = f32[2,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  z_bcast = f32[2,4] broadcast(z_vec), dimensions={1}
  add0 = f32[2,4] add(dot_a, z_bcast)
  add1 = f32[2,4] add(add0, z_matrix)
  c = f32[] constant(0)
  c_bcast = f32[2,4] broadcast(c), dimensions={}
  ROOT out = f32[2,4] maximum(add1, c_bcast)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: f32[2,3], y: f32[3,4], z_vec: f32[4], z_matrix: f32[2,4]) -> f32[2,4] {
; CHECK-DAG:     [[P0:%[^ ]+]] = f32[2,3]{1,0} parameter(0)
; CHECK-DAG:     [[P1:%[^ ]+]] = f32[3,4]{1,0} parameter(1)
; CHECK-DAG:     [[P2:%[^ ]+]] = f32[4]{0} parameter(2)
; CHECK-DAG:     [[P3:%[^ ]+]] = f32[2,4]{1,0} parameter(3)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[2,4]{1,0} custom-call([[P0]], [[P1]], [[P3]], [[P2]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"BIAS_RELU"
; CHECK:           }
      )");
}

TEST_F(CublasLtGemmRewriteTest, ApproxGeluActivation) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[2,3] parameter(0)
  y = f32[3,4] parameter(1)
  dot = f32[2,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  mul.0 = f32[2,4] multiply(dot, dot)
  mul.1 = f32[2,4] multiply(dot, mul.0)
  const.0 = f32[] constant(0.044715)
  bcast.0 = f32[2,4] broadcast(const.0), dimensions={}
  mul.2 = f32[2,4] multiply(mul.1, bcast.0)
  add.0 = f32[2,4] add(dot, mul.2)
  const.1 = f32[] constant(0.797884583)
  bcast.1 = f32[2,4] broadcast(const.1), dimensions={}
  mul.3 = f32[2,4] multiply(add.0, bcast.1)
  tanh = f32[2,4] tanh(mul.3)
  const.2 = f32[] constant(1)
  bcast.2 = f32[2,4] broadcast(const.2), dimensions={}
  add.2 = f32[2,4] add(tanh, bcast.2)
  const.3 = f32[] constant(0.5)
  bcast.3 = f32[2,4] broadcast(const.3), dimensions={}
  mul.4 = f32[2,4] multiply(add.2, bcast.3)
  ROOT out = f32[2,4] multiply(dot, mul.4)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: f32[2,3], y: f32[3,4]) -> f32[2,4] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[2,3]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[3,4]{1,0} parameter(1)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[2,4]{1,0} custom-call([[P0]], [[P1]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"GELU"
; CHECK:           }
      )");
}

TEST_F(CublasLtGemmRewriteTest, ApproxGeluActivationWrongConstant) {
  // Modify one constant slightly, so it should no longer pattern match.
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[2,3] parameter(0)
  y = f32[3,4] parameter(1)
  dot = f32[2,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  mul.0 = f32[2,4] multiply(dot, dot)
  mul.1 = f32[2,4] multiply(dot, mul.0)
  const.0 = f32[] constant(0.05)
  bcast.0 = f32[2,4] broadcast(const.0), dimensions={}
  mul.2 = f32[2,4] multiply(mul.1, bcast.0)
  add.0 = f32[2,4] add(dot, mul.2)
  const.1 = f32[] constant(0.797884583)
  bcast.1 = f32[2,4] broadcast(const.1), dimensions={}
  mul.3 = f32[2,4] multiply(add.0, bcast.1)
  tanh = f32[2,4] tanh(mul.3)
  const.2 = f32[] constant(1)
  bcast.2 = f32[2,4] broadcast(const.2), dimensions={}
  add.2 = f32[2,4] add(tanh, bcast.2)
  const.3 = f32[] constant(0.5)
  bcast.3 = f32[2,4] broadcast(const.3), dimensions={}
  mul.4 = f32[2,4] multiply(add.2, bcast.3)
  ROOT out = f32[2,4] multiply(dot, mul.4)
}

)";

  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-NOT: GELU
      )");
}

TEST_F(CublasLtGemmRewriteTest, VectorBiasThenApproxGeluActivation) {
  if (CudaOrRocmCheck(Switch::False, Switch::True)) {
    GTEST_SKIP() << "TODO: Unsupported blas-lt epilogue on ROCM";
  }
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[2,3] parameter(0)
  y = f32[3,4] parameter(1)
  z = f32[4] parameter(2)
  dot = f32[2,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  z_bcast = f32[2,4] broadcast(z), dimensions={1}
  add = f32[2,4] add(dot, z_bcast)
  mul.0 = f32[2,4] multiply(add, add)
  mul.1 = f32[2,4] multiply(add, mul.0)
  const.0 = f32[] constant(0.044715)
  bcast.0 = f32[2,4] broadcast(const.0), dimensions={}
  mul.2 = f32[2,4] multiply(mul.1, bcast.0)
  add.0 = f32[2,4] add(add, mul.2)
  const.1 = f32[] constant(0.797884583)
  bcast.1 = f32[2,4] broadcast(const.1), dimensions={}
  mul.3 = f32[2,4] multiply(add.0, bcast.1)
  tanh = f32[2,4] tanh(mul.3)
  const.2 = f32[] constant(1)
  bcast.2 = f32[2,4] broadcast(const.2), dimensions={}
  add.2 = f32[2,4] add(tanh, bcast.2)
  const.3 = f32[] constant(0.5)
  bcast.3 = f32[2,4] broadcast(const.3), dimensions={}
  mul.4 = f32[2,4] multiply(add.2, bcast.3)
  ROOT out = f32[2,4] multiply(add, mul.4)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: f32[2,3], y: f32[3,4], z: f32[4]) -> f32[2,4] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[2,3]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[3,4]{1,0} parameter(1)
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[4]{0} parameter(2)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[2,4]{1,0} custom-call([[P0]], [[P1]], [[P2]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"BIAS_GELU"
; CHECK:           }
      )");
}

TEST_F(CublasLtGemmRewriteTest, ApproxGeluActivationWithAux) {
  if (CudaOrRocmCheck(Switch::False, Switch::True)) {
    GTEST_SKIP() << "TODO: Unsupported blas-lt epilogue on ROCM";
  }
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[2,3] parameter(0)
  y = f32[3,4] parameter(1)
  dot = f32[2,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  mul.0 = f32[2,4] multiply(dot, dot)
  mul.1 = f32[2,4] multiply(dot, mul.0)
  const.0 = f32[] constant(0.044715)
  bcast.0 = f32[2,4] broadcast(const.0), dimensions={}
  mul.2 = f32[2,4] multiply(mul.1, bcast.0)
  add.0 = f32[2,4] add(dot, mul.2)
  const.1 = f32[] constant(0.797884583)
  bcast.1 = f32[2,4] broadcast(const.1), dimensions={}
  mul.3 = f32[2,4] multiply(add.0, bcast.1)
  tanh = f32[2,4] tanh(mul.3)
  const.2 = f32[] constant(1)
  bcast.2 = f32[2,4] broadcast(const.2), dimensions={}
  add.2 = f32[2,4] add(tanh, bcast.2)
  const.3 = f32[] constant(0.5)
  bcast.3 = f32[2,4] broadcast(const.3), dimensions={}
  mul.4 = f32[2,4] multiply(add.2, bcast.3)
  mul.5 = f32[2,4] multiply(dot, mul.4)
  ROOT out = (f32[2,4], f32[2,4]) tuple(mul.5, dot)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: f32[2,3], y: f32[3,4]) -> (f32[2,4], f32[2,4]) {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[2,3]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[3,4]{1,0} parameter(1)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = (f32[2,4]{1,0}, f32[2,4]{1,0}) custom-call([[P0]], [[P1]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"GELU_AUX"
; CHECK:           }
      )");
}

TEST_F(CublasLtGemmRewriteTest, VectorBiasThenApproxGeluActivationWithAux) {
  if (CudaOrRocmCheck(Switch::False, Switch::True)) {
    GTEST_SKIP() << "TODO: Unsupported blas-lt epilogue on ROCM";
  }
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[2,3] parameter(0)
  y = f32[3,4] parameter(1)
  z = f32[4] parameter(2)
  dot = f32[2,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  z_bcast = f32[2,4] broadcast(z), dimensions={1}
  add = f32[2,4] add(dot, z_bcast)
  mul.0 = f32[2,4] multiply(add, add)
  mul.1 = f32[2,4] multiply(add, mul.0)
  const.0 = f32[] constant(0.044715)
  bcast.0 = f32[2,4] broadcast(const.0), dimensions={}
  mul.2 = f32[2,4] multiply(mul.1, bcast.0)
  add.0 = f32[2,4] add(add, mul.2)
  const.1 = f32[] constant(0.797884583)
  bcast.1 = f32[2,4] broadcast(const.1), dimensions={}
  mul.3 = f32[2,4] multiply(add.0, bcast.1)
  tanh = f32[2,4] tanh(mul.3)
  const.2 = f32[] constant(1)
  bcast.2 = f32[2,4] broadcast(const.2), dimensions={}
  add.2 = f32[2,4] add(tanh, bcast.2)
  const.3 = f32[] constant(0.5)
  bcast.3 = f32[2,4] broadcast(const.3), dimensions={}
  mul.4 = f32[2,4] multiply(add.2, bcast.3)
  mul.5 = f32[2,4] multiply(add, mul.4)
  ROOT out = (f32[2,4], f32[2,4]) tuple(mul.5, add)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: f32[2,3], y: f32[3,4], z: f32[4]) -> (f32[2,4], f32[2,4]) {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[2,3]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[3,4]{1,0} parameter(1)
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[4]{0} parameter(2)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = (f32[2,4]{1,0}, f32[2,4]{1,0}) custom-call([[P0]], [[P1]], [[P2]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"BIAS_GELU_AUX"
; CHECK:           }
      )");
}

TEST_F(CublasLtGemmRewriteTest, ApproxGeluActivationBF16) {
  if (!CudaOrRocmCheck(se::CudaComputeCapability::AMPERE, Switch::True)) {
    GTEST_SKIP() << "Padding of GEMM bf16 operands only implemented on "
                    "architectures with bf16 Tensor Cores.";
  }
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = bf16[2,3] parameter(0)
  y = bf16[3,4] parameter(1)
  dot = bf16[2,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  mul.0 = bf16[2,4] multiply(dot, dot)
  mul.1 = bf16[2,4] multiply(dot, mul.0)
  const.0 = bf16[] constant(0.044715)
  bcast.0 = bf16[2,4] broadcast(const.0), dimensions={}
  mul.2 = bf16[2,4] multiply(mul.1, bcast.0)
  add.0 = bf16[2,4] add(dot, mul.2)
  const.1 = bf16[] constant(0.797884583)
  bcast.1 = bf16[2,4] broadcast(const.1), dimensions={}
  mul.3 = bf16[2,4] multiply(add.0, bcast.1)
  tanh = bf16[2,4] tanh(mul.3)
  const.2 = bf16[] constant(1)
  bcast.2 = bf16[2,4] broadcast(const.2), dimensions={}
  add.2 = bf16[2,4] add(tanh, bcast.2)
  const.3 = bf16[] constant(0.5)
  bcast.3 = bf16[2,4] broadcast(const.3), dimensions={}
  mul.4 = bf16[2,4] multiply(add.2, bcast.3)
  ROOT out = bf16[2,4] multiply(dot, mul.4)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{5e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: bf16[2,3], y: bf16[3,4]) -> bf16[2,4] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = bf16[2,3]{1,0} parameter(0)
; CHECK-NEXT:    [[C0:%[^ ]+]] = bf16[] constant(0)
; CHECK-NEXT:    [[P0_PAD:%[^ ]+]] = bf16[8,8]{1,0} pad([[P0]], [[C0]]), padding=0_6x0_5
; CHECK-NEXT:    [[P1:%[^ ]+]] = bf16[3,4]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_PAD:%[^ ]+]] = bf16[8,8]{1,0} pad([[P1]], [[C0]]), padding=0_5x0_4
; CHECK-NEXT:    [[DOT:%[^ ]+]] = bf16[8,8]{1,0} custom-call([[P0_PAD]], [[P1_PAD]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"GELU"
; CHECK:           }
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = bf16[2,4]{1,0} slice([[DOT]]), slice={[0:2], [0:4]}
      )");
}

TEST_F(CublasLtGemmRewriteTest, ApproxGeluActivationBitcast) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[2,3] parameter(0)
  y = f32[3,4] parameter(1)
  dot = f32[2,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  dot_bitcast = f32[2,2,2] bitcast(dot)
  mul.0 = f32[2,2,2] multiply(dot_bitcast, dot_bitcast)
  mul.1 = f32[2,2,2] multiply(dot_bitcast, mul.0)
  const.0 = f32[] constant(0.044715)
  bcast.0 = f32[2,2,2] broadcast(const.0), dimensions={}
  mul.2 = f32[2,2,2] multiply(mul.1, bcast.0)
  add.0 = f32[2,2,2] add(dot_bitcast, mul.2)
  const.1 = f32[] constant(0.797884583)
  bcast.1 = f32[2,2,2] broadcast(const.1), dimensions={}
  mul.3 = f32[2,2,2] multiply(add.0, bcast.1)
  tanh = f32[2,2,2] tanh(mul.3)
  const.2 = f32[] constant(1)
  bcast.2 = f32[2,2,2] broadcast(const.2), dimensions={}
  add.2 = f32[2,2,2] add(tanh, bcast.2)
  const.3 = f32[] constant(0.5)
  bcast.3 = f32[2,2,2] broadcast(const.3), dimensions={}
  mul.4 = f32[2,2,2] multiply(add.2, bcast.3)
  ROOT out = f32[2,2,2] multiply(dot_bitcast, mul.4)
}

)";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  GemmRewriter pass(GpuComputeComp());
  TF_ASSERT_OK_AND_ASSIGN(bool changed, this->RunHloPass(&pass, module.get()));
  EXPECT_TRUE(changed);

  EXPECT_THAT(module->entry_computation()->root_instruction(),
              GmockMatch(m::Bitcast(m::CustomCall(
                                        {"__cublas$lt$matmul"},
                                        m::Parameter(0).WithShape(F32, {2, 3}),
                                        m::Parameter(1).WithShape(F32, {3, 4})))
                             .WithShape(F32, {2, 2, 2})));
}

// For F16, the sizes of all dimensions of the operands are required to be
// multiples of 8 to allow matrix bias fusion.
TEST_F(CublasLtGemmRewriteTest, MatrixBiasF16) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f16[8,16] parameter(0)
  y = f16[16,8] parameter(1)
  z = f16[8,8] parameter(2)
  dot_a = f16[8,8] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  ROOT out = f16[8,8] add(dot_a, z)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-3, 1e-3}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: f16[8,16], y: f16[16,8], z: f16[8,8]) -> f16[8,8] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f16[8,16]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f16[16,8]{1,0} parameter(1)
; CHECK-NEXT:    [[P2:%[^ ]+]] = f16[8,8]{1,0} parameter(2)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f16[8,8]{1,0} custom-call([[P0]], [[P1]], [[P2]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
      )");
}

TEST_F(CublasLtGemmRewriteTest, VectorBiasF32UnpaddedWithBitcast) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[2,3]{1,0} parameter(0)
  y = f32[3,4]{1,0} parameter(1)
  z = f32[2]{0} parameter(2)
  dot_a = f32[2,4]{0,1} dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  bitc = f32[4,2]{1,0} bitcast(f32[2,4]{0,1} dot_a)
  z_bcast = f32[4,2] broadcast(z), dimensions={1}
  ROOT add = f32[4,2]{1,0} add(bitc, z_bcast)
}

)";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  GemmRewriter pass(GpuComputeComp());
  TF_ASSERT_OK_AND_ASSIGN(bool changed, this->RunHloPass(&pass, module.get()));
  EXPECT_TRUE(changed);

  EXPECT_THAT(
      module->entry_computation()->root_instruction(),
      GmockMatch(m::Bitcast(m::CustomCall({"__cublas$lt$matmul"},
                                          m::Parameter(0), m::Parameter(1),
                                          m::Parameter(2).WithShape(F32, {2}))
                                .WithShape(F32, {2, 4}))
                     .WithShape(F32, {4, 2})));
}

// For F16, the operands are padded on GPUs with Tensor Cores (i.e. Volta and
// newer architectures) so that the sizes of all dimensions are multiples of 8.
TEST_F(CublasLtGemmRewriteTest, VectorBiasF16Unpadded) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f16[8,16] parameter(0)
  y = f16[16,8] parameter(1)
  z = f16[8] parameter(2)
  dot_a = f16[8,8] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  z_bcast = f16[8,8] broadcast(z), dimensions={1}
  ROOT add = f16[8,8] add(dot_a, z_bcast)
}

)";
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{8e-3, 2e-3}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: f16[8,16], y: f16[16,8], z: f16[8]) -> f16[8,8] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f16[8,16]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f16[16,8]{1,0} parameter(1)
; CHECK-NEXT:    [[P2:%[^ ]+]] = f16[8]{0} parameter(2)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f16[8,8]{1,0} custom-call([[P0]], [[P1]], [[P2]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"BIAS"
; CHECK:           }
      )");
}

TEST_F(CublasLtGemmRewriteTest, VectorBiasF16Padded) {
  if (!CudaOrRocmCheck(se::CudaComputeCapability::VOLTA, Switch::True)) {
    GTEST_SKIP() << "Padding of GEMM operands only implemented on "
                    "architectures with Tensor Cores.";
  }
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f16[6,12] parameter(0)
  y = f16[12,6] parameter(1)
  z = f16[6] parameter(2)
  dot_a = f16[6,6] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  z_bcast = f16[6,6] broadcast(z), dimensions={1}
  ROOT add = f16[6,6] add(dot_a, z_bcast)
}

)";
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-3, 1e-3}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: f16[6,12], y: f16[12,6], z: f16[6]) -> f16[6,6] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f16[6,12]{1,0} parameter(0)
; CHECK-NEXT:    [[C0:%[^ ]+]] = f16[] constant(0)
; CHECK-NEXT:    [[P0_PADDED:%[^ ]+]] = f16[8,16]{1,0} pad([[P0]], [[C0]]), padding=0_2x0_4
; CHECK-NEXT:    [[P1:%[^ ]+]] = f16[12,6]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_PADDED:%[^ ]+]] = f16[16,8]{1,0} pad([[P1]], [[C0]]), padding=0_4x0_2
; CHECK-NEXT:    [[P2:%[^ ]+]] = f16[6]{0} parameter(2)
; CHECK-NEXT:    [[MATMUL:%[^ ]+]] = f16[8,8]{1,0} custom-call([[P0_PADDED]], [[P1_PADDED]], [[P2]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"BIAS"
; CHECK:           }
; CHECK-NEXT:    [[OUT:%[^ ]+]] = f16[6,6]{1,0} slice([[MATMUL]]), slice={[0:6], [0:6]}
      )");
}

// For F16, the operands are padded on GPUs with Tensor Cores (i.e. Volta and
// newer architectures) so that the sizes of all dimensions are multiples of 8.
TEST_F(CublasLtGemmRewriteTest, ReluActivationF16Unpadded) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f16[8,16] parameter(0)
  y = f16[16,8] parameter(1)
  dot_a = f16[8,8] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  c = f16[] constant(0)
  c_bcast = f16[8,8] broadcast(c), dimensions={}
  ROOT out = f16[8,8] maximum(dot_a, c_bcast)
}

)";
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-3, 1e-3}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: f16[8,16], y: f16[16,8]) -> f16[8,8] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f16[8,16]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f16[16,8]{1,0} parameter(1)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f16[8,8]{1,0} custom-call([[P0]], [[P1]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"RELU"
; CHECK:           }
      )");
}

TEST_F(CublasLtGemmRewriteTest, ReluActivationF16Padded) {
  if (!CudaOrRocmCheck(se::CudaComputeCapability::VOLTA, Switch::True)) {
    GTEST_SKIP() << "Padding of GEMM operands only implemented on "
                    "architectures with Tensor Cores.";
  }
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f16[6,12] parameter(0)
  y = f16[12,6] parameter(1)
  dot_a = f16[6,6] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  c = f16[] constant(0)
  c_bcast = f16[6,6] broadcast(c), dimensions={}
  ROOT out = f16[6,6] maximum(dot_a, c_bcast)
}

)";
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: f16[6,12], y: f16[12,6]) -> f16[6,6] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f16[6,12]{1,0} parameter(0)
; CHECK-NEXT:    [[C0:%[^ ]+]] = f16[] constant(0)
; CHECK-NEXT:    [[P0_PADDED:%[^ ]+]] = f16[8,16]{1,0} pad([[P0]], [[C0]]), padding=0_2x0_4
; CHECK-NEXT:    [[P1:%[^ ]+]] = f16[12,6]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_PADDED:%[^ ]+]] = f16[16,8]{1,0} pad([[P1]], [[C0]]), padding=0_4x0_2
; CHECK-NEXT:    [[MATMUL:%[^ ]+]] = f16[8,8]{1,0} custom-call([[P0_PADDED]], [[P1_PADDED]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"RELU"
; CHECK:           }
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f16[6,6]{1,0} slice([[MATMUL]]), slice={[0:6], [0:6]}
      )");
}

TEST_F(CublasLtGemmRewriteTest, MatrixBiasReluActivationF16) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f16[8,16] parameter(0)
  y = f16[16,8] parameter(1)
  z = f16[8,8] parameter(2)
  dot_a = f16[8,8] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  add = f16[8,8] add(dot_a, z)
  c = f16[] constant(0)
  c_bcast = f16[8,8] broadcast(c), dimensions={}
  ROOT out = f16[8,8] maximum(add, c_bcast)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-3, 1e-3}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: f16[8,16], y: f16[16,8], z: f16[8,8]) -> f16[8,8] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f16[8,16]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f16[16,8]{1,0} parameter(1)
; CHECK-NEXT:    [[P2:%[^ ]+]] = f16[8,8]{1,0} parameter(2)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f16[8,8]{1,0} custom-call([[P0]], [[P1]], [[P2]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"RELU"
; CHECK:           }
      )");
}

// For F16, the operands are padded on GPUs with Tensor Cores (i.e. Volta and
// newer architectures) so that the sizes of all dimensions are multiples of 8.
TEST_F(CublasLtGemmRewriteTest, VectorBiasReluActivationF16Unpadded) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f16[8,16] parameter(0)
  y = f16[16,8] parameter(1)
  z = f16[8] parameter(2)
  dot_a = f16[8,8] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  z_bcast = f16[8,8] broadcast(z), dimensions={1}
  add = f16[8,8] add(dot_a, z_bcast)
  c = f16[] constant(0)
  c_bcast = f16[8,8] broadcast(c), dimensions={}
  ROOT out = f16[8,8] maximum(add, c_bcast)
}

)";
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-3, 1e-3}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: f16[8,16], y: f16[16,8], z: f16[8]) -> f16[8,8] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f16[8,16]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f16[16,8]{1,0} parameter(1)
; CHECK-NEXT:    [[P2:%[^ ]+]] = f16[8]{0} parameter(2)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f16[8,8]{1,0} custom-call([[P0]], [[P1]], [[P2]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"BIAS_RELU"
; CHECK:           }
      )");
}

TEST_F(CublasLtGemmRewriteTest, VectorBiasReluActivationF16Padded) {
  if (!CudaOrRocmCheck(se::CudaComputeCapability::VOLTA, Switch::True)) {
    GTEST_SKIP() << "Padding of GEMM operands only implemented on "
                    "architectures with Tensor Cores.";
  }
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f16[6,12] parameter(0)
  y = f16[12,6] parameter(1)
  z = f16[6] parameter(2)
  dot_a = f16[6,6] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  z_bcast = f16[6,6] broadcast(z), dimensions={1}
  add = f16[6,6] add(dot_a, z_bcast)
  c = f16[] constant(0)
  c_bcast = f16[6,6] broadcast(c), dimensions={}
  ROOT out = f16[6,6] maximum(add, c_bcast)
}

)";
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-3, 1e-3}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: f16[6,12], y: f16[12,6], z: f16[6]) -> f16[6,6] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f16[6,12]{1,0} parameter(0)
; CHECK-NEXT:    [[C0:%[^ ]+]] = f16[] constant(0)
; CHECK-NEXT:    [[P0_PADDED:%[^ ]+]] = f16[8,16]{1,0} pad([[P0]], [[C0]]), padding=0_2x0_4
; CHECK-NEXT:    [[P1:%[^ ]+]] = f16[12,6]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_PADDED:%[^ ]+]] = f16[16,8]{1,0} pad([[P1]], [[C0]]), padding=0_4x0_2
; CHECK-NEXT:    [[P2:%[^ ]+]] = f16[6]{0} parameter(2)
; CHECK-NEXT:    [[MATMUL:%[^ ]+]] = f16[8,8]{1,0} custom-call([[P0_PADDED]], [[P1_PADDED]], [[P2]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"BIAS_RELU"
; CHECK:           }
      )");
}

// For bfloat16, the sizes of all dimensions of the operands are required to be
// multiples of 8 to allow matrix bias fusion.
TEST_F(CublasLtGemmRewriteTest, MatrixBiasBF16) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = bf16[8,16] parameter(0)
  y = bf16[16,8] parameter(1)
  z = bf16[8,8] parameter(2)
  dot_a = bf16[8,8] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  ROOT out = bf16[8,8] add(dot_a, z)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-3, 1e-3}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: bf16[8,16], y: bf16[16,8], z: bf16[8,8]) -> bf16[8,8] {
; CHECK-DAG:     [[P0:%[^ ]+]] = bf16[8,16]{1,0} parameter(0)
; CHECK-DAG:     [[P1:%[^ ]+]] = bf16[16,8]{1,0} parameter(1)
; CHECK-DAG:     [[P2:%[^ ]+]] = bf16[8,8]{1,0} parameter(2)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = bf16[8,8]{1,0} custom-call([[P0]], [[P1]], [[P2]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
      )");
}

TEST_F(CublasLtGemmRewriteTest, MatrixBiasBitcastBF16) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = bf16[8,16] parameter(0)
  y = bf16[16,8] parameter(1)
  bias = bf16[2,4,8] parameter(2)
  dot = bf16[8,8] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  bitcast = bf16[2,4,8] bitcast(dot)
  ROOT out = bf16[2,4,8] add(bitcast, bias)
}

)";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  GemmRewriter pass(GpuComputeComp());
  TF_ASSERT_OK_AND_ASSIGN(bool changed, this->RunHloPass(&pass, module.get()));
  EXPECT_TRUE(changed);

  EXPECT_THAT(
      module->entry_computation()->root_instruction(),
      GmockMatch(
          m::Bitcast(m::CustomCall(
                         {"__cublas$lt$matmul"},
                         m::Parameter(0).WithShape(BF16, {8, 16}),
                         m::Parameter(1).WithShape(BF16, {16, 8}),
                         m::Bitcast(m::Parameter(2)).WithShape(BF16, {8, 8})))
              .WithShape(BF16, {2, 4, 8})));
}

// For bfloat16, the operands are padded if necessary on Ampere and newer
// architectures so that the sizes of all dimensions are multiples of 8.
TEST_F(CublasLtGemmRewriteTest, VectorBiasBF16Unpadded) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = bf16[8,16] parameter(0)
  y = bf16[16,8] parameter(1)
  z = bf16[8] parameter(2)
  dot_a = bf16[8,8] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  z_bcast = bf16[8,8] broadcast(z), dimensions={1}
  ROOT add = bf16[8,8] add(dot_a, z_bcast)
}

)";
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{8e-3, 2e-3}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: bf16[8,16], y: bf16[16,8], z: bf16[8]) -> bf16[8,8] {
; CHECK-DAG:     [[P0:%[^ ]+]] = bf16[8,16]{1,0} parameter(0)
; CHECK-DAG:     [[P1:%[^ ]+]] = bf16[16,8]{1,0} parameter(1)
; CHECK-DAG:     [[P2:%[^ ]+]] = bf16[8]{0} parameter(2)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = bf16[8,8]{1,0} custom-call([[P0]], [[P1]], [[P2]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"BIAS"
; CHECK:           }
      )");
}

TEST_F(CublasLtGemmRewriteTest, VectorBiasBF16Padded) {
  if (!CudaOrRocmCheck(se::CudaComputeCapability::AMPERE, Switch::True)) {
    GTEST_SKIP() << "Padding of GEMM operands in bfloat16 only implemented on "
                    "Ampere and newer architectures.";
  }
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = bf16[6,12] parameter(0)
  y = bf16[12,6] parameter(1)
  z = bf16[6] parameter(2)
  dot_a = bf16[6,6] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  z_bcast = bf16[6,6] broadcast(z), dimensions={1}
  ROOT add = bf16[6,6] add(dot_a, z_bcast)
}

)";
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-3, 1e-3}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: bf16[6,12], y: bf16[12,6], z: bf16[6]) -> bf16[6,6] {
; CHECK-DAG:     [[P0:%[^ ]+]] = bf16[6,12]{1,0} parameter(0)
; CHECK-DAG:     [[C0:%[^ ]+]] = bf16[] constant(0)
; CHECK-DAG:     [[P0_PADDED:%[^ ]+]] = bf16[8,16]{1,0} pad([[P0]], [[C0]]), padding=0_2x0_4
; CHECK-DAG:     [[P1:%[^ ]+]] = bf16[12,6]{1,0} parameter(1)
; CHECK-DAG:     [[P1_PADDED:%[^ ]+]] = bf16[16,8]{1,0} pad([[P1]], [[C0]]), padding=0_4x0_2
; CHECK-DAG:     [[P2:%[^ ]+]] = bf16[6]{0} parameter(2)
; CHECK-NEXT:    [[MATMUL:%[^ ]+]] = bf16[8,8]{1,0} custom-call([[P0_PADDED]], [[P1_PADDED]], [[P2]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"BIAS"
; CHECK:           }
; CHECK-NEXT:    [[OUT:%[^ ]+]] = bf16[6,6]{1,0} slice([[MATMUL]]), slice={[0:6], [0:6]}
      )");
}

// For bfloat16, the operands are padded if necessary on Ampere and newer
// architectures so that the sizes of all dimensions are multiples of 8.
TEST_F(CublasLtGemmRewriteTest, ReluActivationBF16Unpadded) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = bf16[8,16] parameter(0)
  y = bf16[16,8] parameter(1)
  dot_a = bf16[8,8] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  c = bf16[] constant(0)
  c_bcast = bf16[8,8] broadcast(c), dimensions={}
  ROOT out = bf16[8,8] maximum(dot_a, c_bcast)
}

)";
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-3, 1e-3}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: bf16[8,16], y: bf16[16,8]) -> bf16[8,8] {
; CHECK-DAG:     [[P0:%[^ ]+]] = bf16[8,16]{1,0} parameter(0)
; CHECK-DAG:     [[P1:%[^ ]+]] = bf16[16,8]{1,0} parameter(1)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = bf16[8,8]{1,0} custom-call([[P0]], [[P1]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"RELU"
; CHECK:           }
      )");
}

TEST_F(CublasLtGemmRewriteTest, ReluActivationBF16Padded) {
  if (!CudaOrRocmCheck(se::CudaComputeCapability::AMPERE, Switch::True)) {
    GTEST_SKIP() << "Padding of GEMM operands in bfloat16 only implemented on "
                    "Ampere and newer architectures.";
  }
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = bf16[6,12] parameter(0)
  y = bf16[12,6] parameter(1)
  dot_a = bf16[6,6] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  c = bf16[] constant(0)
  c_bcast = bf16[6,6] broadcast(c), dimensions={}
  ROOT out = bf16[6,6] maximum(dot_a, c_bcast)
}

)";
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: bf16[6,12], y: bf16[12,6]) -> bf16[6,6] {
; CHECK-DAG:     [[P0:%[^ ]+]] = bf16[6,12]{1,0} parameter(0)
; CHECK-DAG:     [[C0:%[^ ]+]] = bf16[] constant(0)
; CHECK-DAG:     [[P0_PADDED:%[^ ]+]] = bf16[8,16]{1,0} pad([[P0]], [[C0]]), padding=0_2x0_4
; CHECK-DAG:     [[P1:%[^ ]+]] = bf16[12,6]{1,0} parameter(1)
; CHECK-DAG:     [[P1_PADDED:%[^ ]+]] = bf16[16,8]{1,0} pad([[P1]], [[C0]]), padding=0_4x0_2
; CHECK-NEXT:    [[MATMUL:%[^ ]+]] = bf16[8,8]{1,0} custom-call([[P0_PADDED]], [[P1_PADDED]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"RELU"
; CHECK:           }
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = bf16[6,6]{1,0} slice([[MATMUL]]), slice={[0:6], [0:6]}
      )");
}

// For bfloat16, the operands are padded if necessary on Ampere and newer
// architectures so that the sizes of all dimensions are multiples of 8.
TEST_F(CublasLtGemmRewriteTest, VectorBiasReluActivationBF16Unpadded) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = bf16[8,16] parameter(0)
  y = bf16[16,8] parameter(1)
  z = bf16[8] parameter(2)
  dot_a = bf16[8,8] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  z_bcast = bf16[8,8] broadcast(z), dimensions={1}
  add = bf16[8,8] add(dot_a, z_bcast)
  c = bf16[] constant(0)
  c_bcast = bf16[8,8] broadcast(c), dimensions={}
  ROOT out = bf16[8,8] maximum(add, c_bcast)
}

)";
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{8e-3, 2e-3}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: bf16[8,16], y: bf16[16,8], z: bf16[8]) -> bf16[8,8] {
; CHECK-DAG:     [[P0:%[^ ]+]] = bf16[8,16]{1,0} parameter(0)
; CHECK-DAG:     [[P1:%[^ ]+]] = bf16[16,8]{1,0} parameter(1)
; CHECK-DAG:     [[P2:%[^ ]+]] = bf16[8]{0} parameter(2)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = bf16[8,8]{1,0} custom-call([[P0]], [[P1]], [[P2]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"BIAS_RELU"
; CHECK:           }
      )");
}

TEST_F(CublasLtGemmRewriteTest, VectorBiasReluActivationBF16Padded) {
  if (!CudaOrRocmCheck(se::CudaComputeCapability::AMPERE, Switch::True)) {
    GTEST_SKIP() << "Padding of GEMM operands in bfloat16 only implemented on "
                    "Ampere and newer architectures.";
  }
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = bf16[6,12] parameter(0)
  y = bf16[12,6] parameter(1)
  z = bf16[6] parameter(2)
  dot_a = bf16[6,6] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  z_bcast = bf16[6,6] broadcast(z), dimensions={1}
  add = bf16[6,6] add(dot_a, z_bcast)
  c = bf16[] constant(0)
  c_bcast = bf16[6,6] broadcast(c), dimensions={}
  ROOT out = bf16[6,6] maximum(add, c_bcast)
}

)";
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-3, 1e-3}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: bf16[6,12], y: bf16[12,6], z: bf16[6]) -> bf16[6,6] {
; CHECK-DAG:     [[P0:%[^ ]+]] = bf16[6,12]{1,0} parameter(0)
; CHECK-DAG:     [[C0:%[^ ]+]] = bf16[] constant(0)
; CHECK-DAG:     [[P0_PADDED:%[^ ]+]] = bf16[8,16]{1,0} pad([[P0]], [[C0]]), padding=0_2x0_4
; CHECK-DAG:     [[P1:%[^ ]+]] = bf16[12,6]{1,0} parameter(1)
; CHECK-DAG:     [[P1_PADDED:%[^ ]+]] = bf16[16,8]{1,0} pad([[P1]], [[C0]]), padding=0_4x0_2
; CHECK-DAG:     [[P2:%[^ ]+]] = bf16[6]{0} parameter(2)
; CHECK-NEXT:    [[MATMUL:%[^ ]+]] = bf16[8,8]{1,0} custom-call([[P0_PADDED]], [[P1_PADDED]], [[P2]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"BIAS_RELU"
; CHECK:           }
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = bf16[6,6]{1,0} slice([[MATMUL]]), slice={[0:6], [0:6]}
      )");
}

TEST_F(CublasLtGemmRewriteTest, VectorBiasReluActivationF64) {
  if (CudaOrRocmCheck(Switch::False, Switch::True)) {
    GTEST_SKIP() << "TODO: Unsupported blas-lt F64 datatype on ROCM";
  }
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f64[2,3] parameter(0)
  y = f64[3,4] parameter(1)
  z = f64[4] parameter(2)
  dot_a = f64[2,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  z_bcast = f64[2,4] broadcast(z), dimensions={1}
  add = f64[2,4] add(dot_a, z_bcast)
  c = f64[] constant(0)
  c_bcast = f64[2,4] broadcast(c), dimensions={}
  ROOT out = f64[2,4] maximum(add, c_bcast)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-10, 1e-10}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: f64[2,3], y: f64[3,4], z: f64[4]) -> f64[2,4] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f64[2,3]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f64[3,4]{1,0} parameter(1)
; CHECK-NEXT:    [[P2:%[^ ]+]] = f64[4]{0} parameter(2)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f64[2,4]{1,0} custom-call([[P0]], [[P1]], [[P2]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"BIAS_RELU"
; CHECK:           }
      )");
}

TEST_F(CublasLtGemmRewriteTest, AlphaSimpleRewriteBiasAddActivation) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = f32[2,3] parameter(0)
  y = f32[3,4] parameter(1)
  z = f32[4] parameter(2)
  k = f32[] constant(3.0)
  k_bcast = f32[2,4] broadcast(k), dimensions={}
  dot_a = f32[2,4] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}, operand_precision={highest,highest}
  dot_a_multiplied = f32[2, 4] multiply(dot_a, k_bcast)
  z_bcast = f32[2,4] broadcast(z), dimensions={1}
  add = f32[2,4] add(dot_a_multiplied, z_bcast)
  c = f32[] constant(0)
  c_bcast = f32[2,4] broadcast(c), dimensions={}
  ROOT out = f32[2,4] maximum(add, c_bcast)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
  MatchOptimizedHlo(hlo_text,
                    R"(

; CHECK-LABEL: ENTRY %test (x: f32[2,3], y: f32[3,4], z: f32[4]) -> f32[2,4] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f32[2,3]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f32[3,4]{1,0} parameter(1)
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[4]{0} parameter(2)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[2,4]{1,0} custom-call([[P0]], [[P1]], [[P2]]),
; CHECK:           custom_call_target="__cublas$lt$matmul",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":3
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["HIGHEST","HIGHEST"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"BIAS_RELU"
; CHECK:           }
      )");
}

TEST_F(CublasLtGemmRewriteTest, FoldConstantBias) {
  const char* hlo_text = R"(
HloModule test
ENTRY test {
  x = f32[2,2] parameter(0)
  y = f32[2,2] parameter(1)
  bias = f32[2,2] broadcast(f32[2] constant({0, 0})), dimensions={0}

  dot1 = f32[2,2] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  bias1 = f32[2,2] parameter(2)
  sum1 = add(dot1, bias1)

  dot2 = f32[2,2] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  sum2 = add(dot2, f32[2,2] reshape(bias))

  dot3 = f32[2,2] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  bias3 = f32[2,2] transpose(bias), dimensions={1,0}
  sum3 = add(dot3, bias3)

  dot4 = f32[2,2] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  sum4 = add(dot4, f32[2,2] bitcast(bias))

  ROOT root = tuple(sum1, sum2, sum3, sum4)
}
)";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  GemmRewriter pass(GpuComputeComp());
  TF_ASSERT_OK_AND_ASSIGN(bool changed, this->RunHloPass(&pass, module.get()));
  SCOPED_TRACE(module->ToString());
  EXPECT_TRUE(changed);

  EXPECT_THAT(
      module->entry_computation()->root_instruction(),
      GmockMatch(m::Tuple(
          m::CustomCall(m::Parameter(0), m::Parameter(1), m::Parameter()),
          m::CustomCall(m::Parameter(0), m::Parameter(1), m::Constant()),
          m::CustomCall(m::Parameter(0), m::Parameter(1), m::Constant()),
          m::CustomCall(m::Parameter(0), m::Parameter(1), m::Constant()))));
}

TEST_F(CublasLtGemmRewriteTest, MultipleMaximumUsers) {
  const char* hlo_text = R"(
HloModule multiple_maximum_users

relu {
  Arg_0 = f32[3,896,54]{2,1,0} parameter(0)
  constant = f32[] constant(0)
  broadcast = f32[3,896,54]{2,1,0} broadcast(constant), dimensions={}
  ROOT maximum = f32[3,896,54]{2,1,0} maximum(Arg_0, broadcast)
}

ENTRY main {
  constant = f32[] constant(1)
  broadcast_1 = f32[3,896,1024]{2,1,0} broadcast(constant), dimensions={}
  Arg_2 = f32[1024,54]{1,0} parameter(2)
  dot = f32[3,896,54]{2,1,0} dot(broadcast_1, Arg_2), lhs_contracting_dims={2}, rhs_contracting_dims={0}
  Arg_1 = f32[54]{0} parameter(1)
  broadcast_2 = f32[3,896,54]{2,1,0} broadcast(Arg_1), dimensions={2}
  add = f32[3,896,54]{2,1,0} add(dot, broadcast_2)
  call = f32[3,896,54]{2,1,0} call(add), to_apply=relu
  Arg_0 = f32[1]{0} parameter(0)
  reshape_1 = f32[1,1,1]{2,1,0} reshape(Arg_0)
  broadcast_3 = f32[1,1,1]{2,1,0} broadcast(reshape_1), dimensions={0,1,2}
  reshape_2 = f32[] reshape(broadcast_3)
  broadcast_4 = f32[3,896,54]{2,1,0} broadcast(reshape_2), dimensions={}
  multiply = f32[3,896,54]{2,1,0} multiply(call, broadcast_4)
  ROOT tuple = (f32[3,896,54]{2,1,0}, f32[3,896,54]{2,1,0}) tuple(multiply, call)
}
)";

  // TODO(cjfj): Why do we need to relax the error constraint here?!
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-4}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK:           custom_call_target="__cublas$lt$matmul",
      )");
}

// Test gemm matrix bias add fusion with mix type and out of place update(C !=
// D)
TEST_F(CublasLtGemmRewriteTest, MatrixBiasMixTypeOutOfPlace) {
  if (CudaOrRocmCheck(Switch::False, Switch::True)) {
    GTEST_SKIP() << "TODO: Unsupported mixed datatypes on ROCM";
  }
  std::vector<std::tuple<absl::string_view, absl::string_view>>
      type_combinations = {
          {"f16", "f32"},
          {"bf16", "f32"},
      };

  const char* hlo_text_template = R"(
HloModule test

ENTRY test {
  x = <<ABType>>[16,32] parameter(0)
  y = <<ABType>>[32,16] parameter(1)
  z = <<DType>>[16,16] parameter(2)
  dot_a = <<ABType>>[16,16] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  convert = <<DType>>[16,16] convert(dot_a)
  ROOT out = <<DType>>[16,16] add(convert, z)
})";
  for (const auto& type_combination : type_combinations) {
    absl::flat_hash_map<absl::string_view, absl::string_view> replacements;
    replacements["<<ABType>>"] = std::get<0>(type_combination);
    replacements["<<DType>>"] = std::get<1>(type_combination);
    const auto hlo_text = absl::StrReplaceAll(hlo_text_template, replacements);
    EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-3, 1e-3}));
    TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> optimized_module,
                            GetOptimizedModule(hlo_text));
    EXPECT_THAT(optimized_module->entry_computation()->root_instruction(),
                GmockMatch(m::CustomCall(m::Parameter(0), m::Parameter(1),
                                         m::Parameter(2))));
  }
}

// Test batch gemm matrix bias add fusion with mix type and out of place
// update(C != D)
TEST_F(CublasLtGemmRewriteTest, MatrixBiasMixTypeOutOfPlaceBatched) {
  if (CudaOrRocmCheck(Switch::False, Switch::True)) {
    GTEST_SKIP() << "TODO: Unsupported mixed datatypes on ROCM";
  }
  std::vector<std::tuple<absl::string_view, absl::string_view>>
      type_combinations = {
          {"f16", "f32"},
          {"bf16", "f32"},
      };

  const char* hlo_text_template = R"(
HloModule test

ENTRY test {
  x = <<ABType>>[4,16,32] parameter(0)
  y = <<ABType>>[4,32,16] parameter(1)
  z = <<DType>>[4,16,16] parameter(2)
  dot_a = <<ABType>>[4,16,16] dot(x, y), lhs_contracting_dims={2}, rhs_contracting_dims={1}, lhs_batch_dims={0}, rhs_batch_dims={0}
  convert = <<DType>>[4,16,16] convert(dot_a)
  ROOT out = <<DType>>[4,16,16] add(convert, z)
})";
  for (const auto& type_combination : type_combinations) {
    absl::flat_hash_map<absl::string_view, absl::string_view> replacements;
    replacements["<<ABType>>"] = std::get<0>(type_combination);
    replacements["<<DType>>"] = std::get<1>(type_combination);
    const auto hlo_text = absl::StrReplaceAll(hlo_text_template, replacements);
    EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-3, 1e-3}));
    TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> optimized_module,
                            GetOptimizedModule(hlo_text));
    EXPECT_THAT(optimized_module->entry_computation()->root_instruction(),
                GmockMatch(m::CustomCall(m::Parameter(0), m::Parameter(1),
                                         m::Parameter(2))));
  }
}

// Test gemm matrix bias add fusion with mix type and in place update(C = D)
TEST_F(CublasLtGemmRewriteTest, MatrixBiasMixTypeInPlace) {
  if (CudaOrRocmCheck(Switch::False, Switch::True)) {
    GTEST_SKIP() << "TODO: Unsupported mixed datatypes on ROCM";
  }
  std::vector<std::tuple<absl::string_view, absl::string_view>>
      type_combinations = {
          {"f16", "f32"},
          {"bf16", "f32"},
      };
  const char* hlo_text_template = R"(
HloModule test

ENTRY test {
  x = <<ABType>>[16,32] parameter(0)
  y = <<ABType>>[32,16] parameter(1)
  z = <<DType>>[16,16] parameter(2)
  dot_a = <<ABType>>[16,16] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  bias = <<DType>>[16,16] negate(z)
  convert = <<DType>>[16,16] convert(dot_a)
  ROOT out = <<DType>>[16,16] add(convert, bias)
})";

  for (const auto& type_combination : type_combinations) {
    absl::flat_hash_map<absl::string_view, absl::string_view> replacements;
    replacements["<<ABType>>"] = std::get<0>(type_combination);
    replacements["<<DType>>"] = std::get<1>(type_combination);
    const auto hlo_text = absl::StrReplaceAll(hlo_text_template, replacements);
    EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-3, 1e-3}));
    TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> optimized_module,
                            GetOptimizedModule(hlo_text));
    EXPECT_THAT(optimized_module->entry_computation()->root_instruction(),
                GmockMatch(m::CustomCall(m::Parameter(0), m::Parameter(1),
                                         m::Negate(m::Parameter(2)))));
  }
}

// Test gemm matrix bias add fusion with mix type that is not supported
TEST_F(CublasLtGemmRewriteTest, MatrixBiasMixTypeNotSupported) {
  const char* hlo_text = R"(
HloModule test

ENTRY test {
  x = bf16[16,32] parameter(0)
  y = bf16[32,16] parameter(1)
  z = f64[16,16] parameter(2)
  dot_a = bf16[16,16] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  bias = f64[16,16] negate(z)
  convert = f64[16,16] convert(dot_a)
  ROOT out = f64[16,16] add(convert, bias)
}

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-3, 1e-3}));
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> optimized_module,
                          GetOptimizedModule(hlo_text));
  EXPECT_THAT(
      optimized_module->entry_computation()->root_instruction(),
      GmockMatch(m::Fusion(m::Parameter(2),
                           m::CustomCall({"__cublas$lt$matmul"},
                                         m::Parameter(0), m::Parameter(1)))));
}

class ParameterizedFp8GemmRewriteTest : public ParameterizedGemmRewriteTest {
 protected:
  // Check the HLO runs and has an FP8 cuBLAS LT custom call on supported
  // architectures (Ada, Hopper, and later).
  void CheckFp8IfSupported(absl::string_view hlo_text,
                           ErrorSpec error_spec = ErrorSpec{1e-2, 1e-2}) {
    if (!CudaOrRocmCheck(8, 9, Switch::False)) {
      return;
    }
    EXPECT_TRUE(RunAndCompare(hlo_text, error_spec));

    // Most FP8 tests directly create a GemmRewriter and check the output.
    // Here, also run the entire HLO pass pipeline to ensure no other passes
    // interfere with GemmRewriter's pattern matching.
    TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> optimized_module,
                            GetOptimizedModule(hlo_text));
    const HloInstruction* call =
        FindInstruction(optimized_module.get(), HloOpcode::kCustomCall);
    ASSERT_NE(call, nullptr);
    EXPECT_EQ(call->custom_call_target(), "__cublas$lt$matmul$f8");
  }
  void SetUp() override {
    // VLOG(-1) << "Running test " <<
    // ::testing::UnitTest::GetInstance()->current_test_info()->name();
    if (CudaOrRocmCheck(Switch::False, Switch::True)) {
      GTEST_SKIP() << "F8 gemm rewrite is not yet supported on ROCm platform";
    }
  }
};

TEST_P(ParameterizedFp8GemmRewriteTest, DoNotRewriteToF8OnPreAda) {
  if (CudaOrRocmCheck(8, 9, Switch::False)) {
    GTEST_SKIP() << "Test requires a pre-Ada GPU.";
  }
  const char* hlo_text = R"(
    HloModule test

    ENTRY PreAdaTest {
      x = f8e4m3fn[16,32] parameter(0)
      y = f8e4m3fn[32,16] parameter(1)
      ROOT out = f8e4m3fn[16,16] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
          }

)";

  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-2, 1e-2}));
  MatchOptimizedHlo(hlo_text,
                    R"(
; CHECK-LABEL: ENTRY %PreAdaTest (x: f8e4m3fn[16,32], y: f8e4m3fn[32,16]) -> f8e4m3fn[16,16] {
; CHECK:    {{.*}} = {{.*}} custom-call({{.*}}, {{.*}})
; CHECK-DAG:  custom_call_target="<<CUBLAS_CUSTOM_CALL_TARGET_PLACEHOLDER>>"
          )");
}

TEST_P(ParameterizedFp8GemmRewriteTest, UnsupportedTypesF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif  // CUDA_VERSION < 12000

  // Test with types unsupported by cuBLAS LT when FP8 is used. cuBLAS LT with
  // FP8 requires one of the operands to be F8E4M3FN.
  const char* hlo_text = R"(
    HloModule test

    ENTRY unsupported_types {
      x = f8e5m2[16,16] parameter(0)
      y = f8e5m2[16,16] parameter(1)
      ROOT out = f8e5m2[16,16] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
          }
)";
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-2, 1e-2}));
  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(GpuComputeComp()),
                            absl::StrReplaceAll(R"(
; CHECK-LABEL: ENTRY %unsupported_types (x: f8e5m2[16,16], y: f8e5m2[16,16]) -> f8e5m2[16,16] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f8e5m2[16,16]{1,0} parameter(0)
; CHECK-NEXT:    [[P0_CONVERT:%[^ ]+]] = f16[16,16]{1,0} convert([[P0]])
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e5m2[16,16]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_CONVERT:%[^ ]+]] = f16[16,16]{1,0} convert([[P1]])
; CHECK-NEXT:    [[DOT:%[^ ]+]] = {{.*}} custom-call([[P0_CONVERT]], [[P1_CONVERT]]),
; CHECK:           custom_call_target="<<CUBLAS_CUSTOM_CALL_TARGET_PLACEHOLDER>>",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
; CHECK:         ROOT [[OUT:%[^ ]+]] = f8e5m2[16,16]{1,0} convert
      )",
                                                replacements_));
}

TEST_P(ParameterizedFp8GemmRewriteTest, UnscaledABUnscaledDF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif  // CUDA_VERSION < 12000
  const char* hlo_text = R"(
    HloModule test

    ENTRY test {
      x = f8e4m3fn[16,32] parameter(0)
      y = f8e4m3fn[32,16] parameter(1)
      ROOT out = f8e4m3fn[16,16] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
          }

)";

  CheckFp8IfSupported(hlo_text);
  // auto comp = se::DeviceCom
  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(
; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[16,32], y: f8e4m3fn[32,16]) -> f8e4m3fn[16,16] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f8e4m3fn[16,32]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e4m3fn[32,16]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_TRANSPOSE:%[^ ]+]] = f8e4m3fn[16,32]{1,0} transpose([[P1]]), dimensions={1,0}
; CHECK-NEXT:    [[C1:[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f8e4m3fn[16,16]{1,0} custom-call([[P0]], [[P1_TRANSPOSE]], [[C1]], [[C1]], [[C1]], /*index=5*/[[C1]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest, ScaledABUnscaledDF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif  // CUDA_VERSION < 12000
  const char* hlo_text = R"(
    HloModule test

    ENTRY test {
      x = f8e4m3fn[16,32] parameter(0)
      y = f8e4m3fn[32,16] parameter(1)
      x_f32 = f32[16,32] convert(x)
      y_f32 = f32[32,16] convert(y)
      x_scale = f32[] parameter(2)
      y_scale = f32[] parameter(3)
      x_scale_bcast = f32[16,32] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[32,16] broadcast(y_scale), dimensions={}
      x_unscaled = f32[16,32] multiply(x_f32, x_scale_bcast)
      y_unscaled = f32[32,16] multiply(y_f32, y_scale_bcast)
      ROOT out = f32[16,16] dot(x_unscaled, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}
          }

)";

  CheckFp8IfSupported(hlo_text);
  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(
; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[16,32], y: f8e4m3fn[32,16], x_scale: f32[], y_scale: f32[]) -> f32[16,16] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f8e4m3fn[16,32]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e4m3fn[32,16]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_TRANSPOSE:%[^ ]+]] = f8e4m3fn[16,32]{1,0} transpose([[P1]]), dimensions={1,0}
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[] parameter(2)
; CHECK-NEXT:    [[P3:%[^ ]+]] = f32[] parameter(3)
; CHECK-NEXT:    [[C1:%[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[16,16]{1,0} custom-call([[P0]], [[P1_TRANSPOSE]], [[P2]], [[P3]], [[C1]], /*index=5*/[[C1]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest, ScaledABUnscaledDPaddedF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif  // CUDA_VERSION < 12000
  const char* hlo_text = R"(
    HloModule test

    ENTRY test {
      x = f8e4m3fn[13,17] parameter(0)
      y = f8e4m3fn[17,31] parameter(1)
      x_f32 = f32[13,17] convert(x)
      y_f32 = f32[17,31] convert(y)
      x_scale = f32[] parameter(2)
      y_scale = f32[] parameter(3)
      x_scale_bcast = f32[13,17] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[17,31] broadcast(y_scale), dimensions={}
      x_unscaled = f32[13,17] multiply(x_f32, x_scale_bcast)
      y_unscaled = f32[17,31] multiply(y_f32, y_scale_bcast)
      ROOT out = f32[13,31] dot(x_unscaled, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}
          }

)";

  CheckFp8IfSupported(hlo_text);
  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(
; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[13,17], y: f8e4m3fn[17,31], x_scale: f32[], y_scale: f32[]) -> f32[13,31] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f8e4m3fn[13,17]{1,0} parameter(0)
; CHECK-NEXT:    [[C0:%[^ ]+]] = f8e4m3fn[] constant(0)
; CHECK-NEXT:    [[P0_PADDED:%[^ ]+]] = f8e4m3fn[16,32]{1,0} pad([[P0]], [[C0]]), padding=0_3x0_15
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e4m3fn[17,31]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_TRANSPOSE:%[^ ]+]] = f8e4m3fn[31,17]{1,0} transpose([[P1]]), dimensions={1,0}
; CHECK-NEXT:    [[C1:%[^ ]+]] = f8e4m3fn[] constant(0)
; CHECK-NEXT:    [[P1_TRANSPOSE_PADDED:%[^ ]+]] = f8e4m3fn[32,32]{1,0} pad([[P1_TRANSPOSE]], [[C1]])
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[] parameter(2)
; CHECK-NEXT:    [[P3:%[^ ]+]] = f32[] parameter(3)
; CHECK-NEXT:    [[C4:%[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    [[DOT:%[^ ]+]] = f32[16,32]{1,0} custom-call([[P0_PADDED]], [[P1_TRANSPOSE_PADDED]], [[P2]], [[P3]], [[C4]], /*index=5*/[[C4]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
; CHECK-NEXT: ROOT [[OUT:%[^ ]+]] = f32[13,31]{1,0} slice([[DOT]]), slice={[0:13], [0:31]}
      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest, ScaledABUnscaledDBitcastF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif  // CUDA_VERSION < 12000
  const char* hlo_text = R"(
    HloModule test

    ENTRY test {
      x = f8e4m3fn[2,8,16] parameter(0)
      y = f8e4m3fn[16,16] parameter(1)
      x_f32 = f32[2,8,16] convert(x)
      y_f32 = f32[16,16] convert(y)
      x_scale = f32[] parameter(2)
      y_scale = f32[] parameter(3)
      x_scale_bcast = f32[2,8,16] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[16,16] broadcast(y_scale), dimensions={}
      x_unscaled = f32[2,8,16] multiply(x_f32, x_scale_bcast)
      y_unscaled = f32[16,16] multiply(y_f32, y_scale_bcast)
      x_bitcast = f32[16,16] bitcast(x_unscaled)
      ROOT out = f32[16,16] dot(x_bitcast, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}
          }

)";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  GemmRewriter pass(CudaHopperOrRocm());
  TF_ASSERT_OK_AND_ASSIGN(bool changed, this->RunHloPass(&pass, module.get()));
  EXPECT_TRUE(changed);

  EXPECT_THAT(
      module->entry_computation()->root_instruction(),
      GmockMatch(
          m::CustomCall({"__cublas$lt$matmul$f8"}).WithShape(F32, {16, 16})));
}

TEST_P(ParameterizedFp8GemmRewriteTest, ScaledABUnscaledDUnaryOpsF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif  // CUDA_VERSION < 12000
  const char* hlo_text = R"(
    HloModule test

    ENTRY test {
      x = f8e4m3fn[3] parameter(0)
      y = f8e4m3fn[32,16] parameter(1)
      x_f32 = f32[3] convert(x)
      y_f32 = f32[32,16] convert(y)
      x_scale = f32[] parameter(2)
      y_scale = f32[] parameter(3)
      x_scale_bcast = f32[3] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[32,16] broadcast(y_scale), dimensions={}
      x_unscaled = f32[3] multiply(x_f32, x_scale_bcast)
      zero = f32[] constant(0)
      x_unscaled_padded = f32[30] pad(x_unscaled, zero), padding=0_27
      x_unscaled_padded_bcast = f32[30,8,5] broadcast(x_unscaled_padded), dimensions={0}
      x_unscaled_padded_bcast_sliced = f32[16,8,4] slice(x_unscaled_padded_bcast), slice={[2:18], [0:8], [0:4]}
      x_unscaled_padded_bcast_sliced_reshaped = f32[16,32] reshape(x_unscaled_padded_bcast_sliced)
      y_unscaled = f32[32,16] multiply(y_f32, y_scale_bcast)
      ROOT out = f32[16,16] dot(x_unscaled_padded_bcast_sliced_reshaped, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}
          }

)";
  CheckFp8IfSupported(hlo_text);
  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(

; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[3], y: f8e4m3fn[32,16], x_scale: f32[], y_scale: f32[]) -> f32[16,16] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f8e4m3fn[3]{0} parameter(0)
; CHECK-NEXT:    [[C0:%[^ ]+]] = f32[] constant(0)
; CHECK-NEXT:    [[C0_CONVERT:%[^ ]+]] = f8e4m3fn[] convert([[C0]])
; CHECK-NEXT:    [[P0_U0:%[^ ]+]] = f8e4m3fn[30]{0} pad([[P0]], [[C0_CONVERT]]), padding=0_27
; CHECK-NEXT:    [[P0_U1:%[^ ]+]] = f8e4m3fn[30,8,5]{2,1,0} broadcast([[P0_U0]]), dimensions={0}
; CHECK-NEXT:    [[P0_U2:%[^ ]+]] = f8e4m3fn[16,8,4]{2,1,0} slice([[P0_U1]]), slice={[2:18], [0:8], [0:4]}
; CHECK-NEXT:    [[P0_U3:%[^ ]+]] = f8e4m3fn[16,32]{1,0} reshape([[P0_U2]])
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e4m3fn[32,16]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_TRANSPOSE:%[^ ]+]] = f8e4m3fn[16,32]{1,0} transpose([[P1]]), dimensions={1,0}
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[] parameter(2)
; CHECK-NEXT:    [[P3:%[^ ]+]] = f32[] parameter(3)
; CHECK-NEXT:    [[C2:%[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[16,16]{1,0} custom-call([[P0_U3]], [[P1_TRANSPOSE]], [[P2]], [[P3]], [[C2]], /*index=5*/[[C2]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest, ScaledABUnscaledDDynamicSliceF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif  // CUDA_VERSION < 12000
  const char* hlo_text = R"(
    HloModule test

    ENTRY test {
      x = f8e4m3fn[32,32] parameter(0)
      y = f8e4m3fn[16,32] parameter(1)
      zero = s32[] constant(0)
      x_f32 = f32[32,32] convert(x)
      y_f32 = f32[16,32] convert(y)
      x_scale = f32[] parameter(2)
      y_scale = f32[] parameter(3)
      x_scale_bcast = f32[32,32] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[16,32] broadcast(y_scale), dimensions={}
      x_unscaled = f32[32,32] multiply(x_f32, x_scale_bcast)
      y_unscaled = f32[16,32] multiply(y_f32, y_scale_bcast)
      dyn_slice = f32[16,32]{1,0} dynamic-slice(x_unscaled, zero, zero), dynamic_slice_sizes={16,32}
      ROOT dot_a = f32[16,16] dot(dyn_slice, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={1}
          }
)";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  GemmRewriter pass(CudaHopperOrRocm());
  TF_ASSERT_OK_AND_ASSIGN(bool changed, this->RunHloPass(&pass, module.get()));
  EXPECT_TRUE(changed);

  CheckFp8IfSupported(hlo_text);
  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(
; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[32,32], y: f8e4m3fn[16,32], x_scale: f32[], y_scale: f32[]) -> f32[16,16] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f8e4m3fn[32,32]{1,0} parameter(0)
; CHECK-NEXT:    [[C0:%[^ ]+]] = s32[] constant(0)
; CHECK-NEXT:    [[DYN_SLICE:%[^ ]+]] = f8e4m3fn[16,32]{1,0} dynamic-slice([[P0]], [[C0]], [[C0]]), dynamic_slice_sizes={16,32}
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e4m3fn[16,32]{1,0} parameter(1)
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[] parameter(2)
; CHECK-NEXT:    [[P3:%[^ ]+]] = f32[] parameter(3)
; CHECK-NEXT:    [[C1:%[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[16,16]{1,0} custom-call([[DYN_SLICE]], [[P1]], [[P2]], [[P3]], [[C1]], /*index=5*/[[C1]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest, ScaledABUnscaledDSelectF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif  // CUDA_VERSION < 12000
  const char* hlo_text = R"(
    HloModule test

    ENTRY test {
      x = f8e4m3fn[16,32] parameter(0)
      y = f8e4m3fn[16,32] parameter(1)
      x_f32 = f32[16,32] convert(x)
      y_f32 = f32[16,32] convert(y)
      x_scale = f32[] parameter(2)
      y_scale = f32[] parameter(3)
      x_scale_bcast = f32[16,32] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[16,32] broadcast(y_scale), dimensions={}
      x_unscaled = f32[16,32] multiply(x_f32, x_scale_bcast)
      y_unscaled = f32[16,32] multiply(y_f32, y_scale_bcast)
      k = pred[16,32] parameter(4)
      c = f32[] constant(0)
      c_bcast = f32[16,32] broadcast(c), dimensions={}
      select_a = f32[16,32] select(k, y_unscaled, c_bcast)
      ROOT dot_a = f32[16,16] dot(x_unscaled, select_a), lhs_contracting_dims={1}, rhs_contracting_dims={1}
          }
)";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  GemmRewriter pass(CudaHopperOrRocm());
  TF_ASSERT_OK_AND_ASSIGN(bool changed, this->RunHloPass(&pass, module.get()));
  EXPECT_TRUE(changed);

  CheckFp8IfSupported(hlo_text);
  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(
; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[16,32], y: f8e4m3fn[16,32], x_scale: f32[], y_scale: f32[], k: pred[16,32]) -> f32[16,16] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f8e4m3fn[16,32]{1,0} parameter(0)
; CHECK-NEXT:    [[P4:%[^ ]+]] = pred[16,32]{1,0} parameter(4)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e4m3fn[16,32]{1,0} parameter(1)
; CHECK-NEXT:    [[C0:%[^ ]+]] = f32[] constant(0)
; CHECK-NEXT:    [[C0_BCAST:%[^ ]+]] = f32[16,32]{1,0} broadcast([[C0]]), dimensions={}
; CHECK-NEXT:    [[C0_CONVERT:%[^ ]+]] = f8e4m3fn[16,32]{1,0} convert([[C0_BCAST]])
; CHECK-NEXT:    [[SELECT:%[^ ]+]] = f8e4m3fn[16,32]{1,0} select([[P4]], [[P1]], [[C0_CONVERT]])
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[] parameter(2)
; CHECK-NEXT:    [[P3:%[^ ]+]] = f32[] parameter(3)
; CHECK-NEXT:    [[C1:%[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[16,16]{1,0} custom-call([[P0]], [[SELECT]], [[P2]], [[P3]], [[C1]], /*index=5*/[[C1]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest,
       ScaledABUnscaledDSelectNonzeroConstantF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif  // CUDA_VERSION < 12000
  const char* hlo_text = R"(
    HloModule test

    ENTRY test {
      x = f8e4m3fn[16,32] parameter(0)
      y = f8e4m3fn[16,32] parameter(1)
      x_f32 = f32[16,32] convert(x)
      y_f32 = f32[16,32] convert(y)
      x_scale = f32[] parameter(2)
      y_scale = f32[] parameter(3)
      x_scale_bcast = f32[16,32] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[16,32] broadcast(y_scale), dimensions={}
      x_unscaled = f32[16,32] multiply(x_f32, x_scale_bcast)
      y_unscaled = f32[16,32] multiply(y_f32, y_scale_bcast)
      k = pred[16,32] parameter(4)
      c = f32[] constant(1)
      c_bcast = f32[16,32] broadcast(c), dimensions={}
      select_a = f32[16,32] select(k, y_unscaled, c_bcast)
      ROOT dot_a = f32[16,16] dot(x_unscaled, select_a), lhs_contracting_dims={1}, rhs_contracting_dims={1}
          }
)";
  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  GemmRewriter pass(CudaHopperOrRocm());
  TF_ASSERT_OK_AND_ASSIGN(bool changed, this->RunHloPass(&pass, module.get()));
  EXPECT_TRUE(changed);

  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(
; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[16,32], y: f8e4m3fn[16,32], x_scale: f32[], y_scale: f32[], k: pred[16,32]) -> f32[16,16] {
; CHECK-NOT:           custom_call_target="__cublas$lt$matmul$f8"
      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest, BatchedScaledABUnscaledDF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif  // CUDA_VERSION < 12000
  const char* hlo_text = R"(
    HloModule test

    ENTRY test {
      x = f8e4m3fn[10,16,32] parameter(0)
      y = f8e4m3fn[10,32,16] parameter(1)
      x_f32 = f32[10,16,32] convert(x)
      y_f32 = f32[10,32,16] convert(y)
      x_scale = f32[] parameter(2)
      y_scale = f32[] parameter(3)
      x_scale_bcast = f32[10,16,32] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[10,32,16] broadcast(y_scale), dimensions={}
      x_unscaled = f32[10,16,32] multiply(x_f32, x_scale_bcast)
      y_unscaled = f32[10,32,16] multiply(y_f32, y_scale_bcast)
      ROOT out = f32[10,16,16] dot(x_unscaled, y_unscaled), lhs_contracting_dims={2}, rhs_contracting_dims={1}, lhs_batch_dims={0}, rhs_batch_dims={0}
          }

)";

  CheckFp8IfSupported(hlo_text);
  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(
; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[10,16,32], y: f8e4m3fn[10,32,16], x_scale: f32[], y_scale: f32[]) -> f32[10,16,16] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f8e4m3fn[10,16,32]{2,1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e4m3fn[10,32,16]{2,1,0} parameter(1)
; CHECK-NEXT:    [[P1_TRANSPOSE:%[^ ]+]] = f8e4m3fn[10,16,32]{2,1,0} transpose([[P1]]), dimensions={0,2,1}
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[] parameter(2)
; CHECK-NEXT:    [[P3:%[^ ]+]] = f32[] parameter(3)
; CHECK-NEXT:    [[C1:%[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[10,16,16]{2,1,0} custom-call([[P0]], [[P1_TRANSPOSE]], [[P2]], [[P3]], [[C1]], /*index=5*/[[C1]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["2"]
; CHECK-DAG:           "rhs_contracting_dimensions":["2"]
; CHECK-DAG:           "lhs_batch_dimensions":["0"]
; CHECK-DAG:           "rhs_batch_dimensions":["0"]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest, ScaledABAlphaDF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif  // CUDA_VERSION < 12000
  const char* hlo_text = R"(
    HloModule test

    ENTRY test {
      x = f8e4m3fn[16,32] parameter(0)
      y = f8e4m3fn[32,16] parameter(1)
      x_f32 = f32[16,32] convert(x)
      y_f32 = f32[32,16] convert(y)
      x_scale = f32[] parameter(2)
      y_scale = f32[] parameter(3)
      x_scale_bcast = f32[16,32] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[32,16] broadcast(y_scale), dimensions={}
      x_unscaled = f32[16,32] multiply(x_f32, x_scale_bcast)
      y_unscaled = f32[32,16] multiply(y_f32, y_scale_bcast)
      k = f32[] constant(3.0)
      k_bcast = f32[16,16] broadcast(k), dimensions={}
      dot_a = f32[16,16] dot(x_unscaled, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}
      ROOT out = f32[16,16] multiply(dot_a, k_bcast)
          }

)";

  CheckFp8IfSupported(hlo_text);
  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(

; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[16,32], y: f8e4m3fn[32,16], x_scale: f32[], y_scale: f32[]) -> f32[16,16] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f8e4m3fn[16,32]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e4m3fn[32,16]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_TRANSPOSE:%[^ ]+]] = f8e4m3fn[16,32]{1,0} transpose([[P1]]), dimensions={1,0}
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[] parameter(2)
; CHECK-NEXT:    [[P3:%[^ ]+]] = f32[] parameter(3)
; CHECK-NEXT:    [[C1:%[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[16,16]{1,0} custom-call([[P0]], [[P1_TRANSPOSE]], [[P2]], [[P3]], [[C1]], /*index=5*/[[C1]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":3
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest, ScaledABUnscaledDReluActivationF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif  // CUDA_VERSION < 12000
  const char* hlo_text = R"(
    HloModule test

    ENTRY test {
      x = f8e4m3fn[16,32] parameter(0)
      y = f8e4m3fn[32,16] parameter(1)
      x_f32 = f32[16,32] convert(x)
      y_f32 = f32[32,16] convert(y)
      x_scale = f32[] parameter(2)
      y_scale = f32[] parameter(3)
      x_scale_bcast = f32[16,32] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[32,16] broadcast(y_scale), dimensions={}
      x_unscaled = f32[16,32] multiply(x_f32, x_scale_bcast)
      y_unscaled = f32[32,16] multiply(y_f32, y_scale_bcast)
      dot_a = f32[16,16] dot(x_unscaled, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}
      c = f32[] constant(0)
      c_bcast = f32[16,16] broadcast(c), dimensions={}
      ROOT out = f32[16,16] maximum(dot_a, c_bcast)
          }

)";

  CheckFp8IfSupported(hlo_text);
  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(

; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[16,32], y: f8e4m3fn[32,16], x_scale: f32[], y_scale: f32[]) -> f32[16,16] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f8e4m3fn[16,32]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e4m3fn[32,16]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_TRANSPOSE:%[^ ]+]] = f8e4m3fn[16,32]{1,0} transpose([[P1]]), dimensions={1,0}
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[] parameter(2)
; CHECK-NEXT:    [[P3:%[^ ]+]] = f32[] parameter(3)
; CHECK-NEXT:    [[C1:%[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[16,16]{1,0} custom-call([[P0]], [[P1_TRANSPOSE]], [[P2]], [[P3]], [[C1]], /*index=5*/[[C1]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"RELU"
; CHECK:           }
      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest, InvScaledABUnscaledDF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif  // CUDA_VERSION < 12000
  const char* hlo_text = R"(
    HloModule test

    ENTRY test {
      x = f8e4m3fn[16,32] parameter(0)
      y = f8e4m3fn[32,16] parameter(1)
      x_f32 = f32[16,32] convert(x)
      y_f32 = f32[32,16] convert(y)
      x_scale = f32[] parameter(2)
      y_scale = f32[] parameter(3)
      x_scale_bcast = f32[16,32] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[32,16] broadcast(y_scale), dimensions={}
      x_unscaled = f32[16,32] divide(x_f32, x_scale_bcast)
      y_unscaled = f32[32,16] divide(y_f32, y_scale_bcast)
      ROOT out = f32[16,16] dot(x_unscaled, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}
          }

)";

  CheckFp8IfSupported(hlo_text);
  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest, ScaledABUnscaledDMatrixBiasF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif  // CUDA_VERSION < 12000
  const char* hlo_text = R"(
    HloModule test

    ENTRY test {
      x = f8e4m3fn[16,32] parameter(0)
      y = f8e4m3fn[32,16] parameter(1)
      b = f32[16,16] parameter(2)
      x_f32 = f32[16,32] convert(x)
      y_f32 = f32[32,16] convert(y)
      x_scale = f32[] parameter(3)
      y_scale = f32[] parameter(4)
      x_scale_bcast = f32[16,32] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[32,16] broadcast(y_scale), dimensions={}
      x_unscaled = f32[16,32] multiply(x_f32, x_scale_bcast)
      y_unscaled = f32[32,16] multiply(y_f32, y_scale_bcast)
      dot_a = f32[16,16] dot(x_unscaled, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}
      ROOT out = add(dot_a, b)
          }

)";

  CheckFp8IfSupported(hlo_text);
  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(

; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[16,32], y: f8e4m3fn[32,16], b: f32[16,16], x_scale: f32[], y_scale: f32[]) -> f32[16,16] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f8e4m3fn[16,32]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e4m3fn[32,16]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_TRANSPOSE:%[^ ]+]] = f8e4m3fn[16,32]{1,0} transpose([[P1]]), dimensions={1,0}
; CHECK-NEXT:    [[C0:%[^ ]+]] = f32[16,16]{1,0} parameter(2)
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[] parameter(3)
; CHECK-NEXT:    [[P3:%[^ ]+]] = f32[] parameter(4)
; CHECK-NEXT:    [[C1:%[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f32[16,16]{1,0} custom-call([[P0]], [[P1_TRANSPOSE]], [[C0]], [[P2]], [[P3]], /*index=5*/[[C1]], [[C1]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest, ScaledABUnscaledDMatrixBiasPaddedF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif  // CUDA_VERSION < 12000
  const char* hlo_text = R"(
    HloModule test

    ENTRY test {
      x = f8e4m3fn[14,31] parameter(0)
      y = f8e4m3fn[31,14] parameter(1)
      b = f32[14,14] parameter(2)
      x_f32 = f32[14,31] convert(x)
      y_f32 = f32[31,14] convert(y)
      x_scale = f32[] parameter(3)
      y_scale = f32[] parameter(4)
      x_scale_bcast = f32[14,31] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[31,14] broadcast(y_scale), dimensions={}
      x_unscaled = f32[14,31] multiply(x_f32, x_scale_bcast)
      y_unscaled = f32[31,14] multiply(y_f32, y_scale_bcast)
      dot_a = f32[14,14] dot(x_unscaled, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}
      ROOT out = add(dot_a, b)
          }

)";

  CheckFp8IfSupported(hlo_text);
  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(

; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[14,31], y: f8e4m3fn[31,14], b: f32[14,14], x_scale: f32[], y_scale: f32[]) -> f32[14,14] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f8e4m3fn[14,31]{1,0} parameter(0)
; CHECK-NEXT:    [[C0:%[^ ]+]] = f8e4m3fn[] constant(0)
; CHECK-NEXT:    [[P0_PADDED:%[^ ]+]] = f8e4m3fn[16,32]{1,0} pad([[P0]], [[C0]]), padding=0_2x0_1
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e4m3fn[31,14]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_TRANSPOSE:%[^ ]+]] = f8e4m3fn[14,31]{1,0} transpose([[P1]]), dimensions={1,0}
; CHECK-NEXT:    [[C1:%[^ ]+]] = f8e4m3fn[] constant(0)
; CHECK-NEXT:    [[P1_TRANSPOSE_PADDED:%[^ ]+]] = f8e4m3fn[16,32]{1,0} pad([[P1_TRANSPOSE]], [[C1]]), padding=0_2x0_1
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[14,14]{1,0} parameter(2)
; CHECK-NEXT:    [[C2:%[^ ]+]] = f32[] constant(0)
; CHECK-NEXT:    [[P2_PADDED:%[^ ]+]] = f32[16,16]{1,0} pad([[P2]], [[C2]]), padding=0_2x0_2
; CHECK-NEXT:    [[P3:%[^ ]+]] = f32[] parameter(3)
; CHECK-NEXT:    [[P4:%[^ ]+]] = f32[] parameter(4)
; CHECK-NEXT:    [[C3:%[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    [[DOT:%[^ ]+]] = f32[16,16]{1,0} custom-call([[P0_PADDED]], [[P1_TRANSPOSE_PADDED]], [[P2_PADDED]], [[P3]], [[P4]], /*index=5*/[[C3]], [[C3]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
; CHECK-NEXT: ROOT [[OUT:%[^ ]+]] = f32[14,14]{1,0} slice([[DOT]]), slice={[0:14], [0:14]}
      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest, ScaledABScaledDF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif  // CUDA_VERSION < 12000
  const char* hlo_text = R"(
    HloModule test

    ENTRY test {
      x = f8e4m3fn[16,32] parameter(0)
      y = f8e4m3fn[32,16] parameter(1)
      x_f32 = f32[16,32] convert(x)
      y_f32 = f32[32,16] convert(y)
      x_scale = f32[] parameter(2)
      y_scale = f32[] parameter(3)
      z_scale = f32[] parameter(4)
      x_scale_bcast = f32[16,32] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[32,16] broadcast(y_scale), dimensions={}
      z_scale_bcast = f32[16,16] broadcast(z_scale), dimensions={}
      x_unscaled = f32[16,32] multiply(x_f32, x_scale_bcast)
      y_unscaled = f32[32,16] multiply(y_f32, y_scale_bcast)
      dot_a = f32[16,16] dot(x_unscaled, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}
      dot_a_scaled = f32[16,16] divide(dot_a, z_scale_bcast)
      c1 = f32[] constant(-448.)
      c1_bcast = f32[16,16] broadcast(c1), dimensions={}
      c2 = f32[] constant(448.)
      c2_bcast = f32[16,16] broadcast(c2), dimensions={}
      dot_a_clamped = f32[16,16] clamp(c1_bcast, dot_a_scaled, c2_bcast)
      ROOT dot_a_f8 = f8e4m3fn[16,16] convert(dot_a_clamped)
          }

)";

  CheckFp8IfSupported(hlo_text);
  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(
; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[16,32], y: f8e4m3fn[32,16], x_scale: f32[], y_scale: f32[], z_scale: f32[]) -> f8e4m3fn[16,16] {
; CHECK:         [[P0:%[^ ]+]] = f8e4m3fn[16,32]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e4m3fn[32,16]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_TRANSPOSE:%[^ ]+]] = f8e4m3fn[16,32]{1,0} transpose([[P1]]), dimensions={1,0}
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[] parameter(2)
; CHECK-NEXT:    [[P3:%[^ ]+]] = f32[] parameter(3)
; CHECK-NEXT:    [[C1:%[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    [[C2:%[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    [[P4:%[^ ]+]] = f32[] parameter(4)
; CHECK-NEXT:    [[P4_INV:%[^ ]+]] = f32[] divide([[C2]], [[P4]])
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f8e4m3fn[16,16]{1,0} custom-call([[P0]], [[P1_TRANSPOSE]], [[P2]], [[P3]], [[C1]], /*index=5*/[[P4_INV]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest, ScaledABInvScaledDF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif  // CUDA_VERSION < 12000
  const char* hlo_text = R"(
    HloModule test

    ENTRY test {
      x = f8e4m3fn[16,32] parameter(0)
      y = f8e4m3fn[32,16] parameter(1)
      x_f32 = f32[16,32] convert(x)
      y_f32 = f32[32,16] convert(y)
      x_scale = f32[] parameter(2)
      y_scale = f32[] parameter(3)
      z_scale = f32[] parameter(4)
      x_scale_bcast = f32[16,32] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[32,16] broadcast(y_scale), dimensions={}
      z_scale_bcast = f32[16,16] broadcast(z_scale), dimensions={}
      x_unscaled = f32[16,32] multiply(x_f32, x_scale_bcast)
      y_unscaled = f32[32,16] multiply(y_f32, y_scale_bcast)
      dot_a = f32[16,16] dot(x_unscaled, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}
      dot_a_scaled = f32[16,16] multiply(dot_a, z_scale_bcast)
      c1 = f32[] constant(-448.)
      c1_bcast = f32[16,16] broadcast(c1), dimensions={}
      c2 = f32[] constant(448.)
      c2_bcast = f32[16,16] broadcast(c2), dimensions={}
      dot_a_clamped = f32[16,16] clamp(c1_bcast, dot_a_scaled, c2_bcast)
      ROOT dot_a_f8 = f8e4m3fn[16,16] convert(dot_a_clamped)
          }

)";

  CheckFp8IfSupported(hlo_text);
  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(

; CHECK-NOT:     divide

; CHECK:           custom_call_target="__cublas$lt$matmul$f8",

      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest, ScaledABScaledDReluActivationF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif  // CUDA_VERSION < 12000
  const char* hlo_text = R"(
    HloModule test
    ENTRY test {
      x = f8e4m3fn[16,32] parameter(0)
      y = f8e4m3fn[32,16] parameter(1)
      x_f32 = f32[16,32] convert(x)
      y_f32 = f32[32,16] convert(y)
      x_scale = f32[] parameter(2)
      y_scale = f32[] parameter(3)
      z_scale = f32[] parameter(4)
      x_scale_bcast = f32[16,32] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[32,16] broadcast(y_scale), dimensions={}
      z_scale_bcast = f32[16,16] broadcast(z_scale), dimensions={}
      x_unscaled = f32[16,32] multiply(x_f32, x_scale_bcast)
      y_unscaled = f32[32,16] multiply(y_f32, y_scale_bcast)
      c = f32[] constant(0)
      c_bcast = f32[16,16] broadcast(c), dimensions={}
      dot_a = f32[16,16] dot(x_unscaled, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}
      relu_a = f32[16,16] maximum(dot_a, c_bcast)
      relu_a_scaled = f32[16,16] divide(relu_a, z_scale_bcast)
      c1 = f32[] constant(-448.)
      c1_bcast = f32[16,16] broadcast(c1), dimensions={}
      c2 = f32[] constant(448.)
      c2_bcast = f32[16,16] broadcast(c2), dimensions={}
      relu_a_clamped = f32[16,16] clamp(c1_bcast, relu_a_scaled, c2_bcast)
      ROOT out = f8e4m3fn[16,16] convert(relu_a_clamped)
          }
)";

  CheckFp8IfSupported(hlo_text);
  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(
; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[16,32], y: f8e4m3fn[32,16], x_scale: f32[], y_scale: f32[], z_scale: f32[]) -> f8e4m3fn[16,16] {
; CHECK:         [[P0:%[^ ]+]] = f8e4m3fn[16,32]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e4m3fn[32,16]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_TRANSPOSE:%[^ ]+]] = f8e4m3fn[16,32]{1,0} transpose([[P1]]), dimensions={1,0}
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[] parameter(2)
; CHECK-NEXT:    [[P3:%[^ ]+]] = f32[] parameter(3)
; CHECK-NEXT:    [[C1:%[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    [[C2:%[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    [[P4:%[^ ]+]] = f32[] parameter(4)
; CHECK-NEXT:    [[P4_INV:%[^ ]+]] = f32[] divide([[C2]], [[P4]])
; CHECK-NEXT:    ROOT [[OUT:%[^ ]+]] = f8e4m3fn[16,16]{1,0} custom-call([[P0]], [[P1_TRANSPOSE]], [[P2]], [[P3]], [[C1]], /*index=5*/[[P4_INV]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"RELU"
; CHECK:           }
      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest, ScaledABScaledDMatrixBiasF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif  // CUDA_VERSION < 12000
  const char* hlo_text = R"(
    HloModule test

    ENTRY test {
      x = f8e4m3fn[16,32] parameter(0)
      y = f8e4m3fn[32,16] parameter(1)
      x_f16 = f16[16,32] convert(x)
      y_f16 = f16[32,16] convert(y)
      b = f16[16,16] parameter(2)
      x_scale = f16[] parameter(3)
      y_scale = f16[] parameter(4)
      z_scale = f16[] parameter(5)
      x_scale_bcast = f16[16,32] broadcast(x_scale), dimensions={}
      y_scale_bcast = f16[32,16] broadcast(y_scale), dimensions={}
      z_scale_bcast = f16[16,16] broadcast(z_scale), dimensions={}
      x_unscaled = f16[16,32] multiply(x_f16, x_scale_bcast)
      y_unscaled = f16[32,16] multiply(y_f16, y_scale_bcast)
      dot_a = f16[16,16] dot(x_unscaled, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}
      dot_a_bias = f16[16,16] add(dot_a, b)
      dot_a_scaled = f16[16,16] divide(dot_a_bias, z_scale_bcast)
      c1 = f16[] constant(-448.)
      c1_bcast = f16[16,16] broadcast(c1), dimensions={}
      c2 = f16[] constant(448.)
      c2_bcast = f16[16,16] broadcast(c2), dimensions={}
      dot_a_clamped = f16[16,16] clamp(c1_bcast, dot_a_scaled, c2_bcast)
      ROOT dot_a_f8 = f8e4m3fn[16,16] convert(dot_a_clamped)
          }

)";

  CheckFp8IfSupported(hlo_text, ErrorSpec{0.1, 0.1});
  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(

; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[16,32], y: f8e4m3fn[32,16], b: f16[16,16], x_scale: f16[], y_scale: f16[], z_scale: f16[]) -> f8e4m3fn[16,16] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f8e4m3fn[16,32]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e4m3fn[32,16]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_TRANSPOSE:%[^ ]+]] = f8e4m3fn[16,32]{1,0} transpose([[P1]]), dimensions={1,0}
; CHECK-NEXT:    [[C0:%[^ ]+]] = f16[16,16]{1,0} parameter(2)
; CHECK-NEXT:    [[P2:%[^ ]+]] = f16[] parameter(3)
; CHECK:         [[P3:%[^ ]+]] = f16[] parameter(4)
; CHECK:         [[C1:%[^ ]+]] = f32[] constant(1)
; CHECK:         [[P4:%[^ ]+]] = f16[] parameter(5)
; CHECK:       ROOT [[OUT:%[^ ]+]] = f8e4m3fn[16,16]{1,0} custom-call([[P0]], [[P1_TRANSPOSE]], [[C0]], [[DUMMY0:%[^ ]+]], [[DUMMY1:%[^ ]+]], /*index=5*/[[C1]], [[DUMMY2:%[^ ]+]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest, ScaledABScaledDVectorBiasF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif  // CUDA_VERSION < 12000
  const char* hlo_text = R"(
    HloModule test

    ENTRY test {
      x = f8e4m3fn[16,32] parameter(0)
      y = f8e4m3fn[32,16] parameter(1)
      x_f16 = f16[16,32] convert(x)
      y_f16 = f16[32,16] convert(y)
      b = f16[16] parameter(2)
      b_bcast = f16[16,16] broadcast(b), dimensions={1}
      x_scale = f16[] parameter(3)
      y_scale = f16[] parameter(4)
      z_scale = f16[] parameter(5)
      x_scale_bcast = f16[16,32] broadcast(x_scale), dimensions={}
      y_scale_bcast = f16[32,16] broadcast(y_scale), dimensions={}
      z_scale_bcast = f16[16,16] broadcast(z_scale), dimensions={}
      x_unscaled = f16[16,32] multiply(x_f16, x_scale_bcast)
      y_unscaled = f16[32,16] multiply(y_f16, y_scale_bcast)
      dot_a = f16[16,16] dot(x_unscaled, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}
      dot_a_bias = f16[16,16] add(dot_a, b_bcast)
      dot_a_scaled = f16[16,16] divide(dot_a_bias, z_scale_bcast)
      c1 = f16[] constant(-448.)
      c1_bcast = f16[16,16] broadcast(c1), dimensions={}
      c2 = f16[] constant(448.)
      c2_bcast = f16[16,16] broadcast(c2), dimensions={}
      dot_a_clamped = f16[16,16] clamp(c1_bcast, dot_a_scaled, c2_bcast)
      ROOT dot_a_f8 = f8e4m3fn[16,16] convert(dot_a_clamped)
          }

)";

  CheckFp8IfSupported(hlo_text, ErrorSpec{0.1, 0.1});
  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(

; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[16,32], y: f8e4m3fn[32,16], b: f16[16], x_scale: f16[], y_scale: f16[], z_scale: f16[]) -> f8e4m3fn[16,16] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f8e4m3fn[16,32]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e4m3fn[32,16]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_TRANSPOSE:%[^ ]+]] = f8e4m3fn[16,32]{1,0} transpose([[P1]]), dimensions={1,0}
; CHECK-NEXT:    [[P2:%[^ ]+]] = f16[] parameter(3)
; CHECK-NEXT:    [[CV:%[^ ]+]] = f32[] convert([[P2]])
; CHECK-NEXT:    [[P3:%[^ ]+]] = f16[] parameter(4)
; CHECK-NEXT:    [[CV1:%[^ ]+]] = f32[] convert([[P3]])
; CHECK-NEXT:    [[C:%[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    [[C2:%[^ ]+]] = f16[] constant(1)
; CHECK-NEXT:    [[P4:%[^ ]+]] = f16[] parameter(5)
; CHECK-NEXT:    [[DV:%[^ ]+]] = f16[] divide([[C2]], [[P4]])
; CHECK-NEXT:    [[CV2:%[^ ]+]] = f32[] convert([[DV]])
; CHECK-NEXT:    [[VB:%[^ ]+]] = f16[16]{0} parameter(2)
; CHECK:         ROOT [[OUT:%[^ ]+]] = f8e4m3fn[16,16]{1,0} custom-call([[P0]], [[P1_TRANSPOSE]], [[CV]], [[CV1]], [[C]], /*index=5*/[[CV2]], [[VB]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"BIAS"
; CHECK:           }
      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest, ScaledABUnscaledDF32VectorBiasF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif  // CUDA_VERSION < 12000
  const char* hlo_text = R"(
    HloModule test

    ENTRY test {
      x = f8e4m3fn[16,32] parameter(0)
      y = f8e4m3fn[32,16] parameter(1)
      x_f32 = f32[16,32] convert(x)
      y_f32 = f32[32,16] convert(y)
      b = f32[16] parameter(2)
      b_bf16 = bf16[16] convert(b)
      b_f32 = f32[16] convert(b_bf16)
      b_bcast = f32[16,16] broadcast(b_f32), dimensions={1}
      x_scale = f32[] parameter(3)
      y_scale = f32[] parameter(4)
      x_scale_bcast = f32[16,32] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[32,16] broadcast(y_scale), dimensions={}
      x_unscaled = f32[16,32] multiply(x_f32, x_scale_bcast)
      y_unscaled = f32[32,16] multiply(y_f32, y_scale_bcast)
      dot_a = f32[16,16] dot(x_unscaled, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}
      ROOT out = f32[16,16] add(dot_a, b_bcast)
           }

)";

  CheckFp8IfSupported(hlo_text);
  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(
; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[16,32], y: f8e4m3fn[32,16], b: f32[16], x_scale: f32[], y_scale: f32[]) -> f32[16,16] {
; CHECK:         [[P0:%[^ ]+]] = f8e4m3fn[16,32]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e4m3fn[32,16]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_TRANSPOSE:%[^ ]+]] = f8e4m3fn[16,32]{1,0} transpose([[P1]]), dimensions={1,0}
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[] parameter(3)
; CHECK-NEXT:    [[P3:%[^ ]+]] = f32[] parameter(4)
; CHECK-NEXT:    [[C:%[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    [[VB:%[^ ]+]] = f32[16]{0} parameter(2)
; CHECK-NEXT:    [[VBC:%[^ ]+]] = bf16[16]{0} convert([[VB]])
; CHECK:         ROOT [[OUT:%[^ ]+]] = f32[16,16]{1,0} custom-call([[P0]], [[P1_TRANSPOSE]], [[P2]], [[P3]], [[C]], /*index=5*/[[C]], [[VBC]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"BIAS"
; CHECK:           }
      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest,
       ScaledABUnscaledDVectorBiasThenReluActivationF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif  // CUDA_VERSION < 12000
  const char* hlo_text = R"(
    HloModule test

    ENTRY test {
      x = f8e4m3fn[16,32] parameter(0)
      y = f8e4m3fn[32,16] parameter(1)
      b = f16[16] parameter(2)
      b_bcast = f16[16,16] broadcast(b), dimensions={1}
      x_f32 = f16[16,32] convert(x)
      y_f32 = f16[32,16] convert(y)
      x_scale = f16[] parameter(3)
      y_scale = f16[] parameter(4)
      x_scale_bcast = f16[16,32] broadcast(x_scale), dimensions={}
      y_scale_bcast = f16[32,16] broadcast(y_scale), dimensions={}
      x_unscaled = f16[16,32] multiply(x_f32, x_scale_bcast)
      y_unscaled = f16[32,16] multiply(y_f32, y_scale_bcast)
      c = f16[] constant(0)
      c_bcast = f16[16,16] broadcast(c), dimensions={}
      dot_a0 = f16[16,16] dot(x_unscaled, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}
      dot_a = f16[16,16] add(dot_a0, b_bcast)
      ROOT out = f16[16,16] maximum(dot_a, c_bcast)
          }
)";

  CheckFp8IfSupported(hlo_text, ErrorSpec{2e-3, 0.});
  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(
; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[16,32], y: f8e4m3fn[32,16], b: f16[16], x_scale: f16[], y_scale: f16[]) -> f16[16,16] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f8e4m3fn[16,32]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e4m3fn[32,16]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_TRANSPOSE:%[^ ]+]] = f8e4m3fn[16,32]{1,0} transpose([[P1]]), dimensions={1,0}
; CHECK-NEXT:    [[P2:%[^ ]+]] = f16[] parameter(3)
; CHECK-NEXT:    [[CV:%[^ ]+]] = f32[] convert([[P2]])
; CHECK-NEXT:    [[P3:%[^ ]+]] = f16[] parameter(4)
; CHECK-NEXT:    [[CV1:%[^ ]+]] = f32[] convert([[P3]])
; CHECK-NEXT:    [[C:%[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    [[VB:%[^ ]+]] = f16[16]{0} parameter(2)
; CHECK     :    ROOT [[OUT:%[^ ]+]] = f16[16,16]{1,0} custom-call([[P0]], [[P1_TRANSPOSE]], [[CV]], [[CV1]], [[C]], /*index=5*/[[C]], [[VB]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"BIAS_RELU"
; CHECK:           }
      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest, Rank3ScaledABUnscaledDVectorBiasF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "A matrix bias on a matmul is only supported in CUDA 12";
#endif  // CUDA_VERSION < 12000
  const char* hlo_text = R"(
    HloModule test
    ENTRY test {
      x = f8e4m3fn[4,16,16] parameter(0)
      y = f8e4m3fn[16,32] parameter(1)
      b = f32[32] parameter(2)
      b_f16 = f16[32] convert(b)
      b_bcast = f16[4,16,32] broadcast(b_f16), dimensions={2}
      x_f16 = f16[4,16,16] convert(x)
      y_f16 = f16[16,32] convert(y)
      x_scale = f16[] parameter(3)
      y_scale = f16[] parameter(4)
      x_scale_bcast = f16[4,16,16] broadcast(x_scale), dimensions={}
      y_scale_bcast = f16[16,32] broadcast(y_scale), dimensions={}
      x_unscaled = f16[4,16,16] multiply(x_f16, x_scale_bcast)
      x_unscaled_bitcast = f16[64,16] bitcast(x_unscaled)
      y_unscaled = f16[16,32] multiply(y_f16, y_scale_bcast)
      dot_a = f16[64,32] dot(x_unscaled_bitcast, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}
      dot_a_bitcast = f16[4,16,32]{2,1,0} bitcast(dot_a)
      ROOT out = f16[4,16,32] add(dot_a_bitcast, b_bcast)
          }
)";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  GemmRewriter pass(CudaHopperOrRocm());
  TF_ASSERT_OK_AND_ASSIGN(bool changed, this->RunHloPass(&pass, module.get()));
  EXPECT_TRUE(changed);

  EXPECT_THAT(module->entry_computation()->root_instruction(),
              GmockMatch(m::Bitcast(m::CustomCall({"__cublas$lt$matmul$f8"})
                                        .WithShape(F16, {64, 32}))
                             .WithShape(F16, {4, 16, 32})));

  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(
; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[4,16,16], y: f8e4m3fn[16,32], b: f32[32], x_scale: f16[], y_scale: f16[]) -> f16[4,16,32] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f8e4m3fn[4,16,16]{2,1,0} parameter(0)
; CHECK-NEXT:    [[P0_BITCAST:%[^ ]+]] = f8e4m3fn[64,16]{1,0} bitcast([[P0]])
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e4m3fn[16,32]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_TRANSPOSE:%[^ ]+]] = f8e4m3fn[32,16]{1,0} transpose([[P1]]), dimensions={1,0}
; CHECK-NEXT:    [[P2:%[^ ]+]] = f16[] parameter(3)
; CHECK-NEXT:    [[P2_CV:%[^ ]+]] = f32[] convert([[P2]])
; CHECK-NEXT:    [[P3:%[^ ]+]] = f16[] parameter(4)
; CHECK-NEXT:    [[P3_CV:%[^ ]+]] = f32[] convert([[P3]])
; CHECK-NEXT:    [[C:%[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    [[B:%[^ ]+]] = f32[32]{0} parameter(2)
; CHECK-NEXT:    [[B_F16:%[^ ]+]] = f16[32]{0} convert([[B]])
; CHECK-NEXT:    [[GEMM:%[^ ]+]] = f16[64,32]{1,0} custom-call([[P0_BITCAST]], [[P1_TRANSPOSE]], [[P2_CV]], [[P3_CV]], [[C]], /*index=5*/[[C]], [[B_F16]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"BIAS"
; CHECK:           }
; CHECK:         ROOT [[OUT:%[^ ]+]] = f16[4,16,32]{2,1,0} bitcast([[GEMM]])
      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest,
       Rank3ScaledABUnscaledDVectorBiasPaddedF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "A matrix bias on a matmul is only supported in CUDA 12";
#endif
  const char* hlo_text = R"(
    HloModule test
    ENTRY test {
      x = f8e4m3fn[4,15,15] parameter(0)
      y = f8e4m3fn[15,31] parameter(1)
      b = f32[31] parameter(2)
      b_f16 = f16[31] convert(b)
      b_bcast = f16[4,15,31] broadcast(b_f16), dimensions={2}
      x_f16 = f16[4,15,15] convert(x)
      y_f16 = f16[15,31] convert(y)
      x_scale = f16[] parameter(3)
      y_scale = f16[] parameter(4)
      x_scale_bcast = f16[4,15,15] broadcast(x_scale), dimensions={}
      y_scale_bcast = f16[15,31] broadcast(y_scale), dimensions={}
      x_unscaled = f16[4,15,15] multiply(x_f16, x_scale_bcast)
      x_unscaled_bitcast = f16[60,15] bitcast(x_unscaled)
      y_unscaled = f16[15,31] multiply(y_f16, y_scale_bcast)
      dot_a = f16[60,31] dot(x_unscaled_bitcast, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}
      dot_a_bitcast = f16[4,15,31]{2,1,0} bitcast(dot_a)
      ROOT out = f16[4,15,31] add(dot_a_bitcast, b_bcast)
          }
)";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  GemmRewriter pass(CudaHopperOrRocm());
  TF_ASSERT_OK_AND_ASSIGN(bool changed, this->RunHloPass(&pass, module.get()));
  EXPECT_TRUE(changed);

  EXPECT_THAT(
      module->entry_computation()->root_instruction(),
      GmockMatch(m::Bitcast(m::Slice(m::CustomCall({"__cublas$lt$matmul$f8"})
                                         .WithShape(F16, {64, 32}))
                                .WithShape(F16, {60, 31}))
                     .WithShape(F16, {4, 15, 31})));

  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(
; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[4,15,15], y: f8e4m3fn[15,31], b: f32[31], x_scale: f16[], y_scale: f16[]) -> f16[4,15,31] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f8e4m3fn[4,15,15]{2,1,0} parameter(0)
; CHECK-NEXT:    [[P0_BITCAST:%[^ ]+]] = f8e4m3fn[60,15]{1,0} bitcast([[P0]])
; CHECK-NEXT:    [[C1:%[^ ]+]] = f8e4m3fn[] constant(0)
; CHECK-NEXT:    [[P0_PAD:%[^ ]+]] = f8e4m3fn[64,16]{1,0} pad([[P0_BITCAST]], [[C1]]), padding=0_4x0_1
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e4m3fn[15,31]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_TRANSPOSE:%[^ ]+]] = f8e4m3fn[31,15]{1,0} transpose([[P1]]), dimensions={1,0}
; CHECK-NEXT:    [[C2:%[^ ]+]] = f8e4m3fn[] constant(0)
; CHECK-NEXT:    [[P1_PAD:%[^ ]+]] = f8e4m3fn[32,16]{1,0} pad([[P1_TRANSPOSE]], [[C2]]), padding=0_1x0_1
; CHECK-NEXT:    [[P2:%[^ ]+]] = f16[] parameter(3)
; CHECK-NEXT:    [[P2_CV:%[^ ]+]] = f32[] convert([[P2]])
; CHECK-NEXT:    [[P3:%[^ ]+]] = f16[] parameter(4)
; CHECK-NEXT:    [[P3_CV:%[^ ]+]] = f32[] convert([[P3]])
; CHECK-NEXT:    [[C:%[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    [[B:%[^ ]+]] = f32[31]{0} parameter(2)
; CHECK-NEXT:    [[B_F16:%[^ ]+]] = f16[31]{0} convert([[B]])
; CHECK-NEXT:    [[C3:%[^ ]+]] = f16[] constant(0)
; CHECK-NEXT:    [[P2_PAD:%[^ ]+]] = f16[32]{0} pad([[B_F16]], [[C3]]), padding=0_1
; CHECK-NEXT:    [[GEMM:%[^ ]+]] = f16[64,32]{1,0} custom-call([[P0_PAD]], [[P1_PAD]], [[P2_CV]], [[P3_CV]], [[C]], /*index=5*/[[C]], [[P2_PAD]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"BIAS"
; CHECK:           }
; CHECK-NEXT:     [[SLICE:%[^ ]+]] = f16[60,31]{1,0} slice([[GEMM]]), slice={[0:60], [0:31]}
; CHECK-NEXT:     ROOT [[OUT:%[^ ]+]] = f16[4,15,31]{2,1,0} bitcast([[SLICE]])
      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest, Rank3ScaledABUnscaledDMatrixBiasF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "A matrix bias on a matmul is only supported in CUDA 12";
#endif
  const char* hlo_text = R"(
    HloModule test
    ENTRY test {
      x = f8e4m3fn[4,16,16] parameter(0)
      y = f8e4m3fn[16,32] parameter(1)
      b = f32[4,16,32] parameter(2)
      x_f32 = f32[4,16,16] convert(x)
      y_f32 = f32[16,32] convert(y)
      x_scale = f32[] parameter(3)
      y_scale = f32[] parameter(4)
      x_scale_bcast = f32[4,16,16] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[16,32] broadcast(y_scale), dimensions={}
      x_unscaled = f32[4,16,16] multiply(x_f32, x_scale_bcast)
      x_unscaled_bitcast = f32[64,16] bitcast(x_unscaled)
      y_unscaled = f32[16,32] multiply(y_f32, y_scale_bcast)
      dot_a = f32[64,32] dot(x_unscaled_bitcast, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}
      dot_a_bitcast = f32[4,16,32]{2,1,0} bitcast(dot_a)
      ROOT out = f32[4,16,32] add(dot_a_bitcast, b)
          }
)";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  GemmRewriter pass(CudaHopperOrRocm());
  TF_ASSERT_OK_AND_ASSIGN(bool changed, this->RunHloPass(&pass, module.get()));
  EXPECT_TRUE(changed);

  EXPECT_THAT(module->entry_computation()->root_instruction(),
              GmockMatch(m::Bitcast(m::CustomCall({"__cublas$lt$matmul$f8"})
                                        .WithShape(F32, {64, 32}))
                             .WithShape(F32, {4, 16, 32})));

  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(
; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[4,16,16], y: f8e4m3fn[16,32], b: f32[4,16,32], x_scale: f32[], y_scale: f32[]) -> f32[4,16,32] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f8e4m3fn[4,16,16]{2,1,0} parameter(0)
; CHECK-NEXT:    [[P0_BITCAST:%[^ ]+]] = f8e4m3fn[64,16]{1,0} bitcast([[P0]])
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e4m3fn[16,32]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_TRANSPOSE:%[^ ]+]] = f8e4m3fn[32,16]{1,0} transpose([[P1]]), dimensions={1,0}
; CHECK-NEXT:    [[B:%[^ ]+]] = f32[4,16,32]{2,1,0} parameter(2)
; CHECK-NEXT:    [[B_BITCAST:%[^ ]+]] = f32[64,32]{1,0} bitcast([[B]])
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[] parameter(3)
; CHECK-NEXT:    [[P3:%[^ ]+]] = f32[] parameter(4)
; CHECK-NEXT:    [[C:%[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    [[GEMM:%[^ ]+]] = f32[64,32]{1,0} custom-call([[P0_BITCAST]], [[P1_TRANSPOSE]], [[B_BITCAST]], [[P2]], [[P3]], /*index=5*/[[C]], [[C]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
; CHECK:         ROOT [[OUT:%[^ ]+]] = f32[4,16,32]{2,1,0} bitcast([[GEMM]])
      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest,
       Rank3ScaledABUnscaledDMatrixBiasPaddedF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "A matrix bias on a matmul is only supported in CUDA 12";
#endif
  const char* hlo_text = R"(
    HloModule test
    ENTRY test {
      x = f8e4m3fn[3,15,15] parameter(0)
      y = f8e4m3fn[15,31] parameter(1)
      b = f32[3,15,31] parameter(2)
      x_f32 = f32[3,15,15] convert(x)
      y_f32 = f32[15,31] convert(y)
      x_scale = f32[] parameter(3)
      y_scale = f32[] parameter(4)
      x_scale_bcast = f32[3,15,15] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[15,31] broadcast(y_scale), dimensions={}
      x_unscaled = f32[3,15,15] multiply(x_f32, x_scale_bcast)
      x_unscaled_bitcast = f32[45,15] bitcast(x_unscaled)
      y_unscaled = f32[15,31] multiply(y_f32, y_scale_bcast)
      dot_a = f32[45,31] dot(x_unscaled_bitcast, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}
      dot_a_bitcast = f32[3,15,31]{2,1,0} bitcast(dot_a)
      ROOT out = f32[3,15,31] add(dot_a_bitcast, b)
          }
)";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  GemmRewriter pass(CudaHopperOrRocm());
  TF_ASSERT_OK_AND_ASSIGN(bool changed, this->RunHloPass(&pass, module.get()));
  EXPECT_TRUE(changed);

  EXPECT_THAT(
      module->entry_computation()->root_instruction(),
      GmockMatch(m::Bitcast(m::Slice(m::CustomCall({"__cublas$lt$matmul$f8"})
                                         .WithShape(F32, {48, 32}))
                                .WithShape(F32, {45, 31}))
                     .WithShape(F32, {3, 15, 31})));

  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(
; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[3,15,15], y: f8e4m3fn[15,31], b: f32[3,15,31], x_scale: f32[], y_scale: f32[]) -> f32[3,15,31] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f8e4m3fn[3,15,15]{2,1,0} parameter(0)
; CHECK-NEXT:    [[P0_BITCAST:%[^ ]+]] = f8e4m3fn[45,15]{1,0} bitcast([[P0]])
; CHECK-NEXT:    [[C1:%[^ ]+]] = f8e4m3fn[] constant(0)
; CHECK-NEXT:    [[P0_PADDED:%[^ ]+]] = f8e4m3fn[48,16]{1,0} pad([[P0_BITCAST]], [[C1]]), padding=0_3x0_1
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e4m3fn[15,31]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_TRANSPOSE:%[^ ]+]] = f8e4m3fn[31,15]{1,0} transpose([[P1]]), dimensions={1,0}
; CHECK-NEXT:    [[C2:%[^ ]+]] = f8e4m3fn[] constant(0)
; CHECK-NEXT:    [[P1_PADDED:%[^ ]+]] = f8e4m3fn[32,16]{1,0} pad([[P1_TRANSPOSE]], [[C2]]), padding=0_1x0_1
; CHECK-NEXT:    [[B:%[^ ]+]] = f32[3,15,31]{2,1,0} parameter(2)
; CHECK-NEXT:    [[B_BITCAST:%[^ ]+]] = f32[45,31]{1,0} bitcast([[B]])
; CHECK-NEXT:    [[C3:%[^ ]+]] = f32[] constant(0)
; CHECK-NEXT:    [[P2_PADDED:%[^ ]+]] = f32[48,32]{1,0} pad([[B_BITCAST]], [[C3]]), padding=0_3x0_1
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[] parameter(3)
; CHECK-NEXT:    [[P3:%[^ ]+]] = f32[] parameter(4)
; CHECK-NEXT:    [[C:%[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    [[GEMM:%[^ ]+]] = f32[48,32]{1,0} custom-call([[P0_PADDED]], [[P1_PADDED]], [[P2_PADDED]], [[P2]], [[P3]], /*index=5*/[[C]], [[C]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
; CHECK-NEXT:      [[SLICE:%[^ ]+]] = f32[45,31]{1,0} slice([[GEMM]]), slice={[0:45], [0:31]}
; CHECK-NEXT:      ROOT [[OUT:%[^ ]+]] = f32[3,15,31]{2,1,0} bitcast([[SLICE]])
      )");
}

// Do not fuse matrix bias When there is a slice that does not chop off the ends
// of dimensions.
TEST_P(ParameterizedFp8GemmRewriteTest,
       ScaledABUnscaledDMatrixBiasWithSliceF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "A matrix bias on a matmul is only supported in CUDA 12";
#endif
  const char* hlo_text = R"(
    HloModule test
    ENTRY test {
      x = f8e4m3fn[48,16] parameter(0)
      y = f8e4m3fn[16,32] parameter(1)
      b = f32[32,16] parameter(2)
      x_f32 = f32[48,16] convert(x)
      y_f32 = f32[16,32] convert(y)
      x_scale = f32[] parameter(3)
      y_scale = f32[] parameter(4)
      x_scale_bcast = f32[48,16] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[16,32] broadcast(y_scale), dimensions={}
      x_unscaled = f32[48,16] multiply(x_f32, x_scale_bcast)
      y_unscaled = f32[16,32] multiply(y_f32, y_scale_bcast)
      dot_a = f32[48,32] dot(x_unscaled, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}
      dot_a_sliced = f32[32,16] slice(dot_a), slice={[16:48], [16:32]}
      ROOT out = f32[32,16] add(dot_a_sliced, b)
          }
)";

  TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> module,
                          ParseAndReturnVerifiedModule(hlo_text));
  GemmRewriter pass(CudaHopperOrRocm());
  TF_ASSERT_OK_AND_ASSIGN(bool changed, this->RunHloPass(&pass, module.get()));
  EXPECT_TRUE(changed);

  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(
; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[48,16], y: f8e4m3fn[16,32], b: f32[32,16], x_scale: f32[], y_scale: f32[]) -> f32[32,16] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f8e4m3fn[48,16]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e4m3fn[16,32]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_TRANSPOSE:%[^ ]+]] = f8e4m3fn[32,16]{1,0} transpose([[P1]]), dimensions={1,0}
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[] parameter(3)
; CHECK-NEXT:    [[P3:%[^ ]+]] = f32[] parameter(4)
; CHECK-NEXT:    [[C:%[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    [[GEMM:%[^ ]+]] = f32[48,32]{1,0} custom-call([[P0]], [[P1_TRANSPOSE]], [[P2]], [[P3]], [[C]], /*index=5*/[[C]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
; CHECK-NEXT:      [[SLICE:%[^ ]+]] = f32[32,16]{1,0} slice([[GEMM]]), slice={[16:48], [16:32]}
; CHECK-NEXT:      [[B:%[^ ]+]] = f32[32,16]{1,0} parameter(2)
; CHECK-NEXT:      ROOT [[OUT:%[^ ]+]] = f32[32,16]{1,0} add([[SLICE]], [[B]])
      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest, ScaledABUnscaledDWithAllGatherF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "A matrix bias on a matmul is only supported in CUDA 12";
#endif
  absl::string_view hlo_text = R"(
    HloModule test

    ENTRY test {
      x = f8e4m3fn[16,32] parameter(0)
      y = f8e4m3fn[16,32] parameter(1)
      x_f32 = f32[16,32] convert(x)
      y_f32 = f32[16,32] convert(y)
      x_scale = f32[] parameter(2)
      y_scale = f32[] parameter(3)
      x_scale_bcast = f32[16,32] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[16,32] broadcast(y_scale), dimensions={}
      x_unscaled = f32[16,32] multiply(x_f32, x_scale_bcast)
      y_unscaled = f32[16,32] multiply(y_f32, y_scale_bcast)
      all_gather = f32[16,64]{1,0} all-gather(x_unscaled), channel_id=1, replica_groups={{0,1},{2,3},{4,5},{6,7}}, dimensions={1}, use_global_device_ids=true
      all_gather1 = f32[64,32]{1,0} all-gather(y_unscaled), channel_id=2, replica_groups={{0,2,4,6},{1,3,5,7}}, dimensions={0}, use_global_device_ids=true
      ROOT dot_a = f32[16,32] dot(all_gather, all_gather1), lhs_contracting_dims={1}, rhs_contracting_dims={0}
          }
)";

  HloModuleConfig config = GetModuleConfigForTest();
  config.set_use_spmd_partitioning(true);
  config.set_num_partitions(8);

  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(
; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[16,32], y: f8e4m3fn[16,32], x_scale: f32[], y_scale: f32[]) -> f32[16,32] {
; CHECK:         [[P0:%[^ ]+]] = f8e4m3fn[16,32]{1,0} parameter(0)
; CHECK:         [[AG:%[^ ]+]] = f8e4m3fn[16,64]{1,0} all-gather([[P0]]), {{[^ ]+}}
; CHECK:         [[P1:%[^ ]+]] = f8e4m3fn[16,32]{1,0} parameter(1)
; CHECK:         [[AG1:%[^ ]+]] = f8e4m3fn[64,32]{1,0} all-gather([[P1]]), {{[^ ]+}}
; CHECK:         [[P1_TRANSPOSE:%[^ ]+]] = f8e4m3fn[32,64]{1,0} transpose([[AG1]]), dimensions={1,0}
; CHECK:         [[P2:%[^ ]+]] = f32[] parameter(2)
; CHECK:         [[P3:%[^ ]+]] = f32[] parameter(3)
; CHECK:         [[C:%[^ ]+]] = f32[] constant(1)
; CHECK:         ROOT [[GEMM:%[^ ]+]] = f32[16,32]{1,0} custom-call([[AG]], [[P1_TRANSPOSE]], [[P2]], [[P3]], [[C]], /*index=5*/[[C]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
      )",
                            nullptr, &config);
}

TEST_P(ParameterizedFp8GemmRewriteTest, ScaledABUnscaledDWithAllToAllF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "A matrix bias on a matmul is only supported in CUDA 12";
#endif
  absl::string_view hlo_text = R"(
    HloModule test

    ENTRY test {
      x = f8e4m3fn[16,32] parameter(0)
      y = f8e4m3fn[16,32] parameter(1)
      x_f32 = f32[16,32] convert(x)
      y_f32 = f32[16,32] convert(y)
      x_scale = f32[] parameter(2)
      y_scale = f32[] parameter(3)
      x_scale_bcast = f32[16,32] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[16,32] broadcast(y_scale), dimensions={}
      x_unscaled = f32[16,32] multiply(x_f32, x_scale_bcast)
      y_unscaled = f32[16,32] multiply(y_f32, y_scale_bcast)
      all_to_all = f32[16,32]{1,0} all-to-all(x_unscaled), channel_id=1, replica_groups={{0,1,2,3},{4,5,6,7}}, dimensions={0}
      ROOT dot_a = f32[16,16] dot(all_to_all, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={1}
          }
)";

  HloModuleConfig config = GetModuleConfigForTest();
  config.set_use_spmd_partitioning(true);
  config.set_num_partitions(8);

  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(
; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[16,32], y: f8e4m3fn[16,32], x_scale: f32[], y_scale: f32[]) -> f32[16,16] {
; CHECK:         [[P0:%[^ ]+]] = f8e4m3fn[16,32]{1,0} parameter(0)
; CHECK:         [[AA:%[^ ]+]] = f8e4m3fn[16,32]{1,0} all-to-all([[P0]]), {{[^ ]+}}
; CHECK:         [[P1:%[^ ]+]] = f8e4m3fn[16,32]{1,0} parameter(1)
; CHECK:         [[P2:%[^ ]+]] = f32[] parameter(2)
; CHECK:         [[P3:%[^ ]+]] = f32[] parameter(3)
; CHECK:         [[C:%[^ ]+]] = f32[] constant(1)
; CHECK:         ROOT [[GEMM:%[^ ]+]] = f32[16,16]{1,0} custom-call([[AA]], [[P1]], [[P2]], [[P3]], [[C]], /*index=5*/[[C]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
      )",
                            nullptr, &config);
}

TEST_P(ParameterizedFp8GemmRewriteTest,
       ScaledABUnscaledDWithCollectivePermuteF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif
  absl::string_view hlo_text = R"(
    HloModule test

    ENTRY test {
      x = f8e4m3fn[16,32] parameter(0)
      y = f8e4m3fn[16,32] parameter(1)
      x_f32 = f32[16,32] convert(x)
      y_f32 = f32[16,32] convert(y)
      x_scale = f32[] parameter(2)
      y_scale = f32[] parameter(3)
      x_scale_bcast = f32[16,32] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[16,32] broadcast(y_scale), dimensions={}
      x_unscaled = f32[16,32] multiply(x_f32, x_scale_bcast)
      y_unscaled = f32[16,32] multiply(y_f32, y_scale_bcast)
      collective_permute = f32[16,32]{1,0} collective-permute(x_unscaled), source_target_pairs={{0,0}, {1,1}, {2,4}, {3,5}, {4,2}, {5,3}, {6,6}, {7,7}}
      ROOT dot_a = f32[16,16] dot(collective_permute, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={1}
          }
)";

  HloModuleConfig config = GetModuleConfigForTest();
  config.set_use_spmd_partitioning(true);
  config.set_num_partitions(8);

  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(
; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[16,32], y: f8e4m3fn[16,32], x_scale: f32[], y_scale: f32[]) -> f32[16,16] {
; CHECK:         [[P0:%[^ ]+]] = f8e4m3fn[16,32]{1,0} parameter(0)
; CHECK:         [[AA:%[^ ]+]] = f8e4m3fn[16,32]{1,0} collective-permute([[P0]]), {{[^ ]+}}
; CHECK:         [[P1:%[^ ]+]] = f8e4m3fn[16,32]{1,0} parameter(1)
; CHECK:         [[P2:%[^ ]+]] = f32[] parameter(2)
; CHECK:         [[P3:%[^ ]+]] = f32[] parameter(3)
; CHECK:         [[C:%[^ ]+]] = f32[] constant(1)
; CHECK:         ROOT [[GEMM:%[^ ]+]] = f32[16,16]{1,0} custom-call([[AA]], [[P1]], [[P2]], [[P3]], [[C]], /*index=5*/[[C]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
      )",
                            nullptr, &config);
}

TEST_P(ParameterizedFp8GemmRewriteTest,
       ScaledABUnscaledDMatrixBiasThenVectorBiasF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif
  const char* hlo_text = R"(
    HloModule test

    ENTRY test {
      x = f8e4m3fn[16,32] parameter(0)
      y = f8e4m3fn[32,16] parameter(1)
      x_f16 = f16[16,32] convert(x)
      y_f16 = f16[32,16] convert(y)
      b = f16[16] parameter(2)
      b_bcast = f16[16,16] broadcast(b), dimensions={1}
      b2 = f16[16,16] parameter(3)
      x_scale = f16[] parameter(4)
      y_scale = f16[] parameter(5)
      x_scale_bcast = f16[16,32] broadcast(x_scale), dimensions={}
      y_scale_bcast = f16[32,16] broadcast(y_scale), dimensions={}
      x_unscaled = f16[16,32] multiply(x_f16, x_scale_bcast)
      y_unscaled = f16[32,16] multiply(y_f16, y_scale_bcast)
      dot_a = f16[16,16] dot(x_unscaled, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}
      dot_a_bias1 = f16[16,16] add(dot_a, b2)
      ROOT dot_a_bias = f16[16,16] add(dot_a_bias1, b_bcast)
          }

)";
  CheckFp8IfSupported(hlo_text, ErrorSpec{2e-3, 0.});
  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(
; CHECK-LABEL:   ENTRY %test (x: f8e4m3fn[16,32], y: f8e4m3fn[32,16], b: f16[16], b2: f16[16,16], x_scale: f16[], y_scale: f16[]) -> f16[16,16] {
; CHECK-DAG:     [[P0:%[^ ]+]] = f8e4m3fn[16,32]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e4m3fn[32,16]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_TRANSPOSE:%[^ ]+]] = f8e4m3fn[16,32]{1,0} transpose([[P1]]), dimensions={1,0}
; CHECK-NEXT:    [[MB:%[^ ]+]] = f16[16,16]{1,0} parameter(3)
; CHECK-NEXT:    [[P2:%[^ ]+]] = f16[] parameter(4)
; CHECK-NEXT:    [[CV0:%[^ ]+]] = f32[] convert([[P2]])
; CHECK-NEXT:    [[P3:%[^ ]+]] = f16[] parameter(5)
; CHECK-NEXT:    [[CV1:%[^ ]+]] = f32[] convert([[P3]])
; CHECK:         [[C1:%[^ ]+]] = f32[] constant(1)
; CHECK:         [[GEMMOUT:%[^ ]+]] = f16[16,16]{1,0} custom-call([[P0]], [[P1_TRANSPOSE]], [[MB]], [[CV0]], [[CV1]], /*index=5*/[[C1]], [[C1]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":1
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
; CHECK:         [[VB:%[^ ]+]] = f16[16]{0} parameter(2)
; CHECK:         [[VBC:%[^ ]+]] = f16[16,16]{1,0} broadcast([[VB]]), dimensions={1}
; CHECK:         ROOT [[OUT:%[^ ]+]] = f16[16,16]{1,0} add([[GEMMOUT]], [[VBC]])
      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest, ScaledABScaledDWithDAmaxF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif
  const char* hlo_text = R"(
    HloModule test

    apply {
      a = f32[] parameter(0)
      b = f32[] parameter(1)
      ROOT c = f32[] maximum(a, b)
    }

    ENTRY test {
      x = f8e4m3fn[16,32] parameter(0)
      y = f8e4m3fn[32,16] parameter(1)
      x_f32 = f32[16,32] convert(x)
      y_f32 = f32[32,16] convert(y)
      x_scale = f32[] parameter(2)
      y_scale = f32[] parameter(3)
      z_scale = f32[] parameter(4)
      x_scale_bcast = f32[16,32] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[32,16] broadcast(y_scale), dimensions={}
      z_scale_bcast = f32[16,16] broadcast(z_scale), dimensions={}
      x_unscaled = f32[16,32] multiply(x_f32, x_scale_bcast)
      y_unscaled = f32[32,16] multiply(y_f32, y_scale_bcast)
      dot_a = f32[16,16] dot(x_unscaled, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}
      abs_dot_a = f32[16,16] abs(dot_a)
      c0 = f32[] constant(-inf)
      amax = f32[] reduce(abs_dot_a, c0), dimensions={0,1}, to_apply=apply
      dot_a_scaled = f32[16,16] divide(dot_a, z_scale_bcast)
      c1 = f32[] constant(-448.)
      c1_bcast = f32[16,16] broadcast(c1), dimensions={}
      c2 = f32[] constant(448.)
      c2_bcast = f32[16,16] broadcast(c2), dimensions={}
      dot_a_clamped = f32[16,16] clamp(c1_bcast, dot_a_scaled, c2_bcast)
      dot_a_f8 = f8e4m3fn[16,16] convert(dot_a_clamped)
      ROOT out = (f8e4m3fn[16,16], f32[]) tuple(dot_a_f8, amax)
          }

)";

  CheckFp8IfSupported(hlo_text);
  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(
; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[16,32], y: f8e4m3fn[32,16], x_scale: f32[], y_scale: f32[], z_scale: f32[]) -> (f8e4m3fn[16,16], f32[]) {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f8e4m3fn[16,32]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e4m3fn[32,16]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_TRANSPOSE:%[^ ]+]] = f8e4m3fn[16,32]{1,0} transpose([[P1]])
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[] parameter(2)
; CHECK-NEXT:    [[P3:%[^ ]+]] = f32[] parameter(3)
; CHECK-NEXT:    [[C1:%[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    [[C2:%[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    [[P4:%[^ ]+]] = f32[] parameter(4)
; CHECK-NEXT:    [[P4_INV:%[^ ]+]] = f32[] divide([[C2]], [[P4]])
; CHECK-NEXT:    [[OUT:%[^ ]+]] = (f8e4m3fn[16,16]{1,0}, f32[]) custom-call([[P0]], [[P1_TRANSPOSE]], [[P2]], [[P3]], [[C1]], /*index=5*/[[P4_INV]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest,
       ScaledABScaledDWithDAmaxF8WithF16Intermediates) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif
  // This is the same as ScaledABScaledDWithDAmaxF8, but uses F16 intermediate
  // values instead of F32 intermediate values.
  const char* hlo_text = R"(
    HloModule test

    apply {
      a = f16[] parameter(0)
      b = f16[] parameter(1)
      ROOT c = f16[] maximum(a, b)
    }

    ENTRY test {
      x = f8e4m3fn[16,32] parameter(0)
      y = f8e4m3fn[32,16] parameter(1)
      x_f16 = f16[16,32] convert(x)
      y_f16 = f16[32,16] convert(y)
      x_scale = f16[] parameter(2)
      y_scale = f16[] parameter(3)
      z_scale = f16[] parameter(4)
      x_scale_bcast = f16[16,32] broadcast(x_scale), dimensions={}
      y_scale_bcast = f16[32,16] broadcast(y_scale), dimensions={}
      z_scale_bcast = f16[16,16] broadcast(z_scale), dimensions={}
      x_unscaled = f16[16,32] multiply(x_f16, x_scale_bcast)
      y_unscaled = f16[32,16] multiply(y_f16, y_scale_bcast)
      dot_a = f16[16,16] dot(x_unscaled, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}
      abs_dot_a = f16[16,16] abs(dot_a)
      c0 = f16[] constant(-inf)
      amax = f16[] reduce(abs_dot_a, c0), dimensions={0,1}, to_apply=apply
      dot_a_scaled = f16[16,16] divide(dot_a, z_scale_bcast)
      c1 = f16[] constant(-448.)
      c1_bcast = f16[16,16] broadcast(c1), dimensions={}
      c2 = f16[] constant(448.)
      c2_bcast = f16[16,16] broadcast(c2), dimensions={}
      dot_a_clamped = f16[16,16] clamp(c1_bcast, dot_a_scaled, c2_bcast)
      dot_a_f8 = f8e4m3fn[16,16] convert(dot_a_clamped)
      ROOT out = (f8e4m3fn[16,16], f16[]) tuple(dot_a_f8, amax)
          }

)";

  CheckFp8IfSupported(hlo_text);
  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(
; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[16,32], y: f8e4m3fn[32,16], x_scale: f16[], y_scale: f16[], z_scale: f16[]) -> (f8e4m3fn[16,16], f16[]) {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f8e4m3fn[16,32]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e4m3fn[32,16]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_TRANSPOSE:%[^ ]+]] = f8e4m3fn[16,32]{1,0} transpose([[P1]])
; CHECK-NEXT:    [[P2:%[^ ]+]] = f16[] parameter(2)
; CHECK-NEXT:    [[P2_CONVERT:%[^ ]+]] = f32[] convert([[P2]])
; CHECK-NEXT:    [[P3:%[^ ]+]] = f16[] parameter(3)
; CHECK-NEXT:    [[P3_CONVERT:%[^ ]+]] = f32[] convert([[P3]])
; CHECK-NEXT:    [[C1:%[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    [[C2:%[^ ]+]] = f16[] constant(1)
; CHECK-NEXT:    [[P4:%[^ ]+]] = f16[] parameter(4)
; CHECK-NEXT:    [[P4_INV:%[^ ]+]] = f16[] divide([[C2]], [[P4]])
; CHECK-NEXT:    [[P4_INV_CONVERT:%[^ ]+]] = f32[] convert([[P4_INV]])
; CHECK-NEXT:    [[OUT:%[^ ]+]] = (f8e4m3fn[16,16]{1,0}, f32[]) custom-call([[P0]], [[P1_TRANSPOSE]], [[P2_CONVERT]], [[P3_CONVERT]], [[C1]], /*index=5*/[[P4_INV_CONVERT]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest,
       ScaledABScaledDReluActivationWithDAmaxF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif
  const char* hlo_text = R"(
    HloModule test

    apply {
      a = f32[] parameter(0)
      b = f32[] parameter(1)
      ROOT c = f32[] maximum(a, b)
    }

    ENTRY test {
      x = f8e4m3fn[16,32] parameter(0)
      y = f8e4m3fn[32,16] parameter(1)
      x_f32 = f32[16,32] convert(x)
      y_f32 = f32[32,16] convert(y)
      x_scale = f32[] parameter(2)
      y_scale = f32[] parameter(3)
      z_scale = f32[] parameter(4)
      x_scale_bcast = f32[16,32] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[32,16] broadcast(y_scale), dimensions={}
      z_scale_bcast = f32[16,16] broadcast(z_scale), dimensions={}
      x_unscaled = f32[16,32] multiply(x_f32, x_scale_bcast)
      y_unscaled = f32[32,16] multiply(y_f32, y_scale_bcast)
      dot_a = f32[16,16] dot(x_unscaled, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}
      czero = f32[] constant(0)
      czero_bcast = f32[16,16] broadcast(czero), dimensions={}
      dot_a_relu = f32[16,16] maximum(dot_a, czero_bcast)
      c0 = f32[] constant(-inf)
      amax = f32[] reduce(dot_a_relu, c0), dimensions={0,1}, to_apply=apply
      dot_a_scaled = f32[16,16] divide(dot_a_relu, z_scale_bcast)
      c1 = f32[] constant(-448.)
      c1_bcast = f32[16,16] broadcast(c1), dimensions={}
      c2 = f32[] constant(448.)
      c2_bcast = f32[16,16] broadcast(c2), dimensions={}
      dot_a_clamped = f32[16,16] clamp(c1_bcast, dot_a_scaled, c2_bcast)
      dot_a_f8 = f8e4m3fn[16,16] convert(dot_a_clamped)
      ROOT out = (f8e4m3fn[16,16], f32[]) tuple(dot_a_f8, amax)
          }

)";

  CheckFp8IfSupported(hlo_text);
  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(
; CHECK-LABEL: ENTRY %test (x: f8e4m3fn[16,32], y: f8e4m3fn[32,16], x_scale: f32[], y_scale: f32[], z_scale: f32[]) -> (f8e4m3fn[16,16], f32[]) {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f8e4m3fn[16,32]{1,0} parameter(0)
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e4m3fn[32,16]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_TRANSPOSE:%[^ ]+]] = f8e4m3fn[16,32]{1,0} transpose([[P1]])
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[] parameter(2)
; CHECK-NEXT:    [[P3:%[^ ]+]] = f32[] parameter(3)
; CHECK-NEXT:    [[C1:%[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    [[C2:%[^ ]+]] = f32[] constant(1)
; CHECK-NEXT:    [[P4:%[^ ]+]] = f32[] parameter(4)
; CHECK-NEXT:    [[P4_INV:%[^ ]+]] = f32[] divide([[C2]], [[P4]])
; CHECK-NEXT:    [[OUT:%[^ ]+]] = (f8e4m3fn[16,16]{1,0}, f32[]) custom-call([[P0]], [[P1_TRANSPOSE]], [[P2]], [[P3]], [[C1]], /*index=5*/[[P4_INV]]),
; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["1"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"RELU"
; CHECK:           }
      )");
}

TEST_P(ParameterizedFp8GemmRewriteTest, UnscaledABUnscaledDPrecisionF8) {
#if CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif  // CUDA_VERSION < 12000
  const char* hlo_template = R"(
    HloModule test

    ENTRY test {
      x = f8e4m3fn[1600,3200] parameter(0)
      y = f8e4m3fn[3200,1600] parameter(1)
      x_f32 = f32[1600,3200] convert(x)
      y_f32 = f32[3200,1600] convert(y)
      x_scale = f32[] parameter(2)
      y_scale = f32[] parameter(3)
      x_scale_bcast = f32[1600,3200] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[3200,1600] broadcast(y_scale), dimensions={}
      x_unscaled = f32[1600,3200] multiply(x_f32, x_scale_bcast)
      y_unscaled = f32[3200,1600] multiply(y_f32, y_scale_bcast)
      ROOT out = f32[1600,1600] dot(x_unscaled, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}, operand_precision={<<precision>>,<<precision>>}
          }
)";

  absl::flat_hash_map<absl::string_view, absl::string_view> replacements;
  replacements["<<precision>>"] = "default";
  const auto hlo_text_default = absl::StrReplaceAll(hlo_template, replacements);
  EXPECT_TRUE(RunAndCompare(hlo_text_default, ErrorSpec{1e-3, 1e-3}));

  replacements["<<precision>>"] = "highest";
  const auto hlo_text_highest = absl::StrReplaceAll(hlo_template, replacements);
  EXPECT_TRUE(RunAndCompare(hlo_text_highest, ErrorSpec{1e-4, 1e-4}));
}

TEST_P(ParameterizedFp8GemmRewriteTest, ScaledABUnscaledDF8Parameterized) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif
  std::array<std::array<absl::string_view, 7>, 32> combinations;
  int i = 0;

  for (bool d_is_col : {false, true}) {
    for (bool a_is_col : {false, true}) {
      for (bool b_is_col : {false, true}) {
        for (int lhs_contracting_dim : {0, 1}) {
          for (int rhs_contracting_dim : {0, 1}) {
            const absl::string_view lcd =
                lhs_contracting_dim == 1 ? "{1}" : "{0}";
            const absl::string_view rcd =
                rhs_contracting_dim == 1 ? "{1}" : "{0}";
            const absl::string_view a_shape =
                lhs_contracting_dim == 1 ? "[64,32]" : "[32,64]";
            const absl::string_view b_shape =
                rhs_contracting_dim == 0 ? "[32,16]" : "[16,32]";
            const absl::string_view a_layout = a_is_col ? "{0,1}" : "{1,0}";
            const absl::string_view b_layout = b_is_col ? "{0,1}" : "{1,0}";
            const absl::string_view output_layout =
                d_is_col ? "{0,1}" : "{1,0}";
            combinations[i++] = std::array{
                lcd, rcd, a_shape, b_shape, a_layout, b_layout, output_layout};
          }
        }
      }
    }
  }

  const char* hlo_template = R"(
      HloModule test
    ENTRY test {
      x = f8e4m3fn<<Ashape>><<Alayout>> parameter(0)
      x_f32 = f32<<Ashape>><<Alayout>> convert(x)
      x_scale = f32[] parameter(2)
      x_scale_bcast = f32<<Ashape>> broadcast(x_scale), dimensions={}
      x_unscaled = f32<<Ashape>> multiply(x_f32, x_scale_bcast)
      y = f8e4m3fn<<Bshape>><<Blayout>> parameter(1)
      y_f32 = f32<<Bshape>><<Blayout>> convert(y)
      y_scale = f32[] parameter(3)
      y_scale_bcast = f32<<Bshape>> broadcast(y_scale), dimensions={}
      y_unscaled = f32<<Bshape>> multiply(y_f32, y_scale_bcast)
      ROOT out = f32[64,16]<<Olayout>> dot(x_unscaled, y_unscaled), lhs_contracting_dims=<<Lcd>>, rhs_contracting_dims=<<Rcd>>
    }
      )";
  for (const auto& combination : combinations) {
    absl::flat_hash_map<absl::string_view, absl::string_view> replacements;
    replacements["<<Lcd>>"] = std::get<0>(combination);
    replacements["<<Rcd>>"] = std::get<1>(combination);
    replacements["<<Ashape>>"] = std::get<2>(combination);
    replacements["<<Bshape>>"] = std::get<3>(combination);
    replacements["<<Alayout>>"] = std::get<4>(combination);
    replacements["<<Blayout>>"] = std::get<5>(combination);
    replacements["<<Olayout>>"] = std::get<6>(combination);
    const auto hlo_text = absl::StrReplaceAll(hlo_template, replacements);
    CheckFp8IfSupported(hlo_text);

    RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                              R"(
    ; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
          )");
  }
}

TEST_P(ParameterizedFp8GemmRewriteTest,
       ScaledABUnscaledDF8ParameterizedBatched) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif
  // TODO(wenscarl): For batched matmul, not all combinations of A, B and
  // output layouts get pattern matched successfully to FP8 custom call. Only
  // a handful of cases are tested here.
  std::array<std::array<std::string, 7>, 32> combinations;
  std::string lcd, rcd, a_shape, b_shape, a_layout, b_layout, o_layout;
  int i = 0;
  for (bool o_is_col : {false, true}) {
    for (int lhs_contracting_dim : {2, 1}) {
      for (int rhs_contracting_dim : {2, 1}) {
        lcd = lhs_contracting_dim == 2 ? "{2}" : "{1}";
        rcd = rhs_contracting_dim == 2 ? "{2}" : "{1}";
        a_shape = lhs_contracting_dim == 2 ? "[2,64,32]" : "[2,32,64]";
        b_shape = rhs_contracting_dim == 1 ? "[2,32,16]" : "[2,16,32]";
        o_layout = o_is_col ? "{2, 0, 1}" : "{2, 1, 0}";
        for (std::string a_layout : {"{2,1,0}", "{1,2,0}"}) {
          for (std::string b_layout : {"{2,1,0}", "{1,2,0}"}) {
            combinations[i++] = std::array{lcd,      rcd,      a_shape, b_shape,
                                           a_layout, b_layout, o_layout};
          }
        }
      }
    }
  }

  const char* hlo_template = R"(
      HloModule m
ENTRY f {
  x_q = f8e4m3fn<<Ashape>><<Alayout>> parameter(0)
  x_scale = f32[] parameter(2)
  x_scale_broadcast = f32<<Ashape>><<Alayout>> broadcast(x_scale), dimensions={}
  x_q_convert = f32<<Ashape>><<Alayout>> convert(x_q)
  x_qdq = f32<<Ashape>><<Alayout>> multiply(x_q_convert, x_scale_broadcast)

  y_q = f8e4m3fn<<Bshape>><<Blayout>> parameter(1)
  y_scale = f32[] parameter(3)
  y_scale_broadcast = f32<<Bshape>><<Blayout>> broadcast(y_scale), dimensions={}
  y_q_convert = f32<<Bshape>><<Blayout>> convert(y_q)
  y_qdq = f32<<Bshape>><<Blayout>> multiply(y_q_convert, y_scale_broadcast)

  ROOT out = f32[2,64,16]<<Olayout>> dot(x_qdq, y_qdq), lhs_batch_dims={0}, lhs_contracting_dims=<<Lcd>>, rhs_batch_dims={0}, rhs_contracting_dims=<<Rcd>>
}
     )";
  for (const auto& combination : combinations) {
    absl::flat_hash_map<std::string, std::string> replacements;
    replacements["<<Lcd>>"] = std::get<0>(combination);
    replacements["<<Rcd>>"] = std::get<1>(combination);
    replacements["<<Ashape>>"] = std::get<2>(combination);
    replacements["<<Bshape>>"] = std::get<3>(combination);
    replacements["<<Alayout>>"] = std::get<4>(combination);
    replacements["<<Blayout>>"] = std::get<5>(combination);
    replacements["<<Olayout>>"] = std::get<6>(combination);

    const auto hlo_text = absl::StrReplaceAll(hlo_template, replacements);
    CheckFp8IfSupported(hlo_text);

    RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                              R"(
    ; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
          )");
  }
}

TEST_P(ParameterizedFp8GemmRewriteTest, ScaledABUnscaledDF8TF32E5M2) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif
  const char* hlo_text = R"(
    HloModule test

    ENTRY test {
      x = f8e4m3fn[16,32] parameter(0)
      y = f8e5m2[32,16] parameter(1)
      x_f32 = f32[16,32] convert(x)
      y_f32 = f32[32,16] convert(y)
      x_scale = f32[] parameter(2)
      y_scale = f32[] parameter(3)
      x_scale_bcast = f32[16,32] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[32,16] broadcast(y_scale), dimensions={}
      x_unscaled = f32[16,32] multiply(x_f32, x_scale_bcast)
      y_unscaled = f32[32,16] multiply(y_f32, y_scale_bcast)
      ROOT out = f32[16,16] dot(x_unscaled, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}
          }

)";

  CheckFp8IfSupported(hlo_text);
  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            R"(
    ; CHECK:           custom_call_target="__cublas$lt$matmul$f8",
          )");
}

TEST_P(ParameterizedFp8GemmRewriteTest, FnuzTypeF8) {
#if GOOGLE_CUDA && CUDA_VERSION < 12000
  GTEST_SKIP() << "F8 gemm rewrite is only supported in CUDA 12 and above.";
#endif
  // Test that FNUZ FP8 gemms are not rewritten, as cuBLAS does not support them
  const char* hlo_text = R"(
    HloModule test

    ENTRY test {
      x = f8e4m3fnuz[16,32] parameter(0)
      y = f8e4m3fnuz[32,16] parameter(1)
      x_f32 = f32[16,32] convert(x)
      y_f32 = f32[32,16] convert(y)
      x_scale = f32[] parameter(2)
      y_scale = f32[] parameter(3)
      x_scale_bcast = f32[16,32] broadcast(x_scale), dimensions={}
      y_scale_bcast = f32[32,16] broadcast(y_scale), dimensions={}
      x_unscaled = f32[16,32] multiply(x_f32, x_scale_bcast)
      y_unscaled = f32[32,16] multiply(y_f32, y_scale_bcast)
      ROOT out = f32[16,16] dot(x_unscaled, y_unscaled), lhs_contracting_dims={1}, rhs_contracting_dims={0}
          }
)";
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-2, 1e-2}));
  RunAndFilecheckHloRewrite(hlo_text, GemmRewriter(CudaHopperOrRocm()),
                            absl::StrReplaceAll(R"(
; CHECK-LABEL: ENTRY %test (x: f8e4m3fnuz[16,32], y: f8e4m3fnuz[32,16], x_scale: f32[], y_scale: f32[]) -> f32[16,16] {
; CHECK-NEXT:    [[P0:%[^ ]+]] = f8e4m3fnuz[16,32]{1,0} parameter(0)
; CHECK-NEXT:    [[P0_CV:%[^ ]+]] = f32[16,32]{1,0} convert([[P0]])
; CHECK-NEXT:    [[P2:%[^ ]+]] = f32[] parameter(2)
; CHECK-NEXT:    [[P2_B:%[^ ]+]] = f32[16,32]{1,0} broadcast([[P2]]), dimensions={}
; CHECK-NEXT:    [[P0_UNSCALED:%[^ ]+]] = f32[16,32]{1,0} multiply([[P0_CV]], [[P2_B]])
; CHECK-NEXT:    [[P1:%[^ ]+]] = f8e4m3fnuz[32,16]{1,0} parameter(1)
; CHECK-NEXT:    [[P1_CV:%[^ ]+]] = f32[32,16]{1,0} convert([[P1]])
; CHECK-NEXT:    [[P3:%[^ ]+]] = f32[] parameter(3)
; CHECK-NEXT:    [[P3_B:%[^ ]+]] = f32[32,16]{1,0} broadcast([[P3]]), dimensions={}
; CHECK-NEXT:    [[P1_UNSCALED:%[^ ]+]] = f32[32,16]{1,0} multiply([[P1_CV]], [[P3_B]])
; CHECK-NEXT:    [[GEMM:%[^ ]+]] = {{.*}} custom-call([[P0_UNSCALED]], [[P1_UNSCALED]]),
; CHECK:           custom_call_target="<<CUBLAS_CUSTOM_CALL_TARGET_PLACEHOLDER>>",
; CHECK:           backend_config={
; CHECK-DAG:         "alpha_real":1
; CHECK-DAG:         "alpha_imag":0
; CHECK-DAG:         "beta":0
; CHECK-DAG:         "dot_dimension_numbers":{
; CHECK-DAG:           "lhs_contracting_dimensions":["1"]
; CHECK-DAG:           "rhs_contracting_dimensions":["0"]
; CHECK-DAG:           "lhs_batch_dimensions":[]
; CHECK-DAG:           "rhs_batch_dimensions":[]
; CHECK-DAG:         }
; CHECK-DAG:         "precision_config":{
; CHECK-DAG:           "operand_precision":["DEFAULT","DEFAULT"]
; CHECK-DAG:         }
; CHECK-DAG:         "epilogue":"DEFAULT"
; CHECK:           }
      )",
                                                replacements_));
}

INSTANTIATE_TEST_SUITE_P(Fp8CublasTestsBothLegacyAndLt,
                         ParameterizedFp8GemmRewriteTest, ::testing::Bool());
#endif

TEST_F(GemmRewriteTest, NoFuseBiasBroadcast) {
  const char* hlo = R"(

HloModule module

ENTRY main.10 {
  Arg_0.1 = f16[384,128]{1,0} parameter(0)
  Arg_1.2 = f16[128,256]{1,0} parameter(1)
  dot.4 = f16[384,256]{1,0} dot(Arg_0.1, Arg_1.2), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  Arg_2.3 = f16[256]{0} parameter(2)
  reshape.5 = f16[1,256]{1,0} reshape(Arg_2.3)
  broadcast.6 = f16[1,256]{1,0} broadcast(reshape.5), dimensions={0,1}
  reshape.7 = f16[256]{0} reshape(broadcast.6)
  broadcast.8 = f16[384,256]{1,0} broadcast(reshape.7), dimensions={1}
  ROOT add.9 = f16[384,256]{1,0} add(dot.4, broadcast.8)
})";

  MatchOptimizedHlo(hlo, R"(
// CHECK: "beta":0
  )");
}

class GemmRewriteAllocationTest : public GpuCodegenTest {
 public:
  void CheckNumberOfAllocations(const std::string& hlo,
                                int expected_number_of_allocations) {
    TF_ASSERT_OK_AND_ASSIGN(std::unique_ptr<HloModule> optimized_module,
                            GetOptimizedModule(hlo));
    TF_ASSERT_OK_AND_ASSIGN(
        std::unique_ptr<Executable> executable,
        backend().compiler()->RunBackend(
            std::move(optimized_module), backend().default_stream_executor(),
            backend().default_stream_executor()->GetAllocator()));
    GpuExecutable* gpu_executable =
        static_cast<GpuExecutable*>(executable.get());
    absl::Span<const BufferAllocation> allocations =
        gpu_executable->GetAllocations();
    ASSERT_EQ(allocations.size(), expected_number_of_allocations);
  }
};

TEST_F(GemmRewriteAllocationTest, SharedBufferAssignment) {
  const char* hlo_text = R"(
HloModule SharedBufferAssignment

ENTRY AddDotsFunc {
  x = f32[2,2] parameter(0)
  y = f32[2,2] parameter(1)
  bias = f32[2,2] add(x, y)
  dot = f32[2,2] dot(x, y), lhs_contracting_dims={1}, rhs_contracting_dims={0}
  ROOT out = f32[2,2] add(dot, bias)
}

)";

  // Bias should be fused into the multiplication.
  CheckNumberOfAllocations(hlo_text, 4);
  EXPECT_TRUE(RunAndCompare(hlo_text, ErrorSpec{1e-5, 1e-5}));
}

}  // namespace
}  // namespace gpu
}  // namespace xla
