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

#ifndef XLA_SERVICE_GPU_AUTOTUNER_COMPILE_UTIL_H_
#define XLA_SERVICE_GPU_AUTOTUNER_COMPILE_UTIL_H_

#include <memory>
#include <optional>
#include <vector>

#include "absl/functional/any_invocable.h"
#include "absl/types/span.h"
#include "xla/hlo/ir/hlo_clone_context.h"
#include "xla/hlo/ir/hlo_computation.h"
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/hlo/ir/hlo_module.h"
#include "xla/service/compiler.h"
#include "xla/service/executable.h"
#include "xla/service/gpu/autotuner_util.h"
#include "xla/shape.h"
#include "xla/statusor.h"
#include "xla/stream_executor/stream.h"
#include "xla/util.h"
#include "xla/xla.pb.h"

namespace xla {
namespace gpu {

// Autotuning utils which require compiling fusions separately. Requires a
// separate target, as runtime autotuning cannot perform compilation.
class AutotunerCompileUtil {
 public:
  // The GenerateModuleFn must generate/extract a module using the provided
  // debug options. Typically it should set the debug options of the extracted
  // module before it would transform it, to ensure that the transforms can use
  // the debug options. In justified cases, it may override some of the provided
  // debug options.
  using GenerateModuleFn =
      absl::AnyInvocable<StatusOr<std::unique_ptr<HloModule>>(
          const DebugOptions&)>;

  // Generates a compile util for a platform associated with the `stream`.
  //
  // Returns an empty optional if the AutotuneConfig is deviceless, as
  // autotuning is impossible in that case.
  static StatusOr<std::optional<AutotunerCompileUtil>> Create(
      const AutotuneConfig& config, const DebugOptions& opts);

  struct ProfilingOutput {
    ProfilingOutput(absl::Duration duration, ScopedShapedBuffer&& buffer)
        : duration(duration), output(std::move(buffer)) {}

    absl::Duration duration;
    ScopedShapedBuffer output;
  };

  // Generates an executable first, given the module generator function in
  // `extractor`.
  //
  // Runs the resulting executable with the given extractor, cached with
  // `(cache_key, config)`. Returns `std::nullopt` on expected failure, bad
  // `Status` otherwise.
  StatusOr<std::optional<ProfilingOutput>> ProfileExecutable(
      Executable* executable, se::Stream* stream,
      absl::Span<se::DeviceMemoryBase const> input_buffers,
      absl::Span<Shape const> input_shapes);

  // Generic method to compile a generated module from `extractor` in isolation.
  //
  // Returns:
  //  - `nullptr` on *expected* failure
  //  - `Executable` if everything goes fine.
  //  - `Status` on *unexpected* failure.
  StatusOr<std::unique_ptr<Executable>> Compile(GenerateModuleFn extractor);

  // Generic method to extract an HLO using the debug options of the
  // AutotunerCompileUtil.
  //
  // Typically we can use Compile directly.
  StatusOr<std::unique_ptr<HloModule>> ExtractModule(
      GenerateModuleFn extractor);

 private:
  AutotunerCompileUtil(const AutotuneConfig& config, Compiler* compiler,
                       se::StreamExecutor& stream_executor, se::Stream& stream,
                       se::DeviceMemoryAllocator& allocator,
                       const DebugOptions& opts);

  StatusOr<ExecutionOutput> Execute(Executable& executable,
                                    std::vector<ExecutionInput> arguments);

  AutotuneConfig config_;
  Compiler* compiler_;
  se::StreamExecutor& stream_executor_;
  se::Stream& stream_;
  se::DeviceMemoryAllocator& allocator_;
  DebugOptions opts_;
};

}  // namespace gpu
}  // namespace xla

#endif  // XLA_SERVICE_GPU_AUTOTUNER_COMPILE_UTIL_H_
