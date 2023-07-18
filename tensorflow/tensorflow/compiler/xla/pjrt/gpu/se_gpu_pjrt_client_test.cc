/* Copyright 2022 The TensorFlow Authors. All Rights Reserved.

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

#include "tensorflow/compiler/xla/pjrt/gpu/se_gpu_pjrt_client.h"

#include <stdlib.h>

#include <array>
#include <memory>
#include <numeric>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include "absl/container/flat_hash_map.h"
#include "absl/strings/match.h"
#include "absl/strings/str_cat.h"
#include "absl/time/clock.h"
#include "absl/time/time.h"
#include "tensorflow/compiler/xla/pjrt/utils.h"
#include "tensorflow/compiler/xla/service/hlo_parser.h"
#include "tensorflow/compiler/xla/statusor.h"
#include "tensorflow/compiler/xla/test.h"
#include "tensorflow/compiler/xla/tests/literal_test_util.h"
#include "tensorflow/tsl/lib/core/status_test_util.h"
#include "tensorflow/tsl/platform/errors.h"
#include "tensorflow/tsl/platform/status.h"
#include "tensorflow/tsl/platform/status_matchers.h"
#include "tfrt/host_context/async_dispatch.h"  // from @tf_runtime
#include "tfrt/host_context/concurrent_work_queue.h"  // from @tf_runtime
#include "tfrt/host_context/diagnostic.h"  // from @tf_runtime
#include "tfrt/host_context/host_allocator.h"  // from @tf_runtime
#include "tfrt/host_context/host_context.h"  // from @tf_runtime

namespace xla {
namespace {

using ::testing::ElementsAre;
using ::testing::HasSubstr;
using ::tsl::testing::StatusIs;

StatusOr<std::unique_ptr<xla::PjRtLoadedExecutable>> CompileExecutable(
    absl::string_view program, xla::PjRtClient& client,
    xla::CompileOptions compile_options = xla::CompileOptions()) {
  TF_ASSIGN_OR_RETURN(auto hlo_module,
                      ParseAndReturnUnverifiedModule(program, {}));

  xla::XlaComputation xla_computation(hlo_module->ToProto());
  return client.Compile(xla_computation, compile_options);
}

// Given the result of a PjrtExecutable::Execute call (TF-status of vectors of
// vectors), extract the zeroth result from the zeroth device.
StatusOr<std::shared_ptr<xla::Literal>> ExtractSingleResult(
    xla::StatusOr<std::vector<std::vector<std::unique_ptr<xla::PjRtBuffer>>>>&
        result) {
  TF_RETURN_IF_ERROR(result.status());
  TF_RET_CHECK(result->size() == 1);
  std::vector<std::unique_ptr<xla::PjRtBuffer>>& result_buffers = (*result)[0];
  TF_RET_CHECK(result_buffers.size() == 1);
  auto literal_or = result_buffers[0]->ToLiteralSync();
  if (!literal_or.status().ok()) return literal_or.status();
  return *literal_or;
}

static constexpr char const* kProgram = R"(HloModule HostTransfer
    ENTRY SendRecvSynchronous() -> f32[2] {
      in_chain = token[] after-all()

      data = f32[2] constant({2, 3})
      send = (f32[2], u32[], token[]) send(data, in_chain),
        channel_id=1,
        is_host_transfer=true,
        frontend_attributes={
          _xla_host_transfer_handler_name="undef",
          _xla_host_transfer_original_type="f32",
          _xla_host_transfer_rendezvous="undef"
        }
      send-done = token[] send-done(send),
        channel_id=1, is_host_transfer=true

      recv = (f32[2], u32[], token[]) recv(send-done),
        channel_id=2,
        is_host_transfer=true,
        frontend_attributes={
          _xla_host_transfer_handler_name="undef",
          _xla_host_transfer_original_type="f32",
          _xla_host_transfer_rendezvous="undef"
        }
      recv-done = (f32[2], token[]) recv-done(recv),
        channel_id=2, is_host_transfer=true

      ROOT result = f32[2] get-tuple-element(recv-done), index=0
    })";

TEST(StreamExecutorGpuClientTest, SendRecvChunked) {
  TF_ASSERT_OK_AND_ASSIGN(
      auto client, GetStreamExecutorGpuClient(true, /*allocator_config=*/{},

                                              /*node_id=*/0));

  TF_ASSERT_OK_AND_ASSIGN(auto executable,
                          CompileExecutable(kProgram, *client));

  std::array<float, 2> sent_value = {0.0f, 0.0f};

  // Send buffer to host.
  SendCallback send_callback = {
      /*channel_id=*/1, [&](const PjRtTransferMetadata& m, PjRtChunk chunk,
                            int64_t total_size_in_bytes, bool done) {
        float* data = reinterpret_cast<float*>(chunk.data());
        sent_value[0] = data[0];
        sent_value[1] = data[1];
        return OkStatus();
      }};

  // Recv buffer from host.
  RecvCallback recv_callback = {
      /*channel_id=*/2, [&](const PjRtTransferMetadata& m,
                            std::unique_ptr<CopyToDeviceStream> stream) {
        auto chunk0 = PjRtChunk::AllocateDefault(sizeof(float));
        *reinterpret_cast<float*>(chunk0.data()) = 5.0f;
        TF_CHECK_OK(stream->AddChunk(std::move(chunk0)).Await());

        auto chunk1 = PjRtChunk::AllocateDefault(sizeof(float));
        *reinterpret_cast<float*>(chunk1.data()) = 6.0f;
        TF_CHECK_OK(stream->AddChunk(std::move(chunk1)).Await());

        return OkStatus();
      }};

  // Callbacks for point-to-point communication ops.
  std::vector<std::vector<SendCallback>> send_callbacks = {{send_callback}};
  std::vector<std::vector<RecvCallback>> recv_callbacks = {{recv_callback}};

  ExecuteOptions opts;
  opts.send_callbacks = send_callbacks;
  opts.recv_callbacks = recv_callbacks;

  auto result = executable->Execute(/*argument_handles=*/{{}}, opts);

  TF_ASSERT_OK_AND_ASSIGN(std::shared_ptr<xla::Literal> result_literal,
                          ExtractSingleResult(result));
  EXPECT_EQ(sent_value[0], 2.0f);
  EXPECT_EQ(sent_value[1], 3.0f);
  EXPECT_TRUE(LiteralTestUtil::Equal(LiteralUtil::CreateR1<float>({5.0f, 6.0f}),
                                     *result_literal));
}

TEST(StreamExecutorGpuClientTest, SendErrorNoDeadLock) {
  TF_ASSERT_OK_AND_ASSIGN(
      auto client, GetStreamExecutorGpuClient(true, /*allocator_config=*/{},
                                              /*node_id=*/0));

  TF_ASSERT_OK_AND_ASSIGN(auto executable,
                          CompileExecutable(kProgram, *client));

  // Always-failing Send handler.
  SendCallback send_callback = {
      /*channel_id=*/1,
      [&](const PjRtTransferMetadata&, PjRtChunk, int64_t, bool) {
        return InternalError("Uh-oh, can send chunk to host");
      }};

  // No-op Recv handler.
  RecvCallback recv_callback = {
      /*channel_id=*/2,
      [&](const PjRtTransferMetadata& m,
          std::unique_ptr<CopyToDeviceStream> stream) { return OkStatus(); }};

  // Callbacks for point-to-point communication ops.
  std::vector<std::vector<SendCallback>> send_callbacks = {{send_callback}};
  std::vector<std::vector<RecvCallback>> recv_callbacks = {{recv_callback}};

  ExecuteOptions opts;
  opts.send_callbacks = send_callbacks;
  opts.recv_callbacks = recv_callbacks;

  // Check that send error safely rejected and we do not dead lock.
  auto result = executable->Execute(/*argument_handles=*/{{}}, opts);
  EXPECT_TRUE(absl::StrContains(result.status().message(),
                                "Uh-oh, can send chunk to host"));
}

TEST(StreamExecutorGpuClientTest, RecvErrorNoDeadLock) {
  TF_ASSERT_OK_AND_ASSIGN(
      auto client, GetStreamExecutorGpuClient(true, /*allocator_config=*/{},
                                              /*node_id=*/0));

  TF_ASSERT_OK_AND_ASSIGN(auto executable,
                          CompileExecutable(kProgram, *client));

  // No-op Send handler.
  SendCallback send_callback = {
      /*channel_id=*/1, [&](const PjRtTransferMetadata&, PjRtChunk, int64_t,
                            bool) { return OkStatus(); }};

  // Invalid Recv handler that tries to add invalid chunk.
  RecvCallback recv_callback = {
      /*channel_id=*/2, [&](const PjRtTransferMetadata& m,
                            std::unique_ptr<CopyToDeviceStream> stream) {
        auto chunk = PjRtChunk::AllocateDefault(10 * sizeof(float));
        stream->AddChunk(std::move(chunk)).Await().IgnoreError();
        // Return ok status to proceed to corresponding recv-done call.
        return OkStatus();
      }};

  // Callbacks for point-to-point communication ops.
  std::vector<std::vector<SendCallback>> send_callbacks = {{send_callback}};
  std::vector<std::vector<RecvCallback>> recv_callbacks = {{recv_callback}};

  ExecuteOptions opts;
  opts.send_callbacks = send_callbacks;
  opts.recv_callbacks = recv_callbacks;

  // Check that invalid chunk safely rejected and we do not dead lock.
  auto result = executable->Execute(/*argument_handles=*/{{}}, opts);
  EXPECT_TRUE(absl::StrContains(result.status().message(),
                                "Adding chunk of size 40 would overflow buffer "
                                "of size 8 (0 already transferred)"));
}

TEST(StreamExecutorGpuClientTest, ToLiteralAsync) {
  TF_ASSERT_OK_AND_ASSIGN(
      auto client, GetStreamExecutorGpuClient(true, /*allocator_config=*/{},
                                              /*node_id=*/0));
  ASSERT_GE(client->addressable_devices().size(), 1);

  auto src_literal = LiteralUtil::CreateR1<float>({41.0f, 42.0f, 43.0f, 44.0f});
  TF_ASSERT_OK_AND_ASSIGN(
      auto transfer_manager,
      client->CreateBuffersForAsyncHostToDevice(
          {src_literal.shape()}, client->addressable_devices()[0]));
  auto buffer = transfer_manager->RetrieveBuffer(0);

  absl::Mutex mu;
  auto literal = std::make_shared<Literal>(
      ShapeUtil::DeviceShapeToHostShape(buffer->on_device_shape()));
  bool got_literal = false;

  TF_ASSERT_OK(
      transfer_manager->TransferLiteralToBuffer(0, src_literal, [&]() {}));

  buffer->ToLiteral(literal.get(), [&](Status s) {
    absl::MutexLock l(&mu);
    TF_ASSERT_OK(s);
    got_literal = true;
  });
  buffer.reset();

  {
    absl::MutexLock l(&mu);
    mu.Await(absl::Condition(&got_literal));
  }

  ASSERT_TRUE(ShapeUtil::Compatible(src_literal.shape(), literal->shape()));
  ASSERT_EQ(src_literal.data<float>(),
            literal->Relayout(src_literal.shape().layout()).data<float>());
}

TEST(StreamExecutorGpuClientTest, ToLiteralAsyncBeforeBufferReady) {
  TF_ASSERT_OK_AND_ASSIGN(
      auto client, GetStreamExecutorGpuClient(true, /*allocator_config=*/{},
                                              /*node_id=*/0));
  ASSERT_GE(client->addressable_devices().size(), 1);

  auto src_literal = LiteralUtil::CreateR1<float>({41.0f, 42.0f, 43.0f, 44.0f});
  TF_ASSERT_OK_AND_ASSIGN(
      auto transfer_manager,
      client->CreateBuffersForAsyncHostToDevice(
          {src_literal.shape()}, client->addressable_devices()[0]));
  auto buffer = transfer_manager->RetrieveBuffer(0);

  absl::Mutex mu;
  auto literal = std::make_shared<Literal>(
      ShapeUtil::DeviceShapeToHostShape(buffer->on_device_shape()));
  bool got_literal = false;

  buffer->ToLiteral(literal.get(), [&](Status s) {
    absl::MutexLock l(&mu);
    TF_ASSERT_OK(s);
    got_literal = true;
  });

  absl::SleepFor(absl::Milliseconds(10));
  ASSERT_FALSE(got_literal);
  TF_ASSERT_OK(
      transfer_manager->TransferLiteralToBuffer(0, src_literal, [&]() {}));

  buffer.reset();

  {
    absl::MutexLock l(&mu);
    mu.Await(absl::Condition(&got_literal));
  }

  ASSERT_TRUE(ShapeUtil::Compatible(src_literal.shape(), literal->shape()));
  ASSERT_EQ(src_literal.data<float>(),
            literal->Relayout(src_literal.shape().layout()).data<float>());
}

TEST(StreamExecutorGpuClientTest, FromHostAsync) {
  TF_ASSERT_OK_AND_ASSIGN(
      auto client, GetStreamExecutorGpuClient(true, /*allocator_config=*/{},
                                              /*node_id=*/0));
  ASSERT_GE(client->addressable_devices().size(), 1);

  std::vector<Literal> src_literals;
  std::vector<Shape> src_shapes;
  for (int i = 0; i < 4; ++i) {
    std::vector<float> data(i + 1);
    std::iota(data.begin(), data.end(), static_cast<float>(i + 10));
    src_literals.emplace_back(LiteralUtil::CreateR1<float>(data));
    src_shapes.push_back(src_literals.back().shape());
  }
  TF_ASSERT_OK_AND_ASSIGN(auto transfer_manager,
                          client->CreateBuffersForAsyncHostToDevice(
                              src_shapes, client->addressable_devices()[0]));
  std::vector<std::unique_ptr<PjRtBuffer>> buffers;
  for (int i = 0; i < src_shapes.size(); ++i) {
    buffers.emplace_back(transfer_manager->RetrieveBuffer(i));
  }

  absl::Mutex mu;
  std::vector<std::shared_ptr<Literal>> literals;
  int got_literal_count = 0;
  int got_callback_count = 0;

  for (int i = 0; i < src_shapes.size(); ++i) {
    TF_ASSERT_OK(transfer_manager->TransferRawDataToBuffer(
        i,
        absl::string_view(static_cast<char*>(src_literals[i].untyped_data()),
                          src_literals[i].size_bytes()),
        [&]() {}));
  }

  for (auto& buffer : buffers) {
    literals.push_back(std::make_shared<Literal>(
        ShapeUtil::DeviceShapeToHostShape(buffer->on_device_shape())));
    buffer->ToLiteral(literals.back().get(), [&](Status s) {
      absl::MutexLock l(&mu);
      TF_ASSERT_OK(s);
      ++got_literal_count;
    });
    buffer->OnReady([&](Status s) {
      absl::MutexLock l(&mu);
      TF_ASSERT_OK(s);
      ++got_callback_count;
    });
    buffer.reset();
  }

  {
    auto done = [&]() {
      return got_literal_count == src_literals.size() &&
             got_callback_count == src_literals.size();
    };
    absl::MutexLock l(&mu);
    mu.Await(absl::Condition(&done));
  }

  for (int i = 0; i < src_literals.size(); ++i) {
    ASSERT_TRUE(
        ShapeUtil::Compatible(src_literals[i].shape(), literals[i]->shape()));
    ASSERT_EQ(
        src_literals[i].data<float>(),
        literals[i]->Relayout(src_literals[i].shape().layout()).data<float>());
  }
}
TEST(StreamExecutorGpuClientTest, CopyRawToHostFullBuffer) {
  TF_ASSERT_OK_AND_ASSIGN(
      auto client, GetStreamExecutorGpuClient(true, /*allocator_config=*/{},
                                              /*node_id=*/0));
  auto literal = xla::LiteralUtil::CreateR1<float>({41.0f, 42.0f});
  TF_ASSERT_OK_AND_ASSIGN(
      std::unique_ptr<PjRtBuffer> buffer,
      client->BufferFromHostLiteral(literal, client->addressable_devices()[0]));

  void* dst = aligned_alloc(buffer->GetOnDeviceSizeInBytes().value(), 0);

  auto result =
      buffer->CopyRawToHost(dst, 0, buffer->GetOnDeviceSizeInBytes().value());
  TF_EXPECT_OK(result.Await());
  EXPECT_EQ(*(static_cast<float*>(dst)), 41.0f);
  EXPECT_EQ(*(static_cast<float*>(dst) + 1), 42.0f);

  free(dst);
}

TEST(StreamExecutorGpuClientTest, CopyRawToHostSubBuffer) {
  TF_ASSERT_OK_AND_ASSIGN(
      auto client, GetStreamExecutorGpuClient(true, /*allocator_config=*/{},
                                              /*node_id=*/0));
  auto literal = xla::LiteralUtil::CreateR1<float>({41.0f, 42.0f});

  TF_ASSERT_OK_AND_ASSIGN(
      std::unique_ptr<PjRtBuffer> buffer,
      client->BufferFromHostLiteral(literal, client->addressable_devices()[0]));
  void* dst = aligned_alloc(buffer->GetOnDeviceSizeInBytes().value(), 0);

  auto result = buffer->CopyRawToHost(dst, 0, sizeof(float));
  TF_EXPECT_OK(result.Await());
  EXPECT_EQ(*(static_cast<float*>(dst)), 41.0f);

  free(dst);
}

TEST(StreamExecutorGpuClientTest, CopyRawToHostOutOfRange) {
  TF_ASSERT_OK_AND_ASSIGN(
      auto client, GetStreamExecutorGpuClient(true, /*allocator_config=*/{},
                                              /*node_id=*/0));
  auto literal = xla::LiteralUtil::CreateR1<float>({41.0f, 42.0f});

  TF_ASSERT_OK_AND_ASSIGN(
      std::unique_ptr<PjRtBuffer> buffer,
      client->BufferFromHostLiteral(literal, client->addressable_devices()[0]));
  void* dst = aligned_alloc(buffer->GetOnDeviceSizeInBytes().value(), 0);

  auto result =
      buffer->CopyRawToHost(dst, 1, buffer->GetOnDeviceSizeInBytes().value());
  EXPECT_THAT(result.Await(), StatusIs(absl::StatusCode::kInvalidArgument,
                                       HasSubstr("invalid offset 1")));
  free(dst);
}

TEST(StreamExecutorGpuClientTest, AsyncCopyToDevice) {
  TF_ASSERT_OK_AND_ASSIGN(
      auto client, GetStreamExecutorGpuClient(true, /*allocator_config=*/{},
                                              /*node_id=*/0));
  ASSERT_GE(client->addressable_devices().size(), 2);

  // d0 is the device we will perform local/remote sends from.
  auto* d0 = client->addressable_devices()[0];
  // d1 is the device we will perform local/remote recvs, where the recv
  // sync flag may be contended.
  auto* d1 = client->addressable_devices()[1];

  auto src_literal = LiteralUtil::CreateR1<float>({41.0f, 42.0f, 43.0f, 44.0f});
  TF_ASSERT_OK_AND_ASSIGN(
      auto transfer_manager,
      client->CreateBuffersForAsyncHostToDevice({src_literal.shape()}, d0));
  auto src_buffer = transfer_manager->RetrieveBuffer(0);
  // CopyToDevice won't be enqueued until src_buffer is available.
  auto local_recv_buffer = *src_buffer->CopyToDevice(d1);

  TF_ASSERT_OK(
      transfer_manager->TransferLiteralToBuffer(0, src_literal, []() {}));

  auto literal = std::make_shared<Literal>(src_literal.shape());

  auto local_recv_literal = local_recv_buffer->ToLiteral(literal.get());
  TF_EXPECT_OK(local_recv_literal.Await());

  ASSERT_TRUE(ShapeUtil::Compatible(src_literal.shape(), literal->shape()));
  ASSERT_EQ(src_literal.data<float>(),
            literal->Relayout(src_literal.shape().layout()).data<float>());
}

TEST(StreamExecutorGpuClientTest, CreateMixOfErrorBuffers) {
  TF_ASSERT_OK_AND_ASSIGN(
      auto client, GetStreamExecutorGpuClient(true, /*allocator_config=*/{},
                                              /*node_id=*/0));
  ASSERT_GE(client->addressable_devices().size(), 1);

  std::vector<Literal> src_literals;
  std::vector<Shape> src_shapes;
  for (int i = 0; i < 4; ++i) {
    std::vector<float> data(i + 1);
    std::iota(data.begin(), data.end(), static_cast<float>(i + 10));
    src_literals.emplace_back(LiteralUtil::CreateR1<float>(data));
    src_shapes.push_back(src_literals.back().shape());
  }
  ASSERT_OK_AND_ASSIGN(auto transfer_manager,
                       client->CreateBuffersForAsyncHostToDevice(
                           src_shapes, client->addressable_devices()[0]));
  std::vector<std::unique_ptr<PjRtBuffer>> buffers;
  for (int i = 0; i < src_shapes.size(); ++i) {
    buffers.emplace_back(transfer_manager->RetrieveBuffer(i));
  }

  absl::Mutex mu;
  int got_callback_count = 0;
  for (int i = 0; i < 4; ++i) {
    auto& buffer = buffers[i];
    if (i == 0 || i == 3) {
      ASSERT_OK(transfer_manager->TransferLiteralToBuffer(i, src_literals[i],
                                                          [&]() {}));
      buffer->OnReady([&](absl::Status s) {
        absl::MutexLock l(&mu);
        ASSERT_OK(s);
        ++got_callback_count;
      });
    } else {
      absl::Status error = InternalError("error %d", i);
      transfer_manager->SetBufferError(i, error);
      buffer->OnReady([error, &mu, &got_callback_count](absl::Status s) {
        absl::MutexLock l(&mu);
        ASSERT_EQ(s, error);
        ++got_callback_count;
      });
    }
    buffer.reset();
  }

  {
    auto done = [&]() { return got_callback_count == src_literals.size(); };
    absl::MutexLock l(&mu);
    QCHECK(mu.AwaitWithTimeout(absl::Condition(&done), absl::Seconds(60)));
  }
}

TEST(GpuTopology, FromProto) {
  GpuTopologyProto msg;
  ASSERT_TRUE(tsl::protobuf::TextFormat::ParseFromString(
      R"pb(
        device_ids: [ 3, 2, 1 ]
      )pb",
      &msg));

  std::unique_ptr<const GpuTopology> gpu_topology = GpuTopology::FromProto(msg);
  EXPECT_THAT(gpu_topology->device_ids(), ElementsAre(3, 2, 1));
}

TEST(GpuTopology, ToProto) {
  GpuTopology gpu_topology({3, 2, 1});
  GpuTopologyProto msg = gpu_topology.ToProto();
  EXPECT_THAT(msg.device_ids(), ElementsAre(3, 2, 1));
}

TEST(StreamExecutorGpuClientTest, DistributeInit) {
  absl::flat_hash_map<std::string, std::string> kv_store;
  absl::Mutex mu;
  PjRtClient::KeyValueGetCallback kv_get =
      [&kv_store, &mu](const std::string& k,
                       absl::Duration timeout) -> xla::StatusOr<std::string> {
    absl::Duration wait_interval = absl::Milliseconds(10);
    int num_retry = timeout / wait_interval;
    for (int i = 0; i < num_retry; i++) {
      {
        absl::MutexLock lock(&mu);
        auto iter = kv_store.find(k);
        if (iter != kv_store.end()) {
          return iter->second;
        }
      }
      absl::SleepFor(wait_interval);
    }
    return absl::NotFoundError(
        absl::StrCat(k, " is not found in the kv store."));
  };
  PjRtClient::KeyValuePutCallback kv_put =
      [&kv_store, &mu](const std::string& k,
                       const std::string& v) -> xla::Status {
    {
      absl::MutexLock lock(&mu);
      kv_store[k] = v;
    }
    return tsl::OkStatus();
  };
  auto host_context = std::make_unique<tfrt::HostContext>(
      [](const tfrt::DecodedDiagnostic& diag) {
        LOG(ERROR) << "Encountered runtime error: " << diag.message() << "\n";
      },
      tfrt::CreateMallocAllocator(),
      tfrt::CreateMultiThreadedWorkQueue(
          /*num_threads=*/DefaultThreadPoolSize(),
          /*num_blocking_threads=*/4));
  int num_nodes = 2;
  for (int i = 0; i < num_nodes; i++) {
    tfrt::EnqueueWork(host_context.get(), [&kv_get, &kv_put, i, num_nodes] {
      TF_ASSERT_OK_AND_ASSIGN(
          auto client,
          GetStreamExecutorGpuClient(
              true, /*allocator_config=*/{},
              /*node_id=*/i, num_nodes, /*allowed_devices=*/std::nullopt,
              /*platform_name=*/std::nullopt,
              /*should_stage_host_to_device_transfers=*/true, kv_get, kv_put));
      EXPECT_EQ(client->platform_name(), "gpu");
      EXPECT_EQ(client->addressable_device_count(), 2);
      EXPECT_EQ(client->device_count(), 4);
    });
  }
}

}  // namespace
}  // namespace xla
