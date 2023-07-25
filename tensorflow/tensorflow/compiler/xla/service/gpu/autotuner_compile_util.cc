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

#include "tensorflow/compiler/xla/service/gpu/autotuner_compile_util.h"

#include <memory>
#include <optional>
#include <utility>
#include <vector>

#include "absl/status/status.h"
#include "absl/strings/string_view.h"
#include "absl/time/time.h"
#include "absl/types/span.h"
#include "tensorflow/compiler/xla/hlo/ir/hlo_clone_context.h"
#include "tensorflow/compiler/xla/hlo/ir/hlo_computation.h"
#include "tensorflow/compiler/xla/hlo/ir/hlo_instruction.h"
#include "tensorflow/compiler/xla/hlo/ir/hlo_module.h"
#include "tensorflow/compiler/xla/service/compiler.h"
#include "tensorflow/compiler/xla/service/executable.h"
#include "tensorflow/compiler/xla/service/gpu/autotuner_util.h"
#include "tensorflow/compiler/xla/service/gpu/gpu_executable_run_options.h"
#include "tensorflow/compiler/xla/service/gpu/ir_emission_utils.h"
#include "tensorflow/compiler/xla/service/hlo_module_config.h"
#include "tensorflow/compiler/xla/stream_executor/device_memory.h"
#include "tensorflow/compiler/xla/stream_executor/gpu/gpu_stream.h"
#include "tensorflow/compiler/xla/stream_executor/gpu/gpu_timer.h"
#include "tensorflow/compiler/xla/util.h"
#include "tensorflow/compiler/xla/xla.pb.h"
#include "tensorflow/tsl/platform/errors.h"
#include "tensorflow/tsl/platform/statusor.h"

namespace xla {
namespace gpu {

namespace {

std::vector<ExecutionInput> ExecutionInputsFromBuffers(
    Executable* executable, absl::Span<se::DeviceMemoryBase const> buffers) {
  const HloInstruction::InstructionVector& params =
      executable->module().entry_computation()->parameter_instructions();
  CHECK_EQ(params.size(), buffers.size());
  std::vector<ExecutionInput> inputs;
  for (int i = 0; i < params.size(); ++i) {
    inputs.emplace_back(params.at(i)->shape());
    // Our executable doesn't have input-output aliasing, so we can pass
    // unowned input buffers.
    inputs.back().SetUnownedBuffer(
        /*index=*/{}, MaybeOwningDeviceMemory(/*unowned=*/buffers.at(i)));
  }
  return inputs;
}

}  // namespace

AutotunerCompileUtil::AutotunerCompileUtil(const AutotuneConfig& config,
                                           Compiler* compiler,
                                           se::StreamExecutor& stream_executor,
                                           se::Stream& stream,
                                           se::DeviceMemoryAllocator& allocator,
                                           const DebugOptions& opts)
    : config_(config),
      compiler_(compiler),
      stream_executor_(stream_executor),
      stream_(stream),
      allocator_(allocator),
      opts_(opts) {
  // Avoid dumping compilation steps.
  opts_.set_xla_dump_to("");
  opts_.set_xla_gpu_dump_autotune_results_to("");
  opts_.set_xla_gpu_load_autotune_results_from("");
  opts_.set_xla_gpu_dump_llvmir(false);
  // Avoid using another thread pool.
  opts_.set_xla_gpu_force_compilation_parallelism(1);
  // Avoid using GPU graphs as we don't want to measure graph construction time.
  opts_.set_xla_gpu_graph_level(0);
  // Disable experimental OpenXLA runtime.
  opts_.set_xla_gpu_enable_openxla_runtime(false);
}

StatusOr<std::optional<AutotunerCompileUtil::ProfilingOutput>>
AutotunerCompileUtil::ProfileExecutable(
    Executable* executable, se::Stream* stream,
    absl::Span<se::DeviceMemoryBase const> input_buffers) {
  {
    std::vector<ExecutionInput> execution_inputs =
        ExecutionInputsFromBuffers(executable, input_buffers);
    // Warmup: in and out buffers are reused while probing different configs,
    // so GPU caches should be in some comparable states during measurements.
    TF_ASSIGN_OR_RETURN(ExecutionOutput execution_output,
                        Execute(*executable, std::move(execution_inputs)));
    TF_RETURN_IF_ERROR(stream->BlockHostUntilDone());
  }
  std::vector<ExecutionInput> execution_inputs =
      ExecutionInputsFromBuffers(executable, input_buffers);
  TF_ASSIGN_OR_RETURN(auto timer,
                      se::gpu::GpuTimer::Create(se::gpu::AsGpuStream(stream)));
  TF_ASSIGN_OR_RETURN(ExecutionOutput execution_output,
                      Execute(*executable, std::move(execution_inputs)));
  TF_ASSIGN_OR_RETURN(absl::Duration timer_duration,
                      timer.GetElapsedDuration());
  return std::make_optional<ProfilingOutput>(
      timer_duration, execution_output.Commit().ConsumeResult());
}

StatusOr<std::unique_ptr<Executable>> AutotunerCompileUtil::Compile(
    GenerateModuleFn extractor) {
  StatusOr<std::unique_ptr<HloModule>> new_hlo_module = extractor();
  if (new_hlo_module.status().GetPayload(kUncompilableFusion).has_value()) {
    // Incompatible value of split-k is an expected failure.
    return std::unique_ptr<Executable>();
  } else if (!new_hlo_module.status().ok()) {
    return new_hlo_module.status();
  }
  (*new_hlo_module)->config().set_debug_options(opts_);

  StatusOr<std::unique_ptr<Executable>> out = compiler_->RunBackend(
      std::move(*new_hlo_module), &stream_executor_,
      Compiler::CompileOptions{&allocator_, /*thread_pool=*/nullptr,
                               /*layout_canonicalization_callback=*/{},
                               /*is_autotuning_compilation=*/true});
  if (out.status().code() == absl::StatusCode::kResourceExhausted) {
    // Being out of shared memory budget is an expected failure.
    return std::unique_ptr<Executable>();
  }
  return out;
}

/*static*/ StatusOr<std::optional<AutotunerCompileUtil>>
AutotunerCompileUtil::Create(const AutotuneConfig& config,
                             const DebugOptions& opts) {
  if (config.IsDeviceless()) {
    return std::nullopt;
  }
  se::StreamExecutor* stream_exec = config.GetExecutor();
  se::DeviceMemoryAllocator* allocator = config.GetAllocator();
  TF_ASSIGN_OR_RETURN(se::Stream* const stream, config.GetStream());
  TF_ASSIGN_OR_RETURN(Compiler * compiler,
                      Compiler::GetForPlatform(stream_exec->platform()));
  return AutotunerCompileUtil(config, compiler, *stream_exec, *stream,
                              *allocator, opts);
}

StatusOr<ExecutionOutput> AutotunerCompileUtil::Execute(
    Executable& executable, std::vector<ExecutionInput> arguments) {
  // Require exclusive GPU lock to prevent other runs during autotuning.
  GpuExecutableRunOptions gpu_opts;
  gpu_opts.set_requires_exclusive_lock_on_gpu();

  ExecutableRunOptions run_options;
  run_options.set_device_ordinal(stream_executor_.device_ordinal());
  run_options.set_stream(&stream_);
  run_options.set_allocator(&allocator_);
  run_options.set_gpu_executable_run_options(&gpu_opts);
  ServiceExecutableRunOptions service_run_options(run_options);
  TF_ASSIGN_OR_RETURN(ExecutionOutput output,
                      executable.ExecuteAsyncOnStreamWrapper(
                          &service_run_options, std::move(arguments)));

  return std::move(output);
}

}  // namespace gpu
}  // namespace xla
