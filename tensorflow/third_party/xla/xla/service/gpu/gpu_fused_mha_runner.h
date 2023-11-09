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

#ifndef XLA_SERVICE_GPU_GPU_FUSED_MHA_RUNNER_H_
#define XLA_SERVICE_GPU_GPU_FUSED_MHA_RUNNER_H_

#include <memory>
#include <optional>
#include <string>
#include <utility>
#include <variant>
#include <vector>

#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/hlo/ir/hlo_instructions.h"
#include "xla/service/gpu/backend_configs.pb.h"
#include "xla/service/gpu/cublas_cudnn.h"
#include "xla/status.h"
#include "xla/statusor.h"
#include "xla/stream_executor/dnn.h"
#include "xla/stream_executor/lazy_op_runner.h"
#include "xla/stream_executor/stream_executor.h"
#include "xla/types.h"
#include "xla/xla_data.pb.h"

namespace xla {
namespace gpu {

// This is an interim structure to hold the parameters to construct a
// GpufMHAConfig.
// Struct to describe properties of a FMHA without being tied to specific
// IR. Will be used to help build FMHA thunks from either XLA HLO or
// LHLO GPU dialect in MLIR.
struct GpufMHADescriptor {
  CudnnfMHAKind kind;
  CudnnfMHABackendConfig backend_config;
  Shape lhs_bmm1_shape;
  Shape rhs_bmm1_shape;
  Shape rhs_bmm2_shape;
  Shape intermediate_lhs_bmm2_shape;
  // This will contain both output shape and activation shape
  std::vector<Shape> output_shapes;
  DotDimensionNumbers bmm1_dnums;
  DotDimensionNumbers bmm2_dnums;

  std::optional<Shape> mask_shape;
  std::optional<Shape> bias_shape;
};

struct GpufMHABackwardDescriptor {
  CudnnfMHAKind kind;
  CudnnfMHABackendConfig backend_config;
  Shape bmm1_grad_gemm1_rhs_shape;
  Shape bmm1_grad_gemm2_rhs_shape;
  Shape bmm2_grad_gemm1_lhs_shape;
  Shape bmm2_grad_gemm2_rhs_shape;
  Shape d_output_shape;
  Shape d_bmm1_lhs_shape;
  Shape d_bmm1_rhs_shape;
  Shape d_bmm2_rhs_shape;
  DotDimensionNumbers bmm1_grad_gemm1_dnums;
  DotDimensionNumbers bmm1_grad_gemm2_dnums;
  DotDimensionNumbers bmm2_grad_gemm1_dnums;
  DotDimensionNumbers bmm2_grad_gemm2_dnums;

  std::optional<Shape> mask_shape;
  std::optional<Shape> d_bias_shape;
};
// Structure to describe static properties of a GPU fused Multi-Headed
// Attention.
struct GpufMHAConfig {
  static StatusOr<GpufMHAConfig> For(const GpufMHADescriptor& fmha_desc);
  PrimitiveType
      input_type;  // Capture the primitive type of one of the inputs of BMM1
  PrimitiveType output_type;
  CudnnfMHAKind kind;
  std::optional<double> fmha_scale;
  std::optional<double> dropout_rate;
  std::optional<int64_t> seed;

  se::dnn::AlgorithmDesc algorithm;

  // bias -> [1, num_attn_heads, q_seq_len, kv_seq_len]
  // mask -> [batch_size, 1, q_seq_len, kv_seq_len]
  se::dnn::MatmulTensorDescriptor lhs_bmm1;
  se::dnn::MatmulTensorDescriptor rhs_bmm1;
  se::dnn::MatmulTensorDescriptor rhs_bmm2;
  se::dnn::MatmulTensorDescriptor intermediate_lhs_bmm2;
  se::dnn::TensorDescriptor output;

  std::optional<se::dnn::TensorDescriptor> activation;
  std::optional<se::dnn::TensorDescriptor> mask;
  std::optional<se::dnn::TensorDescriptor> bias;
};

// Structure to describe static properties of a GPU fused Multi-Headed
// Attention backward.
struct GpufMHABackwardConfig {
  static StatusOr<GpufMHABackwardConfig> For(
      const GpufMHABackwardDescriptor& fmha_desc);
  PrimitiveType
      input_type;  // Capture the primitive type of one of the inputs of BMM1
  PrimitiveType output_type;
  CudnnfMHAKind kind;
  std::optional<double> fmha_scale;
  std::optional<double> dropout_rate;
  std::optional<int64_t> seed;

  se::dnn::AlgorithmDesc algorithm;

  // mask -> [batch_size, 1, q_seq_len, kv_seq_len]
  // d_bias -> [1, num_heads, q_seq_len, kv_seq_len]
  se::dnn::MatmulTensorDescriptor bmm1_grad_gemm1_rhs;
  se::dnn::MatmulTensorDescriptor bmm1_grad_gemm2_rhs;
  se::dnn::MatmulTensorDescriptor bmm2_grad_gemm1_lhs;
  se::dnn::MatmulTensorDescriptor bmm2_grad_gemm2_rhs;
  se::dnn::MatmulTensorDescriptor d_output;
  se::dnn::TensorDescriptor d_bmm1_lhs;
  se::dnn::TensorDescriptor d_bmm1_rhs;
  se::dnn::TensorDescriptor d_bmm2_rhs;
  se::dnn::TensorDescriptor d_s;
  std::optional<se::dnn::TensorDescriptor> d_bias;
  std::optional<se::dnn::TensorDescriptor> mask;
};

// Implementation struct exposed for debugging and log analysis.
struct GpufMHAParams {
  static StatusOr<GpufMHAParams> For(
      const GpufMHAConfig& config, se::DeviceMemoryBase lhs_bmm1_buffer,
      se::DeviceMemoryBase rhs_bmm1_buffer,
      se::DeviceMemoryBase rhs_bmm2_buffer, se::DeviceMemoryBase output_buffer,
      std::optional<se::DeviceMemoryBase> mask_buffer,
      std::optional<se::DeviceMemoryBase> bias_buffer,
      std::optional<se::DeviceMemoryBase> activation_buffer);

  const GpufMHAConfig* config;  // Not owned
  se::DeviceMemoryBase lhs_bmm1_buffer;
  se::DeviceMemoryBase rhs_bmm1_buffer;
  se::DeviceMemoryBase rhs_bmm2_buffer;
  se::DeviceMemoryBase output_buffer;
  std::optional<se::DeviceMemoryBase> activation_buffer;
  std::optional<se::DeviceMemoryBase> mask_buffer;
  std::optional<se::DeviceMemoryBase> bias_buffer;
};

struct GpufMHABackwardParams {
  static StatusOr<GpufMHABackwardParams> For(
      const GpufMHABackwardConfig& config,
      se::DeviceMemoryBase bmm1_grad_gemm1_rhs_buffer,
      se::DeviceMemoryBase bmm1_grad_gemm2_rhs_buffer,
      se::DeviceMemoryBase bmm2_grad_gemm1_lhs_buffer,
      se::DeviceMemoryBase bmm2_grad_gemm2_rhs_buffer,
      se::DeviceMemoryBase d_output_buffer,
      se::DeviceMemoryBase d_bmm1_lhs_buffer,
      se::DeviceMemoryBase d_bmm1_rhs_buffer,
      se::DeviceMemoryBase d_bmm2_rhs_buffer, se::DeviceMemoryBase d_s_buffer,
      std::optional<se::DeviceMemoryBase> mask_buffer,
      std::optional<se::DeviceMemoryBase> d_bias_buffer);

  const GpufMHABackwardConfig* config;  // Not owned
  se::DeviceMemoryBase bmm1_grad_gemm1_rhs_buffer;
  se::DeviceMemoryBase bmm1_grad_gemm2_rhs_buffer;
  se::DeviceMemoryBase bmm2_grad_gemm1_lhs_buffer;
  se::DeviceMemoryBase bmm2_grad_gemm2_rhs_buffer;
  se::DeviceMemoryBase d_output_buffer;
  se::DeviceMemoryBase d_bmm1_lhs_buffer;
  se::DeviceMemoryBase d_bmm1_rhs_buffer;
  se::DeviceMemoryBase d_bmm2_rhs_buffer;
  se::DeviceMemoryBase d_s_buffer;
  std::optional<se::DeviceMemoryBase> d_bias_buffer;
  std::optional<se::DeviceMemoryBase> mask_buffer;
};

class FusedMultiHeadedAttentionRunner {
 public:
  using Repr =
      std::variant<std::monostate,  // To allow XXX default ctor
                   std::unique_ptr<se::dnn::LazyOpRunner<se::dnn::FusedMHAOp>>>;

  FusedMultiHeadedAttentionRunner() = default;

  explicit FusedMultiHeadedAttentionRunner(
      std::unique_ptr<se::dnn::LazyOpRunner<se::dnn::FusedMHAOp>> runner)
      : repr_(std::move(runner)) {}

  explicit FusedMultiHeadedAttentionRunner(Repr runner)
      : repr_(std::move(runner)) {}

  explicit FusedMultiHeadedAttentionRunner(const GpufMHAConfig& config)
      : FusedMultiHeadedAttentionRunner(CreateRunner(config)) {
    if (std::holds_alternative<std::monostate>(repr_)) {
      CHECK(false) << "Cannot construct FusedMultiHeadedAttentionRunner with "
                      "std::monostate";
    }
  }

  se::dnn::AlgorithmDesc ToAlgorithmDesc() const {
    return std::visit(ToAlgorithmDescVisitor{}, repr_);
  }

  se::dnn::LazyOpRunner<se::dnn::FusedMHAOp>* AsFusedMHARunner() {
    CHECK(std::holds_alternative<
          std::unique_ptr<se::dnn::LazyOpRunner<se::dnn::FusedMHAOp>>>(repr_));
    return std::get<
               std::unique_ptr<se::dnn::LazyOpRunner<se::dnn::FusedMHAOp>>>(
               repr_)
        .get();
  }

 private:
  //  The CreateRunner function is defined as static because it
  //  doesn't need access to any non-static member variables of the
  //  FusedMultiHeadedAttentionRunner class. Defining it static makes it easy to
  //  use and makes it clear that it is a utility function that doesn't rely on
  //  the state of any specific instance of the class.
  static Repr CreateRunner(const GpufMHAConfig& config) {
    switch (config.kind) {
      case CudnnfMHAKind::kBmmBmm:
      case CudnnfMHAKind::kSoftmaxDropout:
      case CudnnfMHAKind::kSoftmax:
      case CudnnfMHAKind::kScaleBiasSoftmax:
      case CudnnfMHAKind::kScaleBiasSoftmaxDropout:
      case CudnnfMHAKind::kScaleMaskSoftmax:
      case CudnnfMHAKind::kScaleMaskSoftmaxDropout:
      case CudnnfMHAKind::kScaleBiasMaskSoftmax:
      case CudnnfMHAKind::kScaleBiasMaskSoftmaxDropout:
        return std::make_unique<se::dnn::LazyOpRunner<se::dnn::FusedMHAOp>>(
            config.algorithm);
      default:
        LOG(FATAL) << "Internal error: unsupported CUDNN MHA kind in "
                      "FusedMultiHeadedAttentionRunner";
    }
  }

  struct ToAlgorithmDescVisitor {
    template <typename RunnerPtr>
    se::dnn::AlgorithmDesc operator()(const RunnerPtr& runner) {
      return runner->ToAlgorithmDesc();
    }

    se::dnn::AlgorithmDesc operator()(const std::monostate&) {
      CHECK(false) << "Internal error: uninitialized runner in ToAlgorithmDesc";
    }
  };

  Repr repr_;
};

class FusedMultiHeadedAttentionBackwardRunner {
 public:
  using Repr = std::variant<
      std::monostate,  // To allow XXX default ctor
      std::unique_ptr<se::dnn::LazyOpRunner<se::dnn::FusedMHABackwardOp>>>;

  FusedMultiHeadedAttentionBackwardRunner() = default;

  explicit FusedMultiHeadedAttentionBackwardRunner(
      std::unique_ptr<se::dnn::LazyOpRunner<se::dnn::FusedMHABackwardOp>>
          runner)
      : repr_(std::move(runner)) {}

  explicit FusedMultiHeadedAttentionBackwardRunner(Repr runner)
      : repr_(std::move(runner)) {}

  explicit FusedMultiHeadedAttentionBackwardRunner(
      const GpufMHABackwardConfig& config)
      : FusedMultiHeadedAttentionBackwardRunner(CreateRunner(config)) {
    if (std::holds_alternative<std::monostate>(repr_)) {
      CHECK(false)
          << "Cannot construct FusedMultiHeadedAttentionBackwardRunner with "
             "std::monostate";
    }
  }

  se::dnn::AlgorithmDesc ToAlgorithmDesc() const {
    return std::visit(ToAlgorithmDescVisitor{}, repr_);
  }

  se::dnn::LazyOpRunner<se::dnn::FusedMHABackwardOp>*
  AsFusedMHABackwardRunner() {
    CHECK(std::holds_alternative<
          std::unique_ptr<se::dnn::LazyOpRunner<se::dnn::FusedMHABackwardOp>>>(
        repr_));
    return std::get<std::unique_ptr<
        se::dnn::LazyOpRunner<se::dnn::FusedMHABackwardOp>>>(repr_)
        .get();
  }

 private:
  //  The CreateRunner function is defined as static because it
  //  doesn't need access to any non-static member variables of the
  //  FusedMultiHeadedAttentionBackwardRunner class. Defining it static makes it
  //  easy to use and makes it clear that it is a utility function that doesn't
  //  rely on the state of any specific instance of the class.
  static Repr CreateRunner(const GpufMHABackwardConfig& config) {
    switch (config.kind) {
      case CudnnfMHAKind::kBackwardBmmBmm:
      case CudnnfMHAKind::kBackwardSoftmaxDropout:
      case CudnnfMHAKind::kBackwardSoftmax:
      case CudnnfMHAKind::kBackwardScaleBiasSoftmax:
      case CudnnfMHAKind::kBackwardScaleBiasSoftmaxDropout:
      case CudnnfMHAKind::kBackwardScaleBiasMaskSoftmax:
      case CudnnfMHAKind::kBackwardScaleBiasMaskSoftmaxDropout:
      case CudnnfMHAKind::kBackwardScaleMaskSoftmax:
      case CudnnfMHAKind::kBackwardScaleMaskSoftmaxDropout:
        return std::make_unique<
            se::dnn::LazyOpRunner<se::dnn::FusedMHABackwardOp>>(
            config.algorithm);
      default:
        LOG(FATAL) << "Internal error: unsupported CUDNN MHA kind in "
                      "FusedMultiHeadedAttentionBackwardRunner";
    }
  }

  struct ToAlgorithmDescVisitor {
    template <typename RunnerPtr>
    se::dnn::AlgorithmDesc operator()(const RunnerPtr& runner) {
      return runner->ToAlgorithmDesc();
    }

    se::dnn::AlgorithmDesc operator()(const std::monostate&) {
      CHECK(false) << "Internal error: uninitialized runner in ToAlgorithmDesc";
    }
  };

  Repr repr_;
};

struct RunFusedMHAOptions {
  // Nullable output-parameter pointer for profiling results.
  // Profile results remain unused for now since cuDNN FMHA has only one
  // algorithm for now.
  se::dnn::ProfileResult* profile_result = nullptr;

  // Use this runner cache (and its configured algorithm), instead of the one
  // from the instruction.
  FusedMultiHeadedAttentionRunner* runner_cache;
};

struct RunFusedMHABackwardOptions {
  // Nullable output-parameter pointer for profiling results.
  // Profile results remain unused for now since cuDNN FMHA has only one
  // algorithm for now.
  se::dnn::ProfileResult* profile_result = nullptr;

  // Use this runner cache (and its configured algorithm), instead of the one
  // from the instruction.
  FusedMultiHeadedAttentionBackwardRunner* runner_cache;
};

Status RunGpuFMHA(const GpufMHAConfig& fmha_config,
                  se::DeviceMemoryBase lhs_bmm1_buffer,
                  se::DeviceMemoryBase rhs_bmm1_buffer,
                  se::DeviceMemoryBase rhs_bmm2_buffer,
                  se::DeviceMemoryBase output_buffer,
                  se::DeviceMemoryBase scratch_buffer,
                  std::optional<se::DeviceMemoryBase> mask_buffer,
                  std::optional<se::DeviceMemoryBase> bias_buffer,
                  std::optional<se::DeviceMemoryBase> activation_buffer,
                  se::Stream* stream, RunFusedMHAOptions = {});

Status RunGpuFMHABackward(const GpufMHABackwardConfig& fmha_config,
                          se::DeviceMemoryBase bmm1_grad_gemm1_rhs_buffer,
                          se::DeviceMemoryBase bmm1_grad_gemm2_rhs_buffer,
                          se::DeviceMemoryBase bmm2_grad_gemm1_lhs_buffer,
                          se::DeviceMemoryBase bmm2_grad_gemm2_rhs_buffer,
                          se::DeviceMemoryBase d_output_buffer,
                          se::DeviceMemoryBase scratch_buffer,
                          se::DeviceMemoryBase d_bmm1_lhs_buffer,
                          se::DeviceMemoryBase d_bmm1_rhs_buffer,
                          se::DeviceMemoryBase d_bmm2_rhs_buffer,
                          se::DeviceMemoryBase d_s_buffer,
                          std::optional<se::DeviceMemoryBase> mask_buffer,
                          std::optional<se::DeviceMemoryBase> d_bias_buffer,
                          se::Stream* stream, RunFusedMHABackwardOptions = {});

std::string ToString(const GpufMHAConfig& config);

}  // namespace gpu
}  // namespace xla
#endif  // XLA_SERVICE_GPU_GPU_FUSED_MHA_RUNNER_H_
