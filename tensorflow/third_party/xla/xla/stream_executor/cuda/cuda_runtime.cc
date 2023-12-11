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

#include "absl/base/optimization.h"
#include "absl/log/log.h"
#include "absl/status/status.h"
#include "absl/strings/str_cat.h"
#include "third_party/gpus/cuda/include/cuda.h"
#include "third_party/gpus/cuda/include/cuda_runtime_api.h"
#include "third_party/gpus/cuda/include/driver_types.h"
#include "xla/stream_executor/gpu/gpu_runtime.h"
#include "xla/stream_executor/gpu/gpu_types.h"
#include "tsl/platform/statusor.h"

namespace stream_executor::gpu {

static const char* ToString(cudaError_t error) {
  return cudaGetErrorString(error);
}

#define RETURN_IF_CUDA_RES_ERROR(expr, ...)                 \
  do {                                                      \
    cudaError_t _res = (expr);                              \
    if (ABSL_PREDICT_FALSE(_res != cudaSuccess)) {          \
      return absl::InternalError(                           \
          absl::StrCat(__VA_ARGS__, ": ", ToString(_res))); \
    }                                                       \
  } while (0)

tsl::StatusOr<GpuFunctionHandle> GpuRuntime::GetFuncBySymbol(void* symbol) {
  VLOG(2) << "Get CUDA function from a symbol: " << symbol;
  cudaFunction_t func;
  RETURN_IF_CUDA_RES_ERROR(cudaGetFuncBySymbol(&func, symbol),
                           "Failed call to cudaGetFuncBySymbol");
  return reinterpret_cast<CUfunction>(func);
}

}  // namespace stream_executor::gpu
