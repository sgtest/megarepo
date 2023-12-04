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

#include "xla/service/gpu/runtime3/command_buffer_thunk.h"

#include <algorithm>
#include <cstdint>
#include <memory>
#include <optional>
#include <utility>
#include <vector>

#include "xla/service/buffer_assignment.h"
#include "xla/service/gpu/buffer_allocations.h"
#include "xla/service/gpu/launch_dimensions.h"
#include "xla/service/gpu/matmul_utils.h"
#include "xla/service/gpu/runtime3/command_buffer_allocations.h"
#include "xla/service/gpu/runtime3/command_buffer_cmd.h"
#include "xla/service/gpu/thunk.h"
#include "xla/service/service_executable_run_options.h"
#include "xla/shape_util.h"
#include "xla/stream_executor/blas.h"
#include "xla/stream_executor/command_buffer.h"
#include "xla/stream_executor/cuda/cuda_test_kernels.h"
#include "xla/stream_executor/device_memory.h"
#include "xla/stream_executor/multi_platform_manager.h"
#include "xla/stream_executor/platform.h"
#include "xla/stream_executor/stream_executor.h"
#include "xla/types.h"  // IWYU pragma: keep
#include "tsl/lib/core/status_test_util.h"
#include "tsl/platform/test.h"

namespace xla::gpu {

static se::StreamExecutor* CudaExecutor() {
  auto* platform = se::MultiPlatformManager::PlatformWithName("CUDA").value();
  return platform->ExecutorForDevice(0).value();
}

TEST(CommandBufferThunkTest, MemcpyCmd) {
  se::StreamExecutor* executor = CudaExecutor();

  se::Stream stream(executor);
  stream.Init();
  ASSERT_TRUE(stream.ok());

  int64_t length = 4;
  int64_t byte_length = sizeof(int32_t) * length;

  // Prepare arguments: a=42, b=0
  se::DeviceMemory<int32_t> a = executor->AllocateArray<int32_t>(length, 0);
  se::DeviceMemory<int32_t> b = executor->AllocateArray<int32_t>(length, 0);

  stream.ThenMemset32(&a, 42, byte_length);
  stream.ThenMemZero(&b, byte_length);

  // Prepare buffer allocations for recording command buffer.
  BufferAllocation alloc_a(/*index=*/0, byte_length, /*color=*/0);
  BufferAllocation alloc_b(/*index=*/1, byte_length, /*color=*/0);

  BufferAllocation::Slice slice_a(&alloc_a, 0, byte_length);
  BufferAllocation::Slice slice_b(&alloc_b, 0, byte_length);

  // Prepare commands sequence for constructing command buffer.
  CommandBufferCmdSequence commands;
  commands.Emplace<MemcpyDeviceToDeviceCmd>(slice_b, slice_a, byte_length);

  // Construct a thunk with command sequence.
  CommandBufferThunk thunk(std::move(commands), Thunk::ThunkInfo(nullptr));

  ServiceExecutableRunOptions run_options;
  BufferAllocations allocations({a, b}, 0, executor->GetAllocator());
  Thunk::ExecuteParams params(run_options, allocations, &stream, {});

  // Execute command buffer thunk and verify that it copied the memory.
  TF_ASSERT_OK(thunk.ExecuteOnStream(params));
  TF_ASSERT_OK(stream.BlockHostUntilDone());

  // Copy `b` data back to host.
  std::vector<int32_t> dst(4, 0);
  stream.ThenMemcpy(dst.data(), b, byte_length);

  ASSERT_EQ(dst, std::vector<int32_t>(4, 42));

  // Try to update the command buffer with the same buffers.
  stream.ThenMemZero(&b, byte_length);

  // Thunk execution should automatically update underlying command buffer.
  TF_ASSERT_OK(thunk.ExecuteOnStream(params));
  TF_ASSERT_OK(stream.BlockHostUntilDone());

  // Copy `b` data back to host.
  std::fill(dst.begin(), dst.end(), 0);
  stream.ThenMemcpy(dst.data(), b, byte_length);

  ASSERT_EQ(dst, std::vector<int32_t>(4, 42));
}

// This test does the following operations:
// 1. Allocates memory region "a" and "c" outside command buffer.
// 2. Allocates memory region "b" inside command buffer.
// 3. MemCopyDeviceToDevice from "a" to "b" inside command buffer.

// 4. MemCopyDeviceToDevice from "b" to "c" inside command buffer.
// 5. Free memory region "b" inside command buffer.
// 6. Verify that region "c" has the same content as "a".
TEST(CommandBufferThunkTest, MemallocFreeCmdSameThunk) {
  se::StreamExecutor* executor = CudaExecutor();

  se::Stream stream(executor);
  stream.Init();
  ASSERT_TRUE(stream.ok());

  // Prepare arguments:
  int64_t length = 4;
  int64_t byte_length = sizeof(int32_t) * length;

  BufferAllocation alloc_a(/*index=*/0, byte_length, /*color=*/0);
  BufferAllocation alloc_b(/*index=*/1, byte_length, /*color=*/0);
  BufferAllocation alloc_c(/*index=*/2, byte_length, /*color=*/0);
  BufferAllocation::Slice slice_a(&alloc_a, 0, byte_length);
  BufferAllocation::Slice slice_b(&alloc_b, 0, byte_length);
  BufferAllocation::Slice slice_c(&alloc_c, 0, byte_length);

  // Prepare commands sequence for constructing command buffer.
  CommandBufferCmdSequence commands;
  commands.Emplace<AllocateCmd>(alloc_b);
  commands.Emplace<MemcpyDeviceToDeviceCmd>(slice_b, slice_a, byte_length);
  commands.Emplace<MemcpyDeviceToDeviceCmd>(slice_c, slice_b, byte_length);
  commands.Emplace<FreeCmd>(alloc_b);

  // Construct a thunk with command sequence.
  CommandBufferThunk thunk(std::move(commands), Thunk::ThunkInfo(nullptr));

  // Prepare arguments: a=42, b=0
  se::DeviceMemory<int32_t> a = executor->AllocateArray<int32_t>(length, 0);
  stream.ThenMemset32(&a, 42, byte_length);

  se::DeviceMemory<int32_t> b(se::DeviceMemoryBase(
      reinterpret_cast<int32_t*>(BufferAllocations::kExternalAllocationMarker),
      byte_length));
  se::DeviceMemory<int32_t> c = executor->AllocateArray<int32_t>(length, 0);

  std::unique_ptr<CommandBufferAllocations> external_allocation =
      std::make_unique<CommandBufferAllocations>();

  BufferAllocations allocations({a, b, c}, 0, executor->GetAllocator(),
                                external_allocation.get());

  ServiceExecutableRunOptions run_options;
  Thunk::ExecuteParams params(run_options, allocations, &stream, {});

  // Execute command buffer thunk and verify that it copied the memory.
  TF_ASSERT_OK(thunk.ExecuteOnStream(params));
  TF_ASSERT_OK(stream.BlockHostUntilDone());

  // Copy `b` data back to host.
  std::vector<int32_t> dst(4, 0);
  stream.ThenMemcpy(dst.data(), allocations.GetMutableDeviceAddress(2),
                    byte_length);

  ASSERT_EQ(dst, std::vector<int32_t>(4, 42));
}

// This test does the following operations:
// 1. Allocates memory region "a" and "c" outside command buffer.
// 2. Allocates memory region "b" inside command buffer thunk 1.
// 3. MemCopyDeviceToDevice from "a" to "b" inside command buffer 1.
// 4. MemCopyDeviceToDevice from "b" to "c" inside command buffer 2.
// 5. Free memory region "b" inside command buffer 2.
// 6. Verify that region "c" has the same content as "a".
TEST(CommandBufferThunkTest, MemallocFreeCmdAcrossThunk) {
  se::StreamExecutor* executor = CudaExecutor();

  se::Stream stream(executor);
  stream.Init();
  ASSERT_TRUE(stream.ok());

  // Prepare arguments:
  int64_t length = 4;
  int64_t byte_length = sizeof(int32_t) * length;

  BufferAllocation alloc_a(/*index=*/0, byte_length, /*color=*/0);
  BufferAllocation alloc_b(/*index=*/1, byte_length, /*color=*/0);
  BufferAllocation alloc_c(/*index=*/2, byte_length, /*color=*/0);
  BufferAllocation::Slice slice_a(&alloc_a, 0, byte_length);
  BufferAllocation::Slice slice_b(&alloc_b, 0, byte_length);
  BufferAllocation::Slice slice_c(&alloc_c, 0, byte_length);

  // =================Thunk 1=================================
  // Prepare commands sequence for constructing command buffer.
  CommandBufferCmdSequence commands1;
  commands1.Emplace<AllocateCmd>(alloc_b);
  commands1.Emplace<MemcpyDeviceToDeviceCmd>(slice_b, slice_a, byte_length);

  // Construct a thunk with command sequence.
  CommandBufferThunk thunk1(std::move(commands1), Thunk::ThunkInfo(nullptr));

  // Prepare arguments: a=42, b=0
  se::DeviceMemory<int32_t> a = executor->AllocateArray<int32_t>(length, 0);
  stream.ThenMemset32(&a, 42, byte_length);
  se::DeviceMemory<int32_t> b(se::DeviceMemoryBase(
      reinterpret_cast<int32_t*>(BufferAllocations::kExternalAllocationMarker),
      byte_length));
  se::DeviceMemory<int32_t> c = executor->AllocateArray<int32_t>(length, 0);

  std::unique_ptr<CommandBufferAllocations> external_allocation =
      std::make_unique<CommandBufferAllocations>();

  BufferAllocations allocations({a, b, c}, 0, executor->GetAllocator(),
                                external_allocation.get());

  ServiceExecutableRunOptions run_options;
  Thunk::ExecuteParams params(run_options, allocations, &stream, {});

  // Execute command buffer thunk and verify that it copied the memory.
  TF_ASSERT_OK(thunk1.ExecuteOnStream(params));

  // =================Thunk 2=================================
  CommandBufferCmdSequence commands2;
  commands2.Emplace<MemcpyDeviceToDeviceCmd>(slice_c, slice_b, byte_length);
  commands2.Emplace<FreeCmd>(alloc_b);

  // Construct a thunk with command sequence.
  CommandBufferThunk thunk2(std::move(commands2), Thunk::ThunkInfo(nullptr));

  // Execute command buffer thunk and verify that it copied the memory.
  TF_ASSERT_OK(thunk2.ExecuteOnStream(params));

  // Copy `c` data back to host.
  std::vector<int32_t> dst(4, 0);
  stream.ThenMemcpy(dst.data(), allocations.GetMutableDeviceAddress(2),
                    byte_length);

  ASSERT_EQ(dst, std::vector<int32_t>(4, 42));
}

TEST(CommandBufferThunkTest, LaunchCmd) {
  se::StreamExecutor* executor = CudaExecutor();

  se::Stream stream(executor);
  stream.Init();
  ASSERT_TRUE(stream.ok());

  int64_t length = 4;
  int64_t byte_length = sizeof(int32_t) * length;

  // Prepare arguments: a=42, b=0
  se::DeviceMemory<int32_t> a = executor->AllocateArray<int32_t>(length, 0);
  se::DeviceMemory<int32_t> b = executor->AllocateArray<int32_t>(length, 0);

  stream.ThenMemset32(&a, 42, byte_length);
  stream.ThenMemZero(&b, byte_length);

  // Prepare buffer allocations for recording command buffer.
  BufferAllocation alloc_a(/*index=*/0, byte_length, /*color=*/0);
  BufferAllocation alloc_b(/*index=*/1, byte_length, /*color=*/0);

  BufferAllocation::Slice slice_a(&alloc_a, 0, byte_length);
  BufferAllocation::Slice slice_b(&alloc_b, 0, byte_length);

  auto args = {slice_a, slice_a, slice_b};  // b = a + a

  // Prepare commands sequence for constructing command buffer.
  CommandBufferCmdSequence commands;
  commands.Emplace<LaunchCmd>("add", args, LaunchDimensions(1, 4),
                              /*shmem_bytes=*/0);

  // Construct a thunk with command sequence.
  CommandBufferThunk thunk(std::move(commands), Thunk::ThunkInfo(nullptr));

  ServiceExecutableRunOptions run_options;
  BufferAllocations allocations({a, b}, 0, executor->GetAllocator());
  Thunk::ExecuteParams params(run_options, allocations, &stream, {});

  CommandBufferCmd::ExecutableSource source = {
      /*text=*/se::cuda::internal::kAddI32Kernel, /*binary=*/{}};
  TF_ASSERT_OK(thunk.Initialize(executor, source));

  // Execute command buffer thunk and verify that it added the value.
  TF_ASSERT_OK(thunk.ExecuteOnStream(params));
  TF_ASSERT_OK(stream.BlockHostUntilDone());

  // Copy `b` data back to host.
  std::vector<int32_t> dst(4, 0);
  stream.ThenMemcpy(dst.data(), b, byte_length);

  ASSERT_EQ(dst, std::vector<int32_t>(4, 42 + 42));

  // Prepare buffer allocation for updating command buffer: c=0
  se::DeviceMemory<int32_t> c = executor->AllocateArray<int32_t>(length, 0);
  stream.ThenMemZero(&c, byte_length);

  // Update buffer allocation #1 to buffer `c`.
  allocations = BufferAllocations({a, c}, 0, executor->GetAllocator());

  // Thunk execution should automatically update underlying command buffer.
  TF_ASSERT_OK(thunk.ExecuteOnStream(params));
  TF_ASSERT_OK(stream.BlockHostUntilDone());

  // Copy `c` data back to host.
  std::fill(dst.begin(), dst.end(), 0);
  stream.ThenMemcpy(dst.data(), c, byte_length);

  ASSERT_EQ(dst, std::vector<int32_t>(4, 42 + 42));

  // Try to update the command buffer with the same buffers.
  stream.ThenMemZero(&c, byte_length);

  // Thunk execution should automatically update underlying command buffer.
  TF_ASSERT_OK(thunk.ExecuteOnStream(params));
  TF_ASSERT_OK(stream.BlockHostUntilDone());

  // Copy `c` data back to host.
  std::fill(dst.begin(), dst.end(), 0);
  stream.ThenMemcpy(dst.data(), c, byte_length);

  ASSERT_EQ(dst, std::vector<int32_t>(4, 42 + 42));
}

TEST(CommandBufferThunkTest, GemmCmd) {
  se::StreamExecutor* executor = CudaExecutor();

  se::Stream stream(executor);
  stream.Init();
  ASSERT_TRUE(stream.ok());

  int64_t lhs_length = sizeof(float) * 2 * 4;
  int64_t rhs_length = sizeof(float) * 4 * 3;
  int64_t out_length = sizeof(float) * 2 * 3;

  // Prepare arguments:
  // lhs = [1.0, 2.0, 3.0, 4.0
  //        5.0, 6.0, 7.0, 8.0]
  // rhs = [1.0, 1.0, 1.0
  //        1.0, 1.0, 1.0
  //        1.0, 1.0, 1.0
  //        1.0, 1.0, 1.0]
  se::DeviceMemory<float> lhs = executor->AllocateArray<float>(2 * 4);
  std::vector<float> lhs_arr{1, 2, 3, 4, 5, 6, 7, 8};
  stream.ThenMemcpy(&lhs, lhs_arr.data(), lhs_length);

  se::DeviceMemory<float> rhs = executor->AllocateArray<float>(4 * 3);
  std::vector<float> rhs_arr(12, 1);
  stream.ThenMemcpy(&rhs, rhs_arr.data(), rhs_length);

  se::DeviceMemory<float> out = executor->AllocateArray<float>(2 * 3);
  stream.ThenMemZero(&out, out_length);

  // Prepare buffer allocations for recording command buffer.
  BufferAllocation alloc_lhs(/*index=*/0, lhs_length, /*color=*/0);
  BufferAllocation alloc_rhs(/*index=*/1, rhs_length, /*color=*/0);
  BufferAllocation alloc_out(/*index=*/2, out_length, /*color=*/0);

  BufferAllocation::Slice slice_lhs(&alloc_lhs, 0, lhs_length);
  BufferAllocation::Slice slice_rhs(&alloc_rhs, 0, rhs_length);
  BufferAllocation::Slice slice_out(&alloc_out, 0, out_length);

  auto config = GemmConfig::For(
      ShapeUtil::MakeShape(PrimitiveType::F32, {2, 4}), {}, {1},
      ShapeUtil::MakeShape(PrimitiveType::F32, {4, 3}), {}, {0},
      ShapeUtil::MakeShape(PrimitiveType::F32, {2, 3}), 1.0, 0.0, 0.0,
      std::nullopt, se::blas::kDefaultComputePrecision, false, false);
  ASSERT_TRUE(config.ok());

  // Prepare commands sequence for constructing command buffer.
  CommandBufferCmdSequence commands;
  commands.Emplace<GemmCmd>(config.value(), slice_lhs, slice_rhs, slice_out,
                            /*deterministic=*/true);

  // Construct a thunk with command sequence.
  CommandBufferThunk thunk(std::move(commands), Thunk::ThunkInfo(nullptr));

  ServiceExecutableRunOptions run_options;
  BufferAllocations allocations({lhs, rhs, out}, 0, executor->GetAllocator());
  Thunk::ExecuteParams params(run_options, allocations, &stream, {});

  CommandBufferCmd::ExecutableSource source = {/*text=*/"", /*binary=*/{}};
  TF_ASSERT_OK(thunk.Initialize(executor, source));

  // Execute command buffer thunk and verify that it executed a GEMM.
  TF_ASSERT_OK(thunk.ExecuteOnStream(params));
  TF_ASSERT_OK(stream.BlockHostUntilDone());

  // Copy `out` data back to host.
  std::vector<float> dst(6, 0);
  stream.ThenMemcpy(dst.data(), out, out_length);

  ASSERT_EQ(dst, std::vector<float>({10, 10, 10, 26, 26, 26}));

  // Prepare buffer allocation for updating command buffer.
  se::DeviceMemory<float> updated_out = executor->AllocateArray<float>(2 * 3);
  stream.ThenMemZero(&updated_out, out_length);

  // Update buffer allocation to updated `out` buffer.
  allocations =
      BufferAllocations({lhs, rhs, updated_out}, 0, executor->GetAllocator());

  // Thunk execution should automatically update underlying command buffer.
  TF_ASSERT_OK(thunk.ExecuteOnStream(params));
  TF_ASSERT_OK(stream.BlockHostUntilDone());

  // Copy `updated_out` data back to host.
  std::fill(dst.begin(), dst.end(), 0);
  stream.ThenMemcpy(dst.data(), updated_out, out_length);

  ASSERT_EQ(dst, std::vector<float>({10, 10, 10, 26, 26, 26}));

  // Try to update the command buffer with the same buffers.
  stream.ThenMemZero(&updated_out, out_length);

  // Thunk execution should automatically update underlying command buffer.
  TF_ASSERT_OK(thunk.ExecuteOnStream(params));
  TF_ASSERT_OK(stream.BlockHostUntilDone());

  // Copy `updated_out` data back to host.
  std::fill(dst.begin(), dst.end(), 0);
  stream.ThenMemcpy(dst.data(), updated_out, out_length);

  ASSERT_EQ(dst, std::vector<float>({10, 10, 10, 26, 26, 26}));
}

TEST(CommandBufferThunkTest, MultipleLaunchCmd) {
  se::StreamExecutor* executor = CudaExecutor();

  se::Stream stream(executor);
  stream.Init();
  ASSERT_TRUE(stream.ok());

  int64_t length = 4;
  int64_t byte_length = sizeof(int32_t) * length;

  // Prepare arguments: a=42, b=0
  se::DeviceMemory<int32_t> a = executor->AllocateArray<int32_t>(length, 0);
  se::DeviceMemory<int32_t> b = executor->AllocateArray<int32_t>(length, 0);
  se::DeviceMemory<int32_t> c = executor->AllocateArray<int32_t>(length, 0);
  se::DeviceMemory<int32_t> d = executor->AllocateArray<int32_t>(length, 0);

  stream.ThenMemset32(&a, 42, byte_length);
  stream.ThenMemZero(&b, byte_length);
  stream.ThenMemset32(&c, 21, byte_length);
  stream.ThenMemZero(&d, byte_length);

  // Prepare buffer allocations for recording command buffer.
  BufferAllocation alloc_a(/*index=*/0, byte_length, /*color=*/0);
  BufferAllocation alloc_b(/*index=*/1, byte_length, /*color=*/0);
  BufferAllocation alloc_c(/*index=*/2, byte_length, /*color=*/0);
  BufferAllocation alloc_d(/*index=*/3, byte_length, /*color=*/0);

  BufferAllocation::Slice slice_a(&alloc_a, 0, byte_length);
  BufferAllocation::Slice slice_b(&alloc_b, 0, byte_length);
  BufferAllocation::Slice slice_c(&alloc_c, 0, byte_length);
  BufferAllocation::Slice slice_d(&alloc_d, 0, byte_length);

  auto args = {slice_a, slice_a, slice_b};    // b = a + a
  auto args_1 = {slice_c, slice_c, slice_d};  // d = c + c

  // Prepare commands sequence for constructing command buffer.
  CommandBufferCmdSequence commands;
  commands.Emplace<LaunchCmd>("add", args, LaunchDimensions(1, 4),
                              /*shmem_bytes=*/0);
  commands.Emplace<LaunchCmd>("add", args_1, LaunchDimensions(1, 4),
                              /*shmem_bytes=*/0);

  // Construct a thunk with command sequence.
  CommandBufferThunk thunk(std::move(commands), Thunk::ThunkInfo(nullptr));

  ServiceExecutableRunOptions run_options;
  BufferAllocations allocations({a, b, c, d}, 0, executor->GetAllocator());
  Thunk::ExecuteParams params(run_options, allocations, &stream, {});

  CommandBufferCmd::ExecutableSource source = {
      /*text=*/se::cuda::internal::kAddI32Kernel, /*binary=*/{}};
  TF_ASSERT_OK(thunk.Initialize(executor, source));

  // Execute command buffer thunk and verify that it added the value.
  TF_ASSERT_OK(thunk.ExecuteOnStream(params));
  TF_ASSERT_OK(stream.BlockHostUntilDone());

  // Copy `b` data back to host.
  std::vector<int32_t> dst(4, 0);
  stream.ThenMemcpy(dst.data(), b, byte_length);
  ASSERT_EQ(dst, std::vector<int32_t>(4, 42 + 42));

  // Copy `d` data back to host.
  std::fill(dst.begin(), dst.end(), 0);
  stream.ThenMemcpy(dst.data(), d, byte_length);
  ASSERT_EQ(dst, std::vector<int32_t>(4, 21 + 21));

  BufferAllocation alloc_e(/*index=*/3, byte_length, /*color=*/0);
  BufferAllocation::Slice slice_e(&alloc_e, 0, byte_length);

  // Prepare buffer allocation for updating command buffer: e=0
  se::DeviceMemory<int32_t> e = executor->AllocateArray<int32_t>(length, 0);
  stream.ThenMemZero(&e, byte_length);

  // Update buffer allocation #1 to buffer `c`.
  allocations = BufferAllocations({a, b, c, e}, 0, executor->GetAllocator());

  // Thunk execution should automatically update underlying command buffer.
  TF_ASSERT_OK(thunk.ExecuteOnStream(params));
  TF_ASSERT_OK(stream.BlockHostUntilDone());

  // Copy `b` data back to host.
  std::fill(dst.begin(), dst.end(), 0);
  stream.ThenMemcpy(dst.data(), b, byte_length);
  ASSERT_EQ(dst, std::vector<int32_t>(4, 42 + 42));

  // Copy `e` data back to host.
  std::fill(dst.begin(), dst.end(), 0);
  stream.ThenMemcpy(dst.data(), e, byte_length);
  ASSERT_EQ(dst, std::vector<int32_t>(4, 21 + 21));

  // Try to update the command buffer with the same buffers.
  stream.ThenMemZero(&e, byte_length);

  // Thunk execution should automatically update underlying command buffer.
  TF_ASSERT_OK(thunk.ExecuteOnStream(params));
  TF_ASSERT_OK(stream.BlockHostUntilDone());

  // Copy `b` data back to host.
  std::fill(dst.begin(), dst.end(), 0);
  stream.ThenMemcpy(dst.data(), b, byte_length);
  ASSERT_EQ(dst, std::vector<int32_t>(4, 42 + 42));

  // Copy `e` data back to host.
  std::fill(dst.begin(), dst.end(), 0);
  stream.ThenMemcpy(dst.data(), e, byte_length);
  ASSERT_EQ(dst, std::vector<int32_t>(4, 21 + 21));
}

TEST(CommandBufferThunkTest, IfCmd) {
  se::StreamExecutor* executor = CudaExecutor();
  if (!se::CommandBuffer::SupportsConditionalCommands(executor->platform())) {
    GTEST_SKIP() << "CUDA graph conditionals are not supported";
  }

  se::Stream stream(executor);
  stream.Init();
  ASSERT_TRUE(stream.ok());

  int64_t length = 4;
  int64_t byte_length = sizeof(int32_t) * length;

  // Prepare arguments: pred=true, a=42, b=0
  se::DeviceMemory<bool> pred = executor->AllocateArray<bool>(1, 0);
  se::DeviceMemory<int32_t> a = executor->AllocateArray<int32_t>(length, 0);
  se::DeviceMemory<int32_t> b = executor->AllocateArray<int32_t>(length, 0);

  constexpr bool kTrue = true;
  stream.ThenMemcpy(&pred, &kTrue, 1);
  stream.ThenMemset32(&a, 42, byte_length);
  stream.ThenMemZero(&b, byte_length);

  // Prepare buffer allocations for recording command buffer.
  BufferAllocation alloc_p(/*index=*/0, 1, /*color=*/0);
  BufferAllocation alloc_a(/*index=*/1, byte_length, /*color=*/0);
  BufferAllocation alloc_b(/*index=*/2, byte_length, /*color=*/0);

  BufferAllocation::Slice slice_p(&alloc_p, 0, 1);
  BufferAllocation::Slice slice_a(&alloc_a, 0, byte_length);
  BufferAllocation::Slice slice_b(&alloc_b, 0, byte_length);

  auto args = {slice_a, slice_a, slice_b};  // b = a + a

  // Prepare commands sequence for `then` branch.
  CommandBufferCmdSequence then_commands;
  then_commands.Emplace<LaunchCmd>("add", args, LaunchDimensions(1, 4),
                                   /*shmem_bytes=*/0);

  // Prepare commands sequence for thunk.
  CommandBufferCmdSequence commands;
  commands.Emplace<IfCmd>(slice_p, std::move(then_commands));

  // Construct a thunk with command sequence.
  CommandBufferThunk thunk(std::move(commands), Thunk::ThunkInfo(nullptr));

  ServiceExecutableRunOptions run_options;
  BufferAllocations allocations({pred, a, b}, 0, executor->GetAllocator());
  Thunk::ExecuteParams params(run_options, allocations, &stream, {});

  CommandBufferCmd::ExecutableSource source = {
      /*text=*/se::cuda::internal::kAddI32Kernel, /*binary=*/{}};
  TF_ASSERT_OK(thunk.Initialize(executor, source));

  // Execute command buffer thunk and verify that it added the value.
  TF_ASSERT_OK(thunk.ExecuteOnStream(params));
  TF_ASSERT_OK(stream.BlockHostUntilDone());

  // Copy `b` data back to host.
  std::vector<int32_t> dst(4, 0);
  stream.ThenMemcpy(dst.data(), b, byte_length);

  ASSERT_EQ(dst, std::vector<int32_t>(4, 42 + 42));

  // Prepare buffer allocation for updating command buffer: c=0
  se::DeviceMemory<int32_t> c = executor->AllocateArray<int32_t>(length, 0);
  stream.ThenMemZero(&c, byte_length);

  // Update buffer allocation #2 to buffer `c`.
  allocations = BufferAllocations({pred, a, c}, 0, executor->GetAllocator());

  // Thunk execution should automatically update underlying command buffer.
  TF_ASSERT_OK(thunk.ExecuteOnStream(params));
  TF_ASSERT_OK(stream.BlockHostUntilDone());

  // Copy `c` data back to host.
  std::fill(dst.begin(), dst.end(), 0);
  stream.ThenMemcpy(dst.data(), c, byte_length);

  ASSERT_EQ(dst, std::vector<int32_t>(4, 42 + 42));
}

TEST(CommandBufferThunkTest, IfElseCmd) {
  se::StreamExecutor* executor = CudaExecutor();
  if (!se::CommandBuffer::SupportsConditionalCommands(executor->platform())) {
    GTEST_SKIP() << "CUDA graph conditionals are not supported";
  }

  se::Stream stream(executor);
  stream.Init();
  ASSERT_TRUE(stream.ok());

  int64_t length = 4;
  int64_t byte_length = sizeof(int32_t) * length;

  // Prepare arguments: pred=true, a=42, b=0
  se::DeviceMemory<bool> pred = executor->AllocateArray<bool>(1, 0);
  se::DeviceMemory<int32_t> a = executor->AllocateArray<int32_t>(length, 0);
  se::DeviceMemory<int32_t> b = executor->AllocateArray<int32_t>(length, 0);

  constexpr bool kTrue = true;
  stream.ThenMemcpy(&pred, &kTrue, 1);
  stream.ThenMemset32(&a, 42, byte_length);
  stream.ThenMemZero(&b, byte_length);

  // Prepare buffer allocations for recording command buffer.
  BufferAllocation alloc_p(/*index=*/0, 1, /*color=*/0);
  BufferAllocation alloc_a(/*index=*/1, byte_length, /*color=*/0);
  BufferAllocation alloc_b(/*index=*/2, byte_length, /*color=*/0);

  BufferAllocation::Slice slice_p(&alloc_p, 0, 1);
  BufferAllocation::Slice slice_a(&alloc_a, 0, byte_length);
  BufferAllocation::Slice slice_b(&alloc_b, 0, byte_length);

  // Prepare commands sequence for `then` & `else` branches.
  CommandBufferCmdSequence then_commands;
  CommandBufferCmdSequence else_commands;

  {  // Then: b = a + a
    auto args = {slice_a, slice_a, slice_b};
    then_commands.Emplace<LaunchCmd>("add", args, LaunchDimensions(1, 4),
                                     /*shmem_bytes=*/0);
  }

  {  // Else: b = b + b
    auto args = {slice_b, slice_b, slice_b};
    else_commands.Emplace<LaunchCmd>("add", args, LaunchDimensions(1, 4),
                                     /*shmem_bytes=*/0);
  }

  // Prepare commands sequence for thunk.
  CommandBufferCmdSequence commands;
  commands.Emplace<IfElseCmd>(slice_p, std::move(then_commands),
                              std::move(else_commands));

  // Construct a thunk with command sequence.
  CommandBufferThunk thunk(std::move(commands), Thunk::ThunkInfo(nullptr));

  ServiceExecutableRunOptions run_options;
  BufferAllocations allocations({pred, a, b}, 0, executor->GetAllocator());
  Thunk::ExecuteParams params(run_options, allocations, &stream, {});

  CommandBufferCmd::ExecutableSource source = {
      /*text=*/se::cuda::internal::kAddI32Kernel, /*binary=*/{}};
  TF_ASSERT_OK(thunk.Initialize(executor, source));

  // Execute command buffer thunk and verify that it added the value.
  TF_ASSERT_OK(thunk.ExecuteOnStream(params));
  TF_ASSERT_OK(stream.BlockHostUntilDone());

  // Copy `b` data back to host.
  std::vector<int32_t> dst(4, 0);
  stream.ThenMemcpy(dst.data(), b, byte_length);

  ASSERT_EQ(dst, std::vector<int32_t>(4, 42 + 42));

  // Change branch to `else` and check that it updated the `b` buffer.
  constexpr bool kFalse = false;
  stream.ThenMemcpy(&pred, &kFalse, 1);

  TF_ASSERT_OK(thunk.ExecuteOnStream(params));
  TF_ASSERT_OK(stream.BlockHostUntilDone());

  stream.ThenMemcpy(dst.data(), b, byte_length);
  ASSERT_EQ(dst, std::vector<int32_t>(4, 2 * (42 + 42)));
}

TEST(CommandBufferThunkTest, CaseCmd) {
  se::StreamExecutor* executor = CudaExecutor();
  if (!se::CommandBuffer::SupportsConditionalCommands(executor->platform())) {
    GTEST_SKIP() << "CUDA graph conditionals are not supported";
  }

  se::Stream stream(executor);
  stream.Init();
  ASSERT_TRUE(stream.ok());

  int64_t length = 4;
  int64_t byte_length = sizeof(int32_t) * length;

  // Prepare arguments: index=0, a=42, b=0
  se::DeviceMemory<int32_t> index = executor->AllocateArray<int32_t>(1, 0);
  se::DeviceMemory<int32_t> a = executor->AllocateArray<int32_t>(length, 0);
  se::DeviceMemory<int32_t> b = executor->AllocateArray<int32_t>(length, 0);

  stream.ThenMemset32(&index, 0, sizeof(int32_t));
  stream.ThenMemset32(&a, 42, byte_length);
  stream.ThenMemZero(&b, byte_length);

  // Prepare buffer allocations for recording command buffer.
  BufferAllocation alloc_i(/*index=*/0, 1, /*color=*/0);
  BufferAllocation alloc_a(/*index=*/1, byte_length, /*color=*/0);
  BufferAllocation alloc_b(/*index=*/2, byte_length, /*color=*/0);

  BufferAllocation::Slice slice_i(&alloc_i, 0, sizeof(int32_t));
  BufferAllocation::Slice slice_a(&alloc_a, 0, byte_length);
  BufferAllocation::Slice slice_b(&alloc_b, 0, byte_length);

  // Prepare commands sequence for branches.
  std::vector<CommandBufferCmdSequence> branches(2);

  {  // Case 0: b = a + a
    auto args = {slice_a, slice_a, slice_b};
    branches[0].Emplace<LaunchCmd>("add", args, LaunchDimensions(1, 4),
                                   /*shmem_bytes=*/0);
  }

  {  // Case 1: b = b + b
    auto args = {slice_b, slice_b, slice_b};
    branches[1].Emplace<LaunchCmd>("add", args, LaunchDimensions(1, 4),
                                   /*shmem_bytes=*/0);
  }

  // Prepare commands sequence for thunk.
  CommandBufferCmdSequence commands;
  commands.Emplace<CaseCmd>(slice_i, std::move(branches));

  // Construct a thunk with command sequence.
  CommandBufferThunk thunk(std::move(commands), Thunk::ThunkInfo(nullptr));

  ServiceExecutableRunOptions run_options;
  BufferAllocations allocations({index, a, b}, 0, executor->GetAllocator());
  Thunk::ExecuteParams params(run_options, allocations, &stream, {});

  CommandBufferCmd::ExecutableSource source = {
      /*text=*/se::cuda::internal::kAddI32Kernel, /*binary=*/{}};
  TF_ASSERT_OK(thunk.Initialize(executor, source));

  // Execute command buffer thunk and verify that it added the value.
  TF_ASSERT_OK(thunk.ExecuteOnStream(params));
  TF_ASSERT_OK(stream.BlockHostUntilDone());

  // Copy `b` data back to host.
  std::vector<int32_t> dst(4, 0);
  stream.ThenMemcpy(dst.data(), b, byte_length);

  ASSERT_EQ(dst, std::vector<int32_t>(4, 42 + 42));

  // Change `index` to `1` and check that it updated the `b` buffer.
  stream.ThenMemset32(&index, 1, sizeof(int32_t));

  TF_ASSERT_OK(thunk.ExecuteOnStream(params));
  TF_ASSERT_OK(stream.BlockHostUntilDone());

  stream.ThenMemcpy(dst.data(), b, byte_length);
  ASSERT_EQ(dst, std::vector<int32_t>(4, 2 * (42 + 42)));
}

TEST(CommandBufferThunkTest, ForCmd) {
  se::StreamExecutor* executor = CudaExecutor();
  if (!se::CommandBuffer::SupportsConditionalCommands(executor->platform())) {
    GTEST_SKIP() << "CUDA graph conditionals are not supported";
  }

  se::Stream stream(executor);
  stream.Init();
  ASSERT_TRUE(stream.ok());

  int64_t length = 4;
  int64_t byte_length = sizeof(int32_t) * length;

  // Prepare arguments: loop_cnt=0, a=1, b=0
  se::DeviceMemory<int32_t> loop_cnt = executor->AllocateArray<int32_t>(1, 0);
  se::DeviceMemory<int32_t> a = executor->AllocateArray<int32_t>(length, 0);
  se::DeviceMemory<int32_t> b = executor->AllocateArray<int32_t>(length, 0);

  stream.ThenMemset32(&loop_cnt, 0, sizeof(int32_t));
  stream.ThenMemset32(&a, 1, byte_length);
  stream.ThenMemZero(&b, byte_length);

  // Prepare buffer allocations for recording command buffer.
  BufferAllocation alloc_cnt(/*index=*/0, 1, /*color=*/0);
  BufferAllocation alloc_a(/*index=*/1, byte_length, /*color=*/0);
  BufferAllocation alloc_b(/*index=*/2, byte_length, /*color=*/0);

  BufferAllocation::Slice slice_cnt(&alloc_cnt, 0, sizeof(int32_t));
  BufferAllocation::Slice slice_a(&alloc_a, 0, byte_length);
  BufferAllocation::Slice slice_b(&alloc_b, 0, byte_length);

  auto args = {slice_a, slice_b, slice_b};  // b = a + b

  // Prepare commands sequence for loop `body`.
  CommandBufferCmdSequence body_commands;
  body_commands.Emplace<LaunchCmd>("add", args, LaunchDimensions(1, 4),
                                   /*shmem_bytes=*/0);

  // Prepare commands sequence for thunk.
  CommandBufferCmdSequence commands;
  commands.Emplace<ForCmd>(/*num_iterations=*/10, slice_cnt,
                           std::move(body_commands));

  // Construct a thunk with command sequence.
  CommandBufferThunk thunk(std::move(commands), Thunk::ThunkInfo(nullptr));

  ServiceExecutableRunOptions run_options;
  BufferAllocations allocations({loop_cnt, a, b}, 0, executor->GetAllocator());
  Thunk::ExecuteParams params(run_options, allocations, &stream, {});

  CommandBufferCmd::ExecutableSource source = {
      /*text=*/se::cuda::internal::kAddI32Kernel, /*binary=*/{}};
  TF_ASSERT_OK(thunk.Initialize(executor, source));

  // Execute command buffer thunk and verify that it added the value 10 times.
  TF_ASSERT_OK(thunk.ExecuteOnStream(params));
  TF_ASSERT_OK(stream.BlockHostUntilDone());

  // Copy `b` data back to host.
  std::vector<int32_t> dst(4, 0);
  stream.ThenMemcpy(dst.data(), b, byte_length);

  ASSERT_EQ(dst, std::vector<int32_t>(4, 10));
}

TEST(CommandBufferThunkTest, WhileCmd) {
  // TODO(ezhulenev): Find a way to test WhileCmd: add a test only TraceCmd that
  // could allow us trace custom kernels to update while loop iterations. Or
  // maybe add a CustomLaunchCmd and wrap loop update into custom kernel.
}

}  // namespace xla::gpu
