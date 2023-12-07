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

#include <algorithm>
#include <climits>
#include <memory>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include "rocm/rocm_config.h"
#include "xla/primitive_util.h"
#include "xla/status_macros.h"
#include "xla/util.h"

#if TF_HIPBLASLT
#include "xla/stream_executor/gpu/gpu_activation.h"
#include "xla/stream_executor/gpu/gpu_helpers.h"
#include "xla/stream_executor/gpu/gpu_stream.h"
#include "xla/stream_executor/gpu/gpu_timer.h"
#include "xla/stream_executor/rocm/hip_blas_lt.h"
#include "xla/stream_executor/rocm/rocm_blas.h"
#include "xla/stream_executor/scratch_allocator.h"
#include "xla/stream_executor/stream.h"

#define SET_ATTR(setter, handle, attr, value) \
  ToStatus(setter(handle, attr, &value, sizeof(decltype(value))), #setter)

// hipblasLtMatmulDescGetAttribute does not allow nullptr for the last
// argument (size_t* sizeWritten)
#define GET_ATTR(getter, handle, attr, ValueT)                          \
  [&]() -> tsl::StatusOr<ValueT> {                                      \
    ValueT value;                                                       \
    size_t size;                                                        \
    TF_RETURN_IF_ERROR(ToStatus(                                        \
        getter(handle, attr, &value, sizeof(ValueT), &size), #getter)); \
    return std::move(value);                                            \
  }()

namespace stream_executor {

namespace rocm {

using ::xla::complex128;
using ::xla::complex64;

namespace {

template <typename T>
tsl::Status SetAttr(hipblasLtMatrixLayout_t handle,
                    hipblasLtMatrixLayoutAttribute_t attr, T value) {
  return SET_ATTR(wrap::hipblasLtMatrixLayoutSetAttribute, handle, attr, value);
}

template <typename T>
tsl::StatusOr<T> GetAttr(hipblasLtMatrixLayout_t handle,
                         hipblasLtMatrixLayoutAttribute_t attr) {
  return GET_ATTR(wrap::hipblasLtMatrixLayoutGetAttribute, handle, attr, T);
}

template <typename T>
tsl::Status SetAttr(hipblasLtMatmulDesc_t handle,
                    hipblasLtMatmulDescAttributes_t attr, T value) {
  return SET_ATTR(wrap::hipblasLtMatmulDescSetAttribute, handle, attr, value);
}

template <typename T>
tsl::StatusOr<T> GetAttr(hipblasLtMatmulDesc_t handle,
                         hipblasLtMatmulDescAttributes_t attr) {
  return GET_ATTR(wrap::hipblasLtMatmulDescGetAttribute, handle, attr, T);
}

template <typename T>
tsl::Status SetAttr(hipblasLtMatmulPreference_t handle,
                    hipblasLtMatmulPreferenceAttributes_t attr, T value) {
  return SET_ATTR(wrap::hipblasLtMatmulPreferenceSetAttribute, handle, attr,
                  value);
}

static hipblasPointerMode_t AsHipblasLtPointerMode(
    gpu::BlasLt::PointerMode pointer_mode) {
  switch (pointer_mode) {
    case gpu::BlasLt::PointerMode::kHost:
      return HIPBLAS_POINTER_MODE_HOST;
    case gpu::BlasLt::PointerMode::kDevice:
      return HIPBLAS_POINTER_MODE_DEVICE;
  }
}

static tsl::StatusOr<hipblasLtEpilogue_t> AsHipblasLtEpilogue(
    gpu::BlasLt::Epilogue epilogue) {
  switch (epilogue) {
    case gpu::BlasLt::Epilogue::kDefault:
      return HIPBLASLT_EPILOGUE_DEFAULT;
    case gpu::BlasLt::Epilogue::kReLU:
      return HIPBLASLT_EPILOGUE_RELU;
    case gpu::BlasLt::Epilogue::kBias:
      return HIPBLASLT_EPILOGUE_BIAS;
    case gpu::BlasLt::Epilogue::kBiasThenReLU:
      return HIPBLASLT_EPILOGUE_RELU_BIAS;
    case gpu::BlasLt::Epilogue::kGELU:
      return HIPBLASLT_EPILOGUE_GELU;
    default:
      return tsl::errors::Internal("Unsupported epilogue: " +
                                   std::to_string((int)epilogue));
  }
}

}  // namespace

tsl::Status BlasLt::Init() {
  hipblasLtHandle_t blas_lt;
  SE_HIPBLAS_RETURN_IF_ERROR(wrap::hipblasLtCreate(&blas_lt));
  absl::MutexLock lock(&mu_);
  blas_lt_.reset(blas_lt);
  return tsl::OkStatus();
}

/*static*/ tsl::StatusOr<BlasLt::MatrixLayout> BlasLt::MatrixLayout::Create(
    const gpu::MatrixLayout& m) {
  TF_ASSIGN_OR_RETURN(auto type, gpu::AsBlasDataType(m.dtype));

  auto leading_dim_stride = m.leading_dim_stride;
  if (!leading_dim_stride) {
    leading_dim_stride = (m.order == gpu::MatrixLayout::Order::kRowMajor)
                             ? m.num_cols
                             : m.num_rows;
  }
  auto hipblas_data_type_ = AsHipblasDataType(type);
  hipblasLtMatrixLayout_t hip_layout;
  SE_HIPBLAS_RETURN_IF_ERROR(wrap::hipblasLtMatrixLayoutCreate(
      &hip_layout, hipblas_data_type_, m.num_rows, m.num_cols,
      *leading_dim_stride));
  // Wrap hipblas handle immediately, so it is cleaned up if an error occurs.
  BlasLt::MatrixLayout layout(hip_layout, hipblas_data_type_);
  if (m.order != gpu::MatrixLayout::Order::kColumnMajor)
    return tsl::errors::Internal(
        "HipblasLT does not support row-major matrices");
  TF_RETURN_IF_ERROR(SetAttr(hip_layout, HIPBLASLT_MATRIX_LAYOUT_BATCH_COUNT,
                             static_cast<int32_t>(m.batch_size)));

  auto batch_stride = m.batch_stride;
  if (!batch_stride) {
    batch_stride = (m.batch_size > 1) ? m.num_rows * m.num_cols : 0;
  }
  VLOG(2) << "BlasLt::MatrixLayout::Create type: " << (int)type
          << " rows: " << m.num_rows << " cols: " << m.num_cols
          << " batch_size: " << m.batch_size
          << " leading_dim_stride: " << *leading_dim_stride
          << " batch_stride: " << *batch_stride;

  TF_RETURN_IF_ERROR(SetAttr(
      hip_layout, HIPBLASLT_MATRIX_LAYOUT_STRIDED_BATCH_OFFSET, *batch_stride));
  return std::move(layout);
}

/*static*/ tsl::StatusOr<BlasLt::MatmulDesc> BlasLt::MatmulDesc::Create(
    blas::ComputationType compute_type, blas::DataType scale_type,
    blas::Transpose trans_a, blas::Transpose trans_b, Epilogue epilogue,
    PointerMode pointer_mode) {
  hipblasLtMatmulDesc_t hip_desc;
  VLOG(2) << "BlasLt::MatmulDesc::Create compute_type: " << int(compute_type)
          << " scale_type: " << int(scale_type)
          << " epilogue: " << int(epilogue) << " trans_a: " << int(trans_a)
          << " trans_b: " << int(trans_b) << " pointer_mode "
          << int(pointer_mode);
  auto hip_scale_type = AsHipblasDataType(scale_type);
  auto hip_compute_type = AsHipblasComputeType(compute_type);
  SE_HIPBLAS_RETURN_IF_ERROR(wrap::hipblasLtMatmulDescCreate(
      &hip_desc, hip_compute_type, hip_scale_type));
  // Wrap hipblas handle immediately, so it is cleaned up if an error occurs.
  BlasLt::MatmulDesc desc(hip_desc, hip_compute_type, hip_scale_type);
  if (pointer_mode != PointerMode::kHost) {
    return tsl::errors::Internal("hipblaslt does not support device pointers");
  }

  TF_RETURN_IF_ERROR(SetAttr(hip_desc, HIPBLASLT_MATMUL_DESC_TRANSA,
                             AsHipblasOperation(trans_a)));
  TF_RETURN_IF_ERROR(SetAttr(hip_desc, HIPBLASLT_MATMUL_DESC_TRANSB,
                             AsHipblasOperation(trans_b)));
  TF_ASSIGN_OR_RETURN(hipblasLtEpilogue_t epi, AsHipblasLtEpilogue(epilogue));
  TF_RETURN_IF_ERROR(SetAttr(hip_desc, HIPBLASLT_MATMUL_DESC_EPILOGUE, epi));
  return std::move(desc);
}

auto BlasLt::MatmulPlan::GetAlgorithms(size_t max_algorithm_count,
                                       size_t max_workspace_size) const
    -> tsl::StatusOr<std::vector<MatmulAlgorithm>> {
  max_algorithm_count = std::min(max_algorithm_count, size_t{INT_MAX});
  std::vector<hipblasLtMatmulHeuristicResult_t> results(max_algorithm_count);

  {
    absl::MutexLock lock(&blas_lt_ref_.mu_);
    TF_RET_CHECK(blas_lt_ref_.blas_lt_ != nullptr);

    hipblasLtMatmulPreference_t hip_preference;
    SE_HIPBLAS_RETURN_IF_ERROR(
        wrap::hipblasLtMatmulPreferenceCreate(&hip_preference));

    // Wrap hipblas handle immediately, so it is cleaned up if an error occurs.
    Owned<hipblasLtMatmulPreference_t> preference(
        hip_preference, wrap::hipblasLtMatmulPreferenceDestroy);

    TF_RETURN_IF_ERROR(SetAttr<uint64_t>(
        hip_preference, HIPBLASLT_MATMUL_PREF_MAX_WORKSPACE_BYTES,
        max_workspace_size));

    gpu::ScopedActivateExecutorContext sac{blas_lt_ref_.parent_};

    // Right now, hipBlasLt would require setting the bias pointer (even a dummy
    // one) before finding the algorithms for
    // HIPBLASLT_MATMUL_DESC_BIAS_POINTER. Can remove this later once this
    // restriction is gone.
    static int dummy_pointer = 0;
    TF_ASSIGN_OR_RETURN(auto epilogue,
                        GetAttr<hipblasLtEpilogue_t>(
                            op_desc_.get(), HIPBLASLT_MATMUL_DESC_EPILOGUE));
    if (epilogue == HIPBLASLT_EPILOGUE_BIAS) {
      TF_RETURN_IF_ERROR(SetAttr(
          op_desc_.get(), HIPBLASLT_MATMUL_DESC_BIAS_POINTER, &dummy_pointer));
    }

    int found_algorithm_count = 0;
    auto error = wrap::hipblasLtMatmulAlgoGetHeuristic(
        blas_lt_ref_.blas_lt_.get(), op_desc_.get(), a_desc_.get(),
        b_desc_.get(), c_desc_.get(), d_desc_.get(), preference.get(),
        max_algorithm_count, results.data(), &found_algorithm_count);
    if (error != 0) {
      VLOG(0) << "hipblasLtMatmulAlgoGetHeuristic returned " << (int)error;
      SE_HIPBLAS_RETURN_IF_ERROR(error);
    }
    results.resize(found_algorithm_count);
  }  // end mutex block

  std::vector<MatmulAlgorithm> algorithms;
  algorithms.reserve(results.size());
  for (const hipblasLtMatmulHeuristicResult_t& result : results) {
    if (result.state == HIPBLAS_STATUS_SUCCESS) {  // Skip failed algos.
      algorithms.push_back({result.algo, result.workspaceSize});
    }
  }
  return std::move(algorithms);
}

auto BlasLt::GetMatmulPlan(const gpu::GemmConfig& cfg, Epilogue epilogue) const
    -> tsl::StatusOr<MatmulPlanPtr> {
  auto lhs_layout = cfg.lhs_layout, rhs_layout = cfg.rhs_layout,
       output_layout = cfg.output_layout, c_layout = cfg.c_layout;

  // cublasLt matmul requires batch sizes to be equal. If only one operand has a
  // batch, the other will be broadcast (as its batch_stride == 0).
  size_t batch_size = std::max(lhs_layout.batch_size, rhs_layout.batch_size);
  lhs_layout.batch_size = batch_size;
  rhs_layout.batch_size = batch_size;

  bool must_swap_operands =
      MakeOutputColumnMajor(lhs_layout, rhs_layout, output_layout, &c_layout);

  // Do not transpose either input. Note the cuBLASLt documentation somewhat
  // incorrectly claims "A must be transposed and B non-transposed" when A and B
  // are FP8 (https://docs.nvidia.com/cuda/cublas/#cublasltmatmul). In reality,
  // this is only true if A and B are column-major. If A is row-major, A must
  // *not* be transposed, and if B is row-major, B must be transposed. We never
  // transpose A or B, and expect the caller to ensure A is row-major and B is
  // column when A and B are FP8.
  auto trans_a = lhs_layout.transpose ? *lhs_layout.transpose
                                      : blas::Transpose::kNoTranspose;
  auto trans_b = rhs_layout.transpose ? *rhs_layout.transpose
                                      : blas::Transpose::kNoTranspose;

  if (xla::primitive_util::IsF8Type(lhs_layout.dtype) &&
      lhs_layout.order == gpu::MatrixLayout::Order::kColumnMajor) {
    return xla::InternalError("The F8 LHS must be column-major");
  }
  if (xla::primitive_util::IsF8Type(rhs_layout.dtype) &&
      rhs_layout.order == gpu::MatrixLayout::Order::kRowMajor) {
    return xla::InternalError("The F8 RHS must be row-major");
  }

  TF_ASSIGN_OR_RETURN(auto output_dtype,
                      gpu::AsBlasDataType(output_layout.dtype));

  auto compute_type = cfg.compute_type;
  if (!compute_type) {  // obtain compute_type unless provided by the user
    TF_ASSIGN_OR_RETURN(compute_type, gpu::GetBlasComputationType(
                                          lhs_layout.dtype, output_layout.dtype,
                                          cfg.compute_precision));
  }

  if (lhs_layout.order == gpu::MatrixLayout::Order::kRowMajor) {
    trans_a = blas::Transpose::kTranspose;
    lhs_layout.Transpose();
  }
  if (rhs_layout.order == gpu::MatrixLayout::Order::kRowMajor) {
    trans_b = blas::Transpose::kTranspose;
    rhs_layout.Transpose();
  }

  TF_ASSIGN_OR_RETURN(
      auto op_desc,
      MatmulDesc::Create(*compute_type,
                         gpu::GetScaleType(output_dtype, *compute_type),
                         trans_a, trans_b, epilogue));

  TF_ASSIGN_OR_RETURN(auto a_desc, MatrixLayout::Create(lhs_layout));
  TF_ASSIGN_OR_RETURN(auto b_desc, MatrixLayout::Create(rhs_layout));
  TF_ASSIGN_OR_RETURN(auto c_desc, MatrixLayout::Create(c_layout));
  TF_ASSIGN_OR_RETURN(auto d_desc, MatrixLayout::Create(output_layout));

  // std::make_unique won't work with brace initialization in C++17 ;(
  return std::make_unique<MatmulPlan>(*this, std::move(op_desc),
                                      std::move(a_desc), std::move(b_desc),
                                      std::move(c_desc), std::move(d_desc),
                                      cfg.alpha, cfg.beta, must_swap_operands);
}

tsl::Status BlasLt::MatmulPlan::ValidateInputs(
    blas::DataType scale_type, bool alpha_on_device, bool beta_on_device,
    blas::DataType A_type, blas::DataType B_type, blas::DataType C_type,
    blas::DataType D_type) const {
  if (AsHipblasDataType(scale_type) != op_desc_.scale_type()) {
    return tsl::errors::InvalidArgument("mismatched scale types");
  }

  bool expect_scale_factor_on_device =
      (op_desc_.pointer_mode() == HIPBLAS_POINTER_MODE_DEVICE);

  if (alpha_on_device != expect_scale_factor_on_device) {
    return tsl::errors::InvalidArgument("wrong location for alpha");
  }

  if (beta_on_device != expect_scale_factor_on_device) {
    return tsl::errors::InvalidArgument("wrong location for beta");
  }

  if (AsHipblasDataType(A_type) != a_desc_.type()) {
    return tsl::errors::InvalidArgument("mismatched A matrix types");
  }

  if (AsHipblasDataType(B_type) != b_desc_.type()) {
    return tsl::errors::InvalidArgument("mismatched B matrix types");
  }

  if (AsHipblasDataType(C_type) != c_desc_.type()) {
    return tsl::errors::InvalidArgument("mismatched C matrix types");
  }

  if (AsHipblasDataType(D_type) != d_desc_.type()) {
    return tsl::errors::InvalidArgument("mismatched D matrix types");
  }

  return tsl::OkStatus();
}

tsl::Status BlasLt::MatmulPlan::DoMatmul(
    Stream* stream, const void* alpha, DeviceMemoryBase a, DeviceMemoryBase b,
    const void* beta, DeviceMemoryBase c, DeviceMemoryBase d,
    const MatmulAlgorithm& algorithm, ScratchAllocator& scratch_allocator,
    DeviceMemoryBase bias, DeviceMemoryBase aux, DeviceMemoryBase a_scale,
    DeviceMemoryBase b_scale, DeviceMemoryBase c_scale,
    DeviceMemoryBase d_scale, DeviceMemoryBase d_amax,
    blas::ProfileResult* profile_result) const {
  TF_ASSIGN_OR_RETURN(
      std::optional<gpu::GpuTimer> timer,
      gpu::GpuTimer::CreateIfNeeded(gpu::AsGpuStream(stream), profile_result));

  void* workspace = nullptr;
  if (algorithm.workspace_size > 0) {
    TF_ASSIGN_OR_RETURN(
        DeviceMemory<uint8_t> alloc,
        scratch_allocator.AllocateBytes(algorithm.workspace_size));
    workspace = gpu::GpuMemoryMutable(&alloc);
  }

  auto palgo = std::any_cast<hipblasLtMatmulAlgo_t>(&algorithm.opaque_algo);
  {
    absl::MutexLock lock(&blas_lt_ref_.mu_);
    TF_RET_CHECK(blas_lt_ref_.blas_lt_ != nullptr);
    // We must set the bias and aux pointers while holding the mutex, to avoid a
    // potential race condition from multiple threads sharing the same plan.
    if (bias != nullptr) {
      TF_RETURN_IF_ERROR(SetAttr(
          op_desc_.get(), HIPBLASLT_MATMUL_DESC_BIAS_POINTER, bias.opaque()));
    }

    if ((a_scale != nullptr) || (b_scale != nullptr) || (c_scale != nullptr) ||
        (d_scale != nullptr)) {
      return tsl::errors::Internal("hipblaslt does not support scale");
    }

    if (d_amax != nullptr) {
      return tsl::errors::Internal("hipblaslt does not support amax");
    }

    if (aux != nullptr) {
      return tsl::errors::Internal(
          "hipblaslt does not support auxiliary inputs / outputs");
    }

    gpu::ScopedActivateExecutorContext sac{blas_lt_ref_.parent_};

    if (palgo != nullptr) {
      SE_HIPBLAS_RETURN_IF_ERROR(wrap::hipblasLtMatmul(
          blas_lt_ref_.blas_lt_.get(), op_desc_.get(), alpha, a.opaque(),
          a_desc_.get(), b.opaque(), b_desc_.get(), beta, c.opaque(),
          c_desc_.get(), d.opaque(), d_desc_.get(), palgo, workspace,
          algorithm.workspace_size, gpu::AsGpuStreamValue(stream)));
    } else {
      return tsl::errors::Internal("hipblaslt: Invalid algorithm type");
    }
  }

  if (profile_result != nullptr) {
    TF_ASSIGN_OR_RETURN(absl::Duration elapsed, timer->GetElapsedDuration());
    // set algorithm ID to be unique (otherwise it gets kDefaultAlgorithm ID)
    profile_result->set_algorithm(reinterpret_cast<blas::AlgorithmType>(palgo));
    profile_result->set_is_valid(true);
    profile_result->set_elapsed_time_in_ms(absl::ToDoubleMilliseconds(elapsed));
  }
  return tsl::OkStatus();
}

namespace {

template <hipDataType>
struct HipToNativeT;

template <>
struct HipToNativeT<HIP_R_16BF> {
  using type = Eigen::bfloat16;
};
template <>
struct HipToNativeT<HIP_R_16F> {
  using type = Eigen::half;
};
template <>
struct HipToNativeT<HIP_R_32F> {
  using type = float;
};
template <>
struct HipToNativeT<HIP_R_64F> {
  using type = double;
};
template <>
struct HipToNativeT<HIP_C_32F> {
  using type = complex64;
};
template <>
struct HipToNativeT<HIP_C_64F> {
  using type = complex128;
};

}  // namespace

tsl::Status BlasLt::MatmulPlan::ExecuteOnStream(
    Stream* stream, DeviceMemoryBase a, DeviceMemoryBase b, DeviceMemoryBase c,
    DeviceMemoryBase d, DeviceMemoryBase bias, DeviceMemoryBase aux,
    DeviceMemoryBase a_scale, DeviceMemoryBase b_scale,
    DeviceMemoryBase c_scale, DeviceMemoryBase d_scale, DeviceMemoryBase d_amax,
    const MatmulAlgorithm& algorithm, ScratchAllocator& scratch_allocator,
    blas::ProfileResult* profile_result) const {
  if (must_swap_operands_) {
    std::swap(a, b);
  }

  std::tuple operand_types{a_desc_.type(), b_desc_.type(), c_desc_.type(),
                           d_desc_.type()};

#define TYPED_MATMUL(SCALENTYPE, ATYPE, BTYPE, CTYPE, DTYPE)              \
  if (operand_types == std::make_tuple(ATYPE, BTYPE, CTYPE, DTYPE)) {     \
    return gpu::BlasLt::MatmulPlan::DoMatmul<                             \
        SCALENTYPE, HipToNativeT<ATYPE>::type, HipToNativeT<BTYPE>::type, \
        HipToNativeT<CTYPE>::type, HipToNativeT<DTYPE>::type>(            \
        stream, alpha_, a, b, beta_, c, d, bias, aux, a_scale, b_scale,   \
        c_scale, d_scale, d_amax, algorithm, scratch_allocator,           \
        profile_result);                                                  \
  }

  // Other data types:
  TYPED_MATMUL(float, HIP_R_16BF, HIP_R_16BF, HIP_R_16BF, HIP_R_16BF)
  TYPED_MATMUL(float, HIP_R_16F, HIP_R_16F, HIP_R_16F, HIP_R_16F)
  TYPED_MATMUL(float, HIP_R_16BF, HIP_R_16BF, HIP_R_32F, HIP_R_32F)
  TYPED_MATMUL(float, HIP_R_16F, HIP_R_16F, HIP_R_32F, HIP_R_32F)
  TYPED_MATMUL(float, HIP_R_32F, HIP_R_32F, HIP_R_32F, HIP_R_32F)
  TYPED_MATMUL(double, HIP_R_64F, HIP_R_64F, HIP_R_64F, HIP_R_64F)
  TYPED_MATMUL(complex64, HIP_C_32F, HIP_C_32F, HIP_C_32F, HIP_C_32F)
  TYPED_MATMUL(complex128, HIP_C_64F, HIP_C_64F, HIP_C_64F, HIP_C_64F)

#undef TYPED_MATMUL

  return xla::InternalError("Unexpected dtype");
}

}  // namespace rocm

}  // namespace stream_executor

#endif  // TF_HIPBLASLT
