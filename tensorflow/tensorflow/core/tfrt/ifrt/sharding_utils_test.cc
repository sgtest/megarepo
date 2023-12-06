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
#define EIGEN_USE_THREADS

#include "tensorflow/core/tfrt/ifrt/sharding_utils.h"

#include <cstdint>
#include <memory>
#include <utility>
#include <vector>

#include <gtest/gtest.h>
#include "absl/log/log.h"
#include "absl/types/span.h"
#include "unsupported/Eigen/CXX11/Tensor"  // from @eigen_archive
#include "llvm/ADT/SmallVector.h"
#include "xla/python/ifrt/array.h"
#include "xla/python/ifrt/client.h"
#include "xla/python/ifrt/device.h"
#include "xla/python/ifrt/ir/sharding_param.h"
#include "xla/python/ifrt/memory.h"
#include "xla/python/ifrt/shape.h"
#include "xla/python/ifrt/sharding.h"
#include "xla/python/ifrt/test_util.h"
#include "tensorflow/core/framework/tensor.h"
#include "tensorflow/core/framework/tensor_matcher.h"
#include "tensorflow/core/framework/tensor_shape.h"
#include "tensorflow/core/framework/tensor_testutil.h"
#include "tsl/lib/core/status_test_util.h"
#include "tsl/platform/env.h"
#include "tsl/platform/status_matchers.h"
#include "tsl/platform/statusor.h"
#include "tsl/platform/test.h"
#include "tsl/platform/threadpool.h"

namespace tensorflow {
namespace ifrt_serving {
namespace {

using tsl::testing::StatusIs;

struct ShardingUtilsTestParam {
  tensorflow::Tensor in_tensor;
  std::vector<tensorflow::Tensor> expected_out_tensors;
  std::vector<int> device_indices;

  // Parameter to form ShardingParam
  std::vector<int64_t> dim_shards;
  llvm::SmallVector<int, 4> permutation;
  llvm::SmallVector<int, 4> axis_sizes;
};

using ShardingUtilsTest = ::testing::TestWithParam<ShardingUtilsTestParam>;

TEST_P(ShardingUtilsTest, MakeAssembledArrayFromHostBuffer) {
  constexpr int kMaxParallelism = 16;
  auto thread_pool = std::make_unique<tsl::thread::ThreadPool>(
      tsl::Env::Default(), tsl::ThreadOptions(), "Resharding", kMaxParallelism);

  Eigen::ThreadPoolDevice device(thread_pool->AsEigenThreadPool(),
                                 kMaxParallelism);

  auto input_tensor = GetParam().in_tensor;

  // Create contexts required for the compiler execution.
  TF_ASSERT_OK_AND_ASSIGN(std::shared_ptr<xla::ifrt::Client> client,
                          xla::ifrt::test_util::GetClient());
  TF_ASSERT_OK_AND_ASSIGN(auto device_list,
                          xla::ifrt::test_util::GetDevices(
                              client.get(), GetParam().device_indices));

  xla::ifrt::ShardingParam sharding_param{
      GetParam().dim_shards,
      xla::ifrt::ShardingParam::MinorToMajor(GetParam().permutation,
                                             GetParam().axis_sizes)};

  TF_ASSERT_OK_AND_ASSIGN(
      auto sharding, xla::ifrt::ShardingParamSharding::Create(
                         sharding_param, device_list, xla::ifrt::MemoryKind()));

  TF_ASSERT_OK_AND_ASSIGN(
      auto assembled_array,
      MakeAssembledArrayFromHostBuffer(*client, input_tensor,
                                       std::move(sharding), device));

  TF_ASSERT_OK_AND_ASSIGN(auto disassembled_arrays,
                          assembled_array->DisassembleIntoSingleDeviceArrays(
                              xla::ifrt::ArrayCopySemantics::kAlwaysCopy));

  ASSERT_EQ(disassembled_arrays.size(), GetParam().expected_out_tensors.size());

  tensorflow::Tensor host_tensor(tensorflow::DT_INT32,
                                 tensorflow::TensorShape({1, 2}));

  for (int i = 0; i < disassembled_arrays.size(); ++i) {
    LOG(INFO) << "Verifying disassembled array " << i;
    auto disassembled_array = disassembled_arrays[i];
    auto expected_out_tensor = GetParam().expected_out_tensors[i];
    ASSERT_EQ(disassembled_array->shape(),
              xla::ifrt::Shape(expected_out_tensor.shape().dim_sizes()));
    tensorflow::Tensor host_tensor(expected_out_tensor.dtype(),
                                   expected_out_tensor.shape());
    TF_ASSERT_OK(
        disassembled_array
            ->CopyToHostBuffer(host_tensor.data(), /*byte_strides=*/{},
                               xla::ifrt::ArrayCopySemantics::kAlwaysCopy)
            .Await());
    EXPECT_THAT(expected_out_tensor, tensorflow::test::TensorEq(host_tensor));
  }
}

INSTANTIATE_TEST_SUITE_P(
    ShardingUtilsTests, ShardingUtilsTest,
    ::testing::ValuesIn<ShardingUtilsTestParam>(
        {
            {
                .in_tensor = test::AsTensor<int32_t>({1, 2, 3, 4},
                                                     TensorShape({2, 2})),

                .expected_out_tensors =
                    {
                        test::AsTensor<int32_t>({1, 2}, TensorShape({1, 2})),
                        test::AsTensor<int32_t>({3, 4}, TensorShape({1, 2})),
                    },
                .device_indices = {0, 1},
                .dim_shards = {2, 1},
                .permutation = {0, 1},
                .axis_sizes = {2, 1},
            },
            {
                .in_tensor = test::AsTensor<int32_t>({1, 2, 3, 4},
                                                     TensorShape({2, 2})),
                .expected_out_tensors =
                    {
                        test::AsTensor<int32_t>({1, 3}, TensorShape({2, 1})),
                        test::AsTensor<int32_t>({2, 4}, TensorShape({2, 1})),
                    },
                .device_indices = {0, 1},
                .dim_shards = {1, 2},
                .permutation = {0, 1},
                .axis_sizes = {1, 2},
            },
            {
                .in_tensor = test::AsTensor<int32_t>(
                    {1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16},
                    TensorShape({4, 4})),
                .expected_out_tensors =
                    {
                        test::AsTensor<int32_t>({1, 2, 5, 6},
                                                TensorShape({2, 2})),
                        test::AsTensor<int32_t>({3, 4, 7, 8},
                                                TensorShape({2, 2})),
                        test::AsTensor<int32_t>({9, 10, 13, 14},
                                                TensorShape({2, 2})),
                        test::AsTensor<int32_t>({11, 12, 15, 16},
                                                TensorShape({2, 2})),
                    },
                .device_indices = {0, 1, 2, 3},
                .dim_shards = {2, 2},
                .permutation = {0, 1},
                .axis_sizes = {2, 2},
            },
            {
                .in_tensor = test::AsTensor<int32_t>(
                    {1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16},
                    TensorShape({4, 4})),
                .expected_out_tensors =
                    {
                        test::AsTensor<int32_t>({1, 2, 3, 4, 5, 6, 7, 8},
                                                TensorShape({2, 4})),
                        test::AsTensor<int32_t>({9, 10, 11, 12, 13, 14, 15, 16},
                                                TensorShape({2, 4})),
                    },
                .device_indices = {0, 1},
                .dim_shards = {2, 1},
                .permutation = {1, 0},
                .axis_sizes = {2, 1},
            },
        }));

TEST(ShardingUtilsTest, MismatchRank) {
  constexpr int kMaxParallelism = 16;
  auto thread_pool = std::make_unique<tsl::thread::ThreadPool>(
      tsl::Env::Default(), tsl::ThreadOptions(), "Resharding", kMaxParallelism);

  Eigen::ThreadPoolDevice device(thread_pool->AsEigenThreadPool(),
                                 kMaxParallelism);

  auto input_tensor =
      test::AsTensor<int32_t>({1, 2, 3, 4}, TensorShape({2, 1, 2}));

  // Create contexts required for the compiler execution.
  TF_ASSERT_OK_AND_ASSIGN(std::shared_ptr<xla::ifrt::Client> client,
                          xla::ifrt::test_util::GetClient());
  TF_ASSERT_OK_AND_ASSIGN(
      auto device_list, xla::ifrt::test_util::GetDevices(client.get(), {0, 1}));

  xla::ifrt::ShardingParam sharding_param = {
      /*dim_shards=*/{2, 1},
      xla::ifrt::ShardingParam::MinorToMajor(/*permutation=*/{0, 1},
                                             /*axis_sizes=*/{2, 1})};

  TF_ASSERT_OK_AND_ASSIGN(
      auto sharding, xla::ifrt::ShardingParamSharding::Create(
                         sharding_param, device_list, xla::ifrt::MemoryKind()));

  EXPECT_THAT(MakeAssembledArrayFromHostBuffer(*client, input_tensor,
                                               std::move(sharding), device),
              StatusIs(absl::StatusCode::kInvalidArgument,
                       "Expect equal rank of 3 but got 2"));
}

}  // namespace
}  // namespace ifrt_serving
}  // namespace tensorflow
