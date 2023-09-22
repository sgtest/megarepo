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

#include "xla/pjrt/gpu/se_gpu_pjrt_compiler.h"

#include <memory>
#include <optional>

#include "absl/status/status.h"
#include "xla/client/xla_computation.h"
#include "xla/pjrt/gpu/se_gpu_pjrt_client.h"
#include "xla/pjrt/pjrt_client.h"
#include "xla/pjrt/pjrt_compiler.h"
#include "xla/pjrt/pjrt_executable.h"
#include "xla/status_macros.h"
#include "tsl/platform/errors.h"

#if GOOGLE_CUDA || TENSORFLOW_USE_ROCM
#include "xla/client/local_client.h"
#include "xla/pjrt/mlir_to_hlo.h"
#include "xla/pjrt/stream_executor_unloaded_executable.h"
#include "xla/pjrt/utils.h"
#include "xla/service/dump.h"
#include "xla/service/gpu/executable.pb.h"
#include "xla/service/gpu/gpu_compiler.h"
#include "xla/service/hlo_module_util.h"
#include "xla/service/hlo_proto_util.h"
#include "xla/service/local_service.h"
#include "xla/stream_executor/cuda/cuda_platform_id.h"
#endif

#if GOOGLE_CUDA
#include "xla/service/gpu/nvptx_compiler.h"
#elif TENSORFLOW_USE_ROCM
#include "xla/service/gpu/amdgpu_compiler.h"
#endif

namespace xla {
namespace {

bool IsGpuClient(const PjRtClient& client) {
  return client.platform_id() == GpuId();
}

bool IsSameTopology(const PjRtTopologyDescription& topology1,
                    const PjRtTopologyDescription& topology2) {
  const StreamExecutorGpuTopologyDescription& gpu_topology1 =
      tensorflow::down_cast<const StreamExecutorGpuTopologyDescription&>(
          topology1);
  const StreamExecutorGpuTopologyDescription& gpu_topology2 =
      tensorflow::down_cast<const StreamExecutorGpuTopologyDescription&>(
          topology2);
  return gpu_topology1 == gpu_topology2;
}

absl::Status IsValidTopologyAndClientForCompile(
    const PjRtTopologyDescription& topology, PjRtClient* client) {
  if (client == nullptr) {
    return absl::UnimplementedError(
        "SE:GPU compiler requires non-null client.");
  }
  if (!IsGpuClient(*client)) {
    return absl::InvalidArgumentError(
        "SE:GPU compiler requires a GPU PjRtClient.");
  }
  TF_ASSIGN_OR_RETURN(auto client_topology, client->GetTopologyDescription());

  if (!IsSameTopology(topology, *client_topology)) {
    return absl::UnimplementedError(
        "SE:GPU compiler requires the topology same as the one in the client.");
  }
  return absl::OkStatus();
}

#if GOOGLE_CUDA || TENSORFLOW_USE_ROCM
absl::StatusOr<std::unique_ptr<PjRtExecutable>> AotCompile(
    CompileOptions options, const XlaComputation& computation,
    gpu::GpuTargetConfig& gpu_target_config) {
  CompileOptions input_options = options;
  TF_RETURN_IF_ERROR(options.ApplyAllOptionOverrides());

  std::vector<const Shape*> argument_layout_pointers;
  TF_RETURN_IF_ERROR(DetermineArgumentLayoutsFromCompileOptions(
      computation,
      [](Shape shape) { return LayoutUtil::GetWithDefaultLayout(shape); },
      options.argument_layouts, &options.executable_build_options,
      &argument_layout_pointers));

  // TODO(b/300657649): Call `UpdateBuildOptions` like in LocalClient::Compile.
  // TODO(b/300657649): Get HloModuleConfig from `GetHloModuleConfig` like in
  // LocalService::CompileExecutables.
  HloModuleProto hlo_module_proto = computation.proto();
  TF_ASSIGN_OR_RETURN(ProgramShape shape, computation.GetProgramShape());
  DebugOptions debug_options = DefaultDebugOptionsIgnoringFlags();
  HloModuleConfig config(shape);
  config.set_debug_options(debug_options);

  TF_ASSIGN_OR_RETURN(std::unique_ptr<HloModule> hlo_module,
                      HloModule::CreateFromProto(hlo_module_proto, config));
#if GOOGLE_CUDA
  auto gpu_compiler = gpu::NVPTXCompiler();
#elif TENSORFLOW_USE_ROCM
  auto gpu_compiler = gpu::AMDGPUCompiler();
#endif

  UpdateEntryComputationLayout(
      hlo_module.get(), std::bind(&Compiler::DefaultDeviceShapeRepresentation,
                                  &gpu_compiler, std::placeholders::_1));
  DumpHloModuleIfEnabled(*hlo_module, kBeforeOptimizationsDumpName);

  if (!options.executable_build_options.run_backend_only()) {
    TF_ASSIGN_OR_RETURN(hlo_module,
                        gpu_compiler.RunHloPassesWithoutDevice(
                            std::move(hlo_module), Compiler::CompileOptions{},
                            gpu_target_config, AutotuneResults()));
  }

  AotCompilationOptions aot_options(gpu_compiler.PlatformId());
  aot_options.set_target_config(gpu_target_config);

  const int num_replicas = hlo_module->config().replica_count();
  const int num_partitions = hlo_module->config().num_partitions();
  const std::string name = hlo_module->name();
  auto unique_module_group =
      std::make_unique<HloModuleGroup>(std::move(hlo_module));
  TF_ASSIGN_OR_RETURN(
      std::vector<std::unique_ptr<AotCompilationResult>> aot_results,
      gpu_compiler.CompileAheadOfTime(std::move(unique_module_group),
                                      aot_options));
  return std::make_unique<StreamExecutorUnloadedExecutable>(
      std::move(input_options), std::move(aot_results), num_replicas,
      num_partitions, name);
}
#endif
}  // namespace

// TODO(b/285385306): Enable compilation on provided `topology`.
absl::StatusOr<std::unique_ptr<PjRtExecutable>>
StreamExecutorGpuCompiler::Compile(CompileOptions options,
                                   const XlaComputation& computation,
                                   const PjRtTopologyDescription& topology,
                                   PjRtClient* client) {
  if (client == nullptr && gpu_target_config_ != std::nullopt) {
#if GOOGLE_CUDA || TENSORFLOW_USE_ROCM
    return AotCompile(options, computation, *gpu_target_config_);
#endif
    return absl::InternalError(
        "GPU AOT compilation requires the target to be built with CUDA or "
        "ROCm.");
  }
  // TODO(b/296466237): Remove client dependency.
  TF_RETURN_IF_ERROR(IsValidTopologyAndClientForCompile(topology, client));
  return client->Compile(computation, options);
}

absl::StatusOr<std::unique_ptr<PjRtExecutable>>
StreamExecutorGpuCompiler::Compile(CompileOptions options,
                                   mlir::ModuleOp module,
                                   const PjRtTopologyDescription& topology,
                                   PjRtClient* client) {
  if (client == nullptr && gpu_target_config_ != std::nullopt) {
#if GOOGLE_CUDA || TENSORFLOW_USE_ROCM
    XlaComputation xla_computation;
    TF_RETURN_IF_ERROR(MlirToXlaComputation(
        module, xla_computation,
        /*use_tuple_args=*/options.parameter_is_tupled_arguments,
        /*return_tuple=*/false));
    return AotCompile(options, xla_computation, *gpu_target_config_);
#endif
    return absl::InternalError(
        "GPU AOT compilation requires the target to be built with CUDA or "
        "ROCm.");
  }
  // TODO(b/296466237): Remove client dependency.
  TF_RETURN_IF_ERROR(IsValidTopologyAndClientForCompile(topology, client));
  return client->Compile(module, options);
}

REGISTER_MODULE_INITIALIZER(pjrt_register_se_gpu_compiler, {
  std::unique_ptr<PjRtCompiler> compiler =
      std::make_unique<StreamExecutorGpuCompiler>();
  PjRtRegisterCompiler(GpuName(), std::move(compiler));
});
}  // namespace xla
