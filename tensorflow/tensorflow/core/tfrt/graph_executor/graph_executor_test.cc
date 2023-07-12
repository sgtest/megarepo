/* Copyright 2021 The TensorFlow Authors. All Rights Reserved.

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
#include "tensorflow/core/tfrt/graph_executor/graph_executor.h"

#include <memory>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include "learning/brain/experimental/tfrt/native_lowering/kernels/math_kernels.h"
#include "learning/brain/experimental/tfrt/native_lowering/kernels/sync_fallback_kernels.h"
#include <gmock/gmock.h>
#include <gtest/gtest.h>
#include "tensorflow/cc/ops/array_ops.h"
#include "tensorflow/cc/ops/const_op.h"
#include "tensorflow/core/framework/graph.pb.h"
#include "tensorflow/core/framework/types.pb.h"
#include "tensorflow/core/graph/graph_def_builder.h"
#include "tensorflow/core/grappler/utils/grappler_test.h"
#include "tensorflow/core/platform/statusor.h"
#include "tensorflow/core/protobuf/rewriter_config.pb.h"
#include "tensorflow/core/tfrt/mlrt/interpreter/context.h"
#include "tensorflow/core/tfrt/mlrt/interpreter/value.h"
#include "tensorflow/core/tfrt/mlrt/kernel/kernel.h"
#include "tensorflow/core/tfrt/saved_model/saved_model_testutil.h"
#include "tensorflow/tsl/lib/core/status_test_util.h"
#include "tensorflow/tsl/platform/status.h"
#include "tensorflow/tsl/platform/statusor.h"
#include "tfrt/cpp_tests/test_util.h""  // from @tf_runtime
#include "tfrt/tensor/dense_host_tensor.h"  // from @tf_runtime

namespace tensorflow {
namespace tfrt_stub {
namespace {

using ::testing::status::StatusIs;

class GraphExecutorTest : public ::testing::TestWithParam<bool> {};

tensorflow::Status GetSimpleGraphDef(GraphDef& graph_def) {
  auto scope = tensorflow::Scope::NewRootScope().WithDevice("/device:CPU:0");

  auto input = ops::Placeholder(scope.WithOpName("input"), DT_INT32);
  auto rank = ops::Rank(scope.WithOpName("rank"), input);

  return scope.ToGraphDef(&graph_def);
}

std::unique_ptr<mlrt::KernelRegistry> GetKernelRegistry() {
  auto kernel_registry = std::make_unique<mlrt::KernelRegistry>();
  tensorflow::tf_mlrt::RegisterTfMlrtKernels(*kernel_registry);
  tfrt::cpu::RegisterMlrtMathKernels(kernel_registry.get());
  tfrt::cpu::RegisterMlrtFallbackCompatKernels(kernel_registry.get());

  return kernel_registry;
}

TEST_P(GraphExecutorTest, Vanilla) {
  GraphDef graph_def;
  TF_ASSERT_OK(GetSimpleGraphDef(graph_def));

  auto runtime = DefaultTfrtRuntime(/*num_threads=*/1);
  GraphExecutor::Options options(runtime.get());
  options.enable_mlrt = GetParam();

  TF_ASSERT_OK_AND_ASSIGN(
      auto fallback_state,
      tensorflow::tfrt_stub::FallbackState::Create(
          CreateDefaultSessionOptions(options), graph_def.library()))
  auto resource_context = std::make_unique<tfrt::ResourceContext>();
  TF_ASSERT_OK_AND_ASSIGN(
      auto graph_executor,
      GraphExecutor::Create(std::move(options), *fallback_state,
                            std::move(resource_context), graph_def,
                            GetKernelRegistry()));

  // Set input 'x' to [[1, 1, 1]]
  std::vector<std::pair<std::string, tensorflow::Tensor>> inputs;
  inputs.push_back({"input", CreateTfTensor<int32_t>(
                                 /*shape=*/{1, 3}, /*data=*/{1, 1, 1})});

  std::vector<tensorflow::Tensor> outputs;

  TF_ASSERT_OK(graph_executor->Run(/*run_options=*/{}, inputs,
                                   /*output_tensor_names=*/{"rank"},
                                   /*target_tensor_names=*/{}, &outputs));
  ASSERT_EQ(outputs.size(), 1);

  EXPECT_THAT(GetTfTensorData<int32_t>(outputs[0]),
              ::testing::ElementsAreArray({2}));
}

TEST_P(GraphExecutorTest, BasicWithOnlineCostAnalysis) {
  GraphDef graph_def;
  TF_ASSERT_OK(GetSimpleGraphDef(graph_def));

  auto runtime = DefaultTfrtRuntime(/*num_threads=*/1);
  GraphExecutor::Options options(runtime.get());
  options.enable_online_cost_analysis = true;
  options.enable_mlrt = GetParam();

  TF_ASSERT_OK_AND_ASSIGN(
      auto fallback_state,
      tensorflow::tfrt_stub::FallbackState::Create(
          CreateDefaultSessionOptions(options), graph_def.library()));
  auto resource_context = std::make_unique<tfrt::ResourceContext>();
  TF_ASSERT_OK_AND_ASSIGN(
      auto graph_executor,
      GraphExecutor::Create(std::move(options), *fallback_state,
                            std::move(resource_context), graph_def,
                            GetKernelRegistry()));

  // Set input 'x' to [[1, 1, 1]]
  std::vector<std::pair<std::string, tensorflow::Tensor>> inputs;
  inputs.push_back({"input", CreateTfTensor<int32_t>(
                                 /*shape=*/{1, 3}, /*data=*/{1, 1, 1})});

  std::vector<tensorflow::Tensor> outputs;

  // A first run should trigger online cost analysis.
  TF_ASSERT_OK(graph_executor->Run(/*run_options=*/{}, inputs,
                                   /*output_tensor_names=*/{"rank"},
                                   /*target_tensor_names=*/{}, &outputs));
  ASSERT_EQ(outputs.size(), 1);

  EXPECT_THAT(GetTfTensorData<int32_t>(outputs[0]),
              ::testing::ElementsAreArray({2}));

  // A second run should use re-compiled graph with online profiled costs.
  TF_ASSERT_OK(graph_executor->Run(/*run_options=*/{}, inputs,
                                   /*output_tensor_names=*/{"rank"},
                                   /*target_tensor_names=*/{}, &outputs));
  ASSERT_EQ(outputs.size(), 1);

  EXPECT_THAT(GetTfTensorData<int32_t>(outputs[0]),
              ::testing::ElementsAreArray({2}));
}

REGISTER_OP("TestCancel")
    .Input("x: T")
    .Output("z: T")
    .Attr("T: {int32}")
    .SetShapeFn(::tensorflow::shape_inference::UnchangedShape);

class TestCancelKernel : public OpKernel {
 public:
  explicit TestCancelKernel(OpKernelConstruction* context)
      : OpKernel(context) {}

  void Compute(OpKernelContext* ctx) override {
    auto status = absl::CancelledError();
    ctx->cancellation_manager()->StartCancelWithStatus(status);
    ctx->SetStatus(status);
  }
};

REGISTER_KERNEL_BUILDER(Name("TestCancel").Device(DEVICE_CPU),
                        TestCancelKernel);

REGISTER_OP("TestIsCancelled").Output("z: T").Attr("T: {bool}").SetIsStateful();

class TestIsCancelledKernel : public OpKernel {
 public:
  explicit TestIsCancelledKernel(OpKernelConstruction* context)
      : OpKernel(context) {}

  void Compute(OpKernelContext* ctx) override {
    ctx->set_output(
        0, tensorflow::Tensor(ctx->cancellation_manager()->IsCancelled()));
  }
};

REGISTER_KERNEL_BUILDER(Name("TestIsCancelled").Device(DEVICE_CPU),
                        TestIsCancelledKernel);

TEST_P(GraphExecutorTest, Cancellation) {
  GraphDef graph_def;

  tensorflow::GraphDefBuilder builder(
      tensorflow::GraphDefBuilder::kFailImmediately);

  const tensorflow::TensorShape tensor_shape({10, 9});
  tensorflow::Node* input = tensorflow::ops::SourceOp(
      "Placeholder", builder.opts()
                         .WithName("input")
                         .WithAttr("dtype", tensorflow::DT_INT32)
                         .WithAttr("shape", tensor_shape));
  tensorflow::ops::SourceOp("TestIsCancelled",
                            builder.opts()
                                .WithName("is_cancelled")
                                .WithAttr("T", tensorflow::DT_BOOL));
  tensorflow::ops::UnaryOp("TestCancel", input,
                           builder.opts()
                               .WithName("test_cancel")
                               .WithAttr("T", tensorflow::DT_INT32));

  TF_ASSERT_OK(builder.ToGraphDef(&graph_def));

  auto runtime = DefaultTfrtRuntime(/*num_threads=*/1);
  GraphExecutor::Options options(runtime.get());
  options.enable_mlrt = GetParam();

  TF_ASSERT_OK_AND_ASSIGN(
      auto fallback_state,
      tensorflow::tfrt_stub::FallbackState::Create(
          CreateDefaultSessionOptions(options), graph_def.library()))
  auto resource_context = std::make_unique<tfrt::ResourceContext>();
  TF_ASSERT_OK_AND_ASSIGN(
      auto graph_executor,
      GraphExecutor::Create(std::move(options), *fallback_state,
                            std::move(resource_context), graph_def,
                            GetKernelRegistry()));
  {
    std::vector<std::pair<std::string, tensorflow::Tensor>> inputs;
    inputs.push_back({"input", CreateTfTensor<int32_t>(
                                   /*shape=*/{1, 3}, /*data=*/{1, 1, 1})});

    std::vector<tensorflow::Tensor> outputs;
    EXPECT_THAT(graph_executor->Run(/*run_options=*/{}, inputs,
                                    /*output_tensor_names=*/{"test_cancel:0"},
                                    /*target_tensor_names=*/{}, &outputs),
                StatusIs(absl::StatusCode::kCancelled));
  }

  {
    std::vector<tensorflow::Tensor> outputs;
    TF_ASSERT_OK(graph_executor->Run(/*run_options=*/{}, /*inputs=*/{},
                                     /*output_tensor_names=*/{"is_cancelled:0"},
                                     /*target_tensor_names=*/{}, &outputs));
    ASSERT_EQ(outputs.size(), 1);

    EXPECT_THAT(GetTfTensorData<bool>(outputs[0]),
                ::testing::ElementsAreArray({false}));
  }
}

INSTANTIATE_TEST_SUITE_P(GraphExecutorTestSuite, GraphExecutorTest,
                         ::testing::Bool());

TEST_F(GraphExecutorTest, DoOnlineCostAnalysisExactlyOnce) {
  GraphExecutor::LoadedClientGraph loaded_client_graph_0(
      "name0", /*symbol_uids=*/{},
      /*graph_executor=*/nullptr,
      /*mlir_context=*/nullptr,
      /*tf_mlir_with_op_keys=*/{}, /*tfrt_mlir=*/{},
      /*executable_context=*/nullptr,
      /*enable_online_cost_analysis=*/true,
      /*stream_callback_id=*/std::nullopt);
  GraphExecutor::LoadedClientGraph loaded_client_graph_1(
      "name1", /*symbol_uids=*/{},
      /*graph_executor=*/nullptr,
      /*mlir_context=*/nullptr,
      /*tf_mlir_with_op_keys=*/{}, /*tfrt_mlir=*/{},
      /*executable_context=*/nullptr,
      /*enable_online_cost_analysis=*/true,
      /*stream_callback_id=*/std::nullopt);

  // For each `LoadedClientGraph`, `MaybeCreateCostRecorder()` only returns a
  // cost recorder for once.
  EXPECT_TRUE(loaded_client_graph_0.MaybeCreateCostRecorder() != nullptr);
  EXPECT_TRUE(loaded_client_graph_1.MaybeCreateCostRecorder() != nullptr);
  EXPECT_TRUE(loaded_client_graph_0.MaybeCreateCostRecorder() == nullptr);
  EXPECT_TRUE(loaded_client_graph_1.MaybeCreateCostRecorder() == nullptr);
}

TEST_F(GraphExecutorTest, Extend) {
  GraphDef graph_def;
  {
    auto scope = tensorflow::Scope::NewRootScope().WithDevice("/device:CPU:0");

    Output a = ops::Const(scope.WithOpName("a"), 0.0f, {10, 10});

    Output b = ops::Const(scope.WithControlDependencies(a).WithOpName("b"),
                          0.0f, {10, 10});
    Output c = ops::Identity(scope.WithOpName("c"), b);

    TF_ASSERT_OK(scope.ToGraphDef(&graph_def));
  }

  auto runtime = DefaultTfrtRuntime(/*num_threads=*/1);
  GraphExecutor::Options options(runtime.get());
  auto session_options = CreateDefaultSessionOptions(options);
  // Disable optimizations for static graph to allow calls to Session::Extend.
  session_options.config.mutable_experimental()
      ->set_disable_optimize_for_static_graph(true);
  TF_ASSERT_OK_AND_ASSIGN(auto fallback_state,
                          tensorflow::tfrt_stub::FallbackState::Create(
                              session_options, graph_def.library()));
  auto resource_context = std::make_unique<tfrt::ResourceContext>();
  TF_ASSERT_OK_AND_ASSIGN(
      auto graph_executor,
      GraphExecutor::Create(std::move(options), *fallback_state,
                            std::move(resource_context), graph_def,
                            GetKernelRegistry()));

  GraphDef extension;
  {
    auto scope = tensorflow::Scope::NewRootScope().WithDevice("/device:CPU:0");

    auto input = ops::Placeholder(scope.WithOpName("input"), DT_INT32);
    auto rank = ops::Rank(scope.WithOpName("rank"), input);

    TF_ASSERT_OK(scope.ToGraphDef(&extension));
  }

  TF_ASSERT_OK(graph_executor->Extend(extension));

  // Set input 'x' to [[1, 1, 1]]
  std::vector<std::pair<std::string, tensorflow::Tensor>> inputs;
  inputs.push_back({"input", CreateTfTensor<int32_t>(
                                 /*shape=*/{1, 3}, /*data=*/{1, 1, 1})});

  std::vector<tensorflow::Tensor> outputs;

  TF_ASSERT_OK(graph_executor->Run(/*run_options=*/{}, inputs,
                                   /*output_tensor_names=*/{"rank"},
                                   /*target_tensor_names=*/{}, &outputs));
  ASSERT_EQ(outputs.size(), 1);

  EXPECT_THAT(GetTfTensorData<int32_t>(outputs[0]),
              ::testing::ElementsAreArray({2}));
}

TEST_F(GraphExecutorTest, DisableCompilation) {
  GraphDef graph_def;
  TF_ASSERT_OK(GetSimpleGraphDef(graph_def));

  auto runtime = DefaultTfrtRuntime(/*num_threads=*/1);
  GraphExecutor::Options options(runtime.get());
  TF_ASSERT_OK_AND_ASSIGN(
      auto fallback_state,
      tensorflow::tfrt_stub::FallbackState::Create(
          CreateDefaultSessionOptions(options), graph_def.library()));
  auto resource_context = std::make_unique<tfrt::ResourceContext>();
  TF_ASSERT_OK_AND_ASSIGN(
      auto graph_executor,
      GraphExecutor::Create(std::move(options), *fallback_state,
                            std::move(resource_context), graph_def,
                            GetKernelRegistry()));

  // Set input 'x' to [[1, 1, 1]]
  std::vector<std::pair<std::string, tensorflow::Tensor>> inputs;
  inputs.push_back({"input", CreateTfTensor<int32_t>(
                                 /*shape=*/{1, 3}, /*data=*/{1, 1, 1})});

  std::vector<tensorflow::Tensor> outputs;

  GraphExecutor::RunOptions run_options;
  run_options.disable_compilation = true;

  auto status = graph_executor->Run(run_options, inputs,
                                    /*output_tensor_names=*/{"rank"},
                                    /*target_tensor_names=*/{}, &outputs);
  ASSERT_FALSE(status.ok());
  EXPECT_THAT(
      status.ToString(),
      ::testing::HasSubstr("GraphExecutor: compilation is disabled in "
                           "execution but the compiled graph is not found"));

  run_options.disable_compilation = false;
  TF_ASSERT_OK(graph_executor->Run(run_options, inputs,
                                   /*output_tensor_names=*/{"rank"},
                                   /*target_tensor_names=*/{}, &outputs));
  ASSERT_EQ(outputs.size(), 1);

  EXPECT_THAT(GetTfTensorData<int32_t>(outputs[0]),
              ::testing::ElementsAreArray({2}));
}

TEST_F(GraphExecutorTest, SyncExecute) {
  GraphDef graph_def;
  TF_ASSERT_OK(GetSimpleGraphDef(graph_def));
  auto runtime = DefaultTfrtRuntime(/*num_threads=*/1);
  GraphExecutor::Options options(runtime.get());
  options.compile_options.compile_to_sync_tfrt_dialect = true;
  TF_ASSERT_OK_AND_ASSIGN(
      auto fallback_state,
      tensorflow::tfrt_stub::FallbackState::Create(
          CreateDefaultSessionOptions(options), graph_def.library()));

  auto resource_context = std::make_unique<tfrt::ResourceContext>();
  TF_ASSERT_OK_AND_ASSIGN(
      auto graph_executor,
      GraphExecutor::Create(std::move(options), *fallback_state,
                            std::move(resource_context), graph_def,
                            GetKernelRegistry()));

  std::vector<mlrt::Value> inputs;
  tfrt::DenseHostTensor dht =
      tfrt::CreateTensorFromValues<int32_t>({1, 3}, {1, 1, 1});
  inputs.emplace_back(std::move(dht));
  std::vector<mlrt::Value> results;
  results.resize(1);

  TF_ASSERT_OK(graph_executor->RunWithSyncInterpreter(
      "test_graph", absl::Span<mlrt::Value>(inputs),
      /*input_names=*/{"input"}, /*input_dtypes=*/{DT_INT32},
      /*output_tensor_names=*/{"rank"},
      /*target_tensor_names=*/{}, absl::Span<mlrt::Value>(results)));
  tfrt::DenseHostTensor expected =
      tfrt::CreateTensorFromValues<int32_t>({}, {2});

  EXPECT_EQ(expected, results[0].Get<tfrt::DenseHostTensor>());
}

}  // namespace
}  // namespace tfrt_stub
}  // namespace tensorflow
