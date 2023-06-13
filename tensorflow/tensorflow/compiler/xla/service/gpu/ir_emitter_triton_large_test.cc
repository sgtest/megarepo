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

#include <string>

#include "tensorflow/compiler/xla/error_spec.h"
#include "tensorflow/compiler/xla/service/gpu/tests/gpu_codegen_test.h"

namespace xla {
namespace gpu {
namespace {

using TritonGemmTest = GpuCodegenTest;

TEST_F(TritonGemmTest, IndexUsing64Bits) {
  const char* kHloTextRef = R"(
HloModule r

ENTRY e {
  arg0 = f16[65536,32800] parameter(0)
  arg1 = f16[32800,32] parameter(1)
  ROOT custom-call = f16[65536,32] custom-call(arg0, arg1),
    custom_call_target="__cublas$gemm",
    backend_config="{\"alpha_real\":1,\"beta\":0,\"dot_dimension_numbers\":{\"lhs_contracting_dimensions\":[\"1\"],\"rhs_contracting_dimensions\":[\"0\"],\"lhs_batch_dimensions\":[],\"rhs_batch_dimensions\":[]},\"alpha_imag\":0,\"precision_config\":{\"operand_precision\":[\"DEFAULT\",\"DEFAULT\"]},\"epilogue\":\"DEFAULT\"}"
}
)";

  const char* kHloTextTest = R"(
HloModule t

triton_dot {
  p0 = f16[65536,32800] parameter(0)
  p1 = f16[32800,32] parameter(1)
  ROOT dot = f16[65536,32] dot(p0, p1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
}

ENTRY e {
  p0 = f16[65536,32800] parameter(0)
  p1 = f16[32800,32] parameter(1)
  ROOT _ = f16[65536,32] fusion(p0, p1), kind=kCustom, calls=triton_dot,
    backend_config="{kind: \"__triton_gemm\", triton_gemm_config: {\"block_m\":\"32\",\"block_n\":\"32\",\"block_k\":\"32\",\"split_k\":\"1\",\"num_stages\":\"1\",\"num_warps\":\"1\"}}"
}
)";

  EXPECT_TRUE(RunAndCompareTwoModules(kHloTextRef, kHloTextTest,
                                      ErrorSpec{1e-3, 1e-3},
                                      /*run_hlo_passes=*/false));
}

TEST_F(TritonGemmTest, LargeNonContractingProductWorks) {
  const std::string kHloText = R"(
HloModule m

ENTRY e {
  p0 = s8[1310720,2] parameter(0)
  c0 = f16[1310720,2] convert(p0)
  p1 = f16[2,15] parameter(1)
  ROOT dot.12 = f16[1310720,15] dot(c0, p1),
    lhs_contracting_dims={1}, rhs_contracting_dims={0}
})";

  // Make sure the output size is sufficient to use the X grid dimension
  // for the non-contracting dimensions of the output. 16x16 is the smallest
  // MxN block used currently.
  CHECK_GT(1310720 * 15 / (16 * 16), 65535);

  MatchOptimizedHlo(kHloText, R"(
; CHECK: triton
)");

  EXPECT_TRUE(RunAndCompare(kHloText, ErrorSpec{/*aabs=*/1e-3, /*arel=*/1e-3}));
}

TEST_F(TritonGemmTest, LargeBatchWorks) {
  const std::string kHloText = R"(
HloModule m

ENTRY e {
  Arg_0.8 = pred[102400,10,10] parameter(0)
  convert.11 = f32[102400,10,10] convert(Arg_0.8)
  Arg_1.9 = f32[102400,10,100] parameter(1)
  ROOT dot.12 = f32[102400,10,100] dot(convert.11, Arg_1.9),
    lhs_batch_dims={0}, lhs_contracting_dims={2},
    rhs_batch_dims={0}, rhs_contracting_dims={1}
})";

  MatchOptimizedHlo(kHloText, R"(
; CHECK: triton
)");

  // Batch size of 102400 is over 65535 so the X grid dimension has to be used
  // for it.

  EXPECT_TRUE(RunAndCompare(kHloText, ErrorSpec{/*aabs=*/1e-3, /*arel=*/1e-3}));
}

}  // namespace
}  // namespace gpu
}  // namespace xla
