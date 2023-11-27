/* Copyright 2019 The TensorFlow Authors. All Rights Reserved.

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

#include "tensorflow/compiler/tf2xla/mlir_bridge_pass.h"

#include <string>

#include "tensorflow/compiler/mlir/tf2xla/mlir_bridge_rollout_policy.h"
#include "absl/base/call_once.h"
#include "absl/log/log.h"
#include "absl/status/status.h"
#include "mlir/Dialect/Func/IR/FuncOps.h"  // from @llvm-project
#include "mlir/IR/BuiltinOps.h"  // from @llvm-project
#include "mlir/IR/Visitors.h"  // from @llvm-project
#include "tensorflow/compiler/mlir/mlir_graph_optimization_pass.h"
#include "tensorflow/compiler/mlir/tensorflow/ir/tf_ops.h"
#include "tensorflow/compiler/mlir/tensorflow/ir/tf_structs.h"
#include "tensorflow/compiler/mlir/tensorflow/transforms/host_runtime/lower_cluster_to_runtime_ops.h"
#include "tensorflow/compiler/mlir/tensorflow/utils/device_util.h"
#include "tensorflow/compiler/mlir/tf2xla/api/v1/cluster_tf.h"
#include "tensorflow/compiler/mlir/tf2xla/api/v1/tf_dialect_to_executor.h"
#include "tensorflow/compiler/mlir/tf2xla/api/v2/cluster_tf.h"
#include "tensorflow/compiler/mlir/tf2xla/api/v2/tf_dialect_to_executor.h"
#include "tensorflow/compiler/tf2xla/tf2xla_defs.h"
#include "tensorflow/compiler/tf2xla/xla_op_registry.h"
#include "tensorflow/core/common_runtime/device_set.h"
#include "tensorflow/core/framework/metrics.h"
#include "tensorflow/core/lib/monitoring/gauge.h"
#include "tensorflow/core/platform/status.h"
#include "tensorflow/core/public/session_options.h"
#include "tensorflow/core/tpu/tpu_defs.h"
#include "tensorflow/core/util/device_name_utils.h"
#include "tsl/framework/device_type.h"
#include "tsl/platform/errors.h"

namespace tensorflow {

auto* mlir_bridge_gauge_v1 = monitoring::Gauge<bool, 0>::New(
    "/tensorflow/config/experimental/enable_mlir_bridge_gauge_v1",
    "Tracks usage of the MLIR-based TF2XLA bridge among TF1 models");
auto* mlir_bridge_gauge_v2 = monitoring::Gauge<bool, 0>::New(
    "/tensorflow/config/experimental/enable_mlir_bridge_gauge_v2",
    "Tracks usage of the MLIR-based TF2XLA bridge among TF2 models");

namespace {

using ::mlir::ModuleOp;

bool HasTPUDevice(mlir::ModuleOp module) {
  mlir::TF::RuntimeDevices devices;
  if (failed(GetDevicesFromOp(module.getOperation(), &devices))) return false;
  return absl::c_any_of(
      devices.device_names(),
      [](const tensorflow::DeviceNameUtils::ParsedName& device) {
        return device.has_type && device.type == kTpuDevice;
      });
}

bool HasTPUOp(mlir::ModuleOp module) {
  auto walk_result = module.walk([&](mlir::Operation* op) {
    // Check for ops with compile device type "TPU". This allows us to support
    // TPU compilation without replication. Note that currently the compile
    // device type is not set by default before bridge, only if eager context
    // attribute `jit_compile_rewrite` is true.
    // TODO(b/229028654): Remove string conversion once we have C++17.
    const llvm::StringRef compile_device_type_attr_name(
        kCompileDeviceTypeAttr.data(), kCompileDeviceTypeAttr.size());
    auto compilation_attr =
        op->getAttrOfType<mlir::StringAttr>(compile_device_type_attr_name);
    if (compilation_attr && compilation_attr.getValue().str() == kTpuDevice) {
      return mlir::WalkResult::interrupt();
    }
    // TODO(b/223677572): Once the scope for new compilation and replication
    // markers is expanded beyond bridge we can remove this check for
    // `kTPUReplicateAttr`, we will then always have a `kCompileDeviceTypeAttr`
    // in such cases (see above).
    // TODO(b/229028654): Remove string conversion once we have C++17.
    const llvm::StringRef tpu_replicate_attr_name(kTpuReplicateAttr.data(),
                                                  kTpuReplicateAttr.size());
    auto replicate_attr =
        op->getAttrOfType<mlir::StringAttr>(tpu_replicate_attr_name);
    if (replicate_attr) return mlir::WalkResult::interrupt();
    return mlir::WalkResult::advance();
  });
  return walk_result.wasInterrupted();
}

// Checks that the module has both TPU devices in its device list and contains
// TPU ops.
bool HasTPUDevicesAndOps(mlir::ModuleOp module) {
  return HasTPUDevice(module) && HasTPUOp(module);
}

bool HasTPUDevice(const DeviceSet& device_set) {
  for (const Device* device : device_set.devices()) {
    if (!device) continue;
    const DeviceNameUtils::ParsedName& name = device->parsed_name();
    if (name.has_type && name.type == "TPU") return true;
  }
  return false;
}

// Check that graph has tf.StatefulPartitionedCall op with _XlaMustCompile.
bool HasQualifiedNonTPUOp(const Graph& graph) {
  const std::string kStatefulPartitionedCallOp = "StatefulPartitionedCall";
  const std::string kXlaMustCompile = "_XlaMustCompile";
  for (const Node* node : graph.nodes()) {
    auto node_op = node->type_string();
    if (node_op == kStatefulPartitionedCallOp) {
      auto attr = node->attrs().FindByString(kXlaMustCompile);
      if (attr != nullptr && attr->b() == true) {
        return true;
      }
    }
  }
  return false;
}

bool HasTPUPartitionedCallOpInModule(mlir::ModuleOp module) {
  bool has_tpu_partitioned_call = false;
  for (auto func_op : module.getOps<mlir::func::FuncOp>()) {
    func_op->walk([&](mlir::TF::TPUPartitionedCallOp op) {
      has_tpu_partitioned_call = true;
    });
    if (has_tpu_partitioned_call) break;
  }
  return has_tpu_partitioned_call;
}

// V1 Compat Bridge extracts out a program into a submodule and runs clustering
// only on the submodule.
absl::Status RunLowerToRuntimeOpsOnSubmodule(ModuleOp parent_module,
                                             bool is_in_fallback_enabled_mode) {
  int num_submodules = 0;
  absl::Status runtime_lowering_status;
  parent_module.walk([&](ModuleOp submodule) {
    if (submodule == parent_module) return mlir::WalkResult::advance();
    num_submodules++;
    runtime_lowering_status =
        tensorflow::tfrt_compiler::RunLowerClusterToRuntimeOpsPassPipeline(
            submodule, tsl::DeviceType(DEVICE_TPU_XLA_JIT));
    if (num_submodules > 1) {
      return mlir::WalkResult::interrupt();
    }

    return mlir::WalkResult::advance();
  });

  if (num_submodules > 1) {
    return absl::InternalError(
        "Lower to runtime has more than one submodule. Erroring out.");
  }

  return runtime_lowering_status;
}

}  // namespace

// Analyzes the user requested policy as well as the contents of the graph and
// function_library_definition to determine whether the MLIR Bridge should be
// run.
//
// If the user explicitly requests the bridge be enabled or disabled, this
// function will respect the request. If the user does not explicitly request
// enabled or disabled, it will decide whether or not to run the bridge.
//
// The config_proto param is a required input for all TF1 graphs but it is
// redundant for TF2 graphs.
MlirOptimizationPassState GetPassStateImpl(
    bool run_tpu_bridge, const ConfigProto& config_proto, const Graph& graph,
    const FunctionLibraryDefinition& function_library) {
  // Skip MLIR TF/XLA Bridge if no TPU devices and no qualified CPU/GPU
  // graphs are found.
  if (!run_tpu_bridge && !HasQualifiedNonTPUOp(graph)) {
    VLOG(3) << "Skipping MLIR CPU/GPU Bridge, "
               "graph is not qualified to run the bridge";
    return MlirOptimizationPassState::Disabled;
  }

  // We set `uses_uninitialized_resource_args` to false here because the first
  // phase of the bridge is not affected by uninitialized resource args.
  // GetMlirBridgeRolloutPolicy will analyze a TPU graph if users have not
  // explicltly requested a policy.
  MlirBridgeRolloutPolicy policy = GetMlirBridgeRolloutPolicy(
      graph, &function_library, config_proto, /*run_tpu_bridge*/ run_tpu_bridge,
      /*uses_uninitialized_resource_args=*/false,
      /*is_v1_compat=*/false, /*record_stats=*/false);
  // GetPassState is called once before MlirBridgePass starts, and the pass
  // gets skipped if it is disabled. Log such cases in this function. The cases
  // where the pass is enabled will only be logged during their execution to
  // prevent them from being counted twice.
  if (run_tpu_bridge) {
    switch (policy) {
      case MlirBridgeRolloutPolicy::kEnabledByUser:
        return MlirOptimizationPassState::Enabled;
      case MlirBridgeRolloutPolicy::kEnabledAfterGraphAnalysis:
        return MlirOptimizationPassState::FallbackEnabled;
      case MlirBridgeRolloutPolicy::kDisabledByUser:
        VLOG(1) << "Skipping MLIR TPU Bridge, disabled by user. "
                   "Old bridge will evaluate.";
        metrics::UpdateTfMlirBridgeFirstPhaseCounter("tpu", "v2", true,
                                                     "disabled_by_user");
        return MlirOptimizationPassState::Disabled;
      case MlirBridgeRolloutPolicy::kDisabledAfterGraphAnalysis:
        VLOG(1) << "Skipping MLIR TPU Bridge, disabled because "
                   "graph has unsupported features. Old bridge will evaluate.";
        metrics::UpdateTfMlirBridgeFirstPhaseCounter("tpu", "v2", true,
                                                     "invalid_graph");
        // We set `uses_uninitialized_resource_args` to false here because the
        // first phase of the bridge is not affected by uninitialized resource
        // args.
        // For Invalid Graph Analysis we need to log here because Run will not
        // be called.
        LogGraphFeatures(graph, &function_library, config_proto,
                         /*uses_uninitialized_resource_args=*/false,
                         /*is_v1_compat=*/false);
        return MlirOptimizationPassState::Disabled;
    }
  }
  // TODO(b/277112519): Have uniform behavior for GPU/CPU and TPU
  switch (policy) {
    case MlirBridgeRolloutPolicy::kEnabledByUser:
      return MlirOptimizationPassState::Enabled;
    case MlirBridgeRolloutPolicy::kEnabledAfterGraphAnalysis:
      return MlirOptimizationPassState::FallbackEnabled;
    case MlirBridgeRolloutPolicy::kDisabledByUser:
      VLOG(1) << "Skipping MLIR CPU/GPU Bridge, disabled by user.";
      metrics::UpdateTfMlirBridgeFirstPhaseCounter("cpu/gpu", "v2", false,
                                                   "disabled_by_user");
      return MlirOptimizationPassState::Disabled;
    default:
      // This case should never be hit. Added here to be consistent with OSS
      // implementation.
      metrics::UpdateTfMlirBridgeFirstPhaseCounter("cpu/gpu", "v2", false,
                                                   "invalid_graph");
      return MlirOptimizationPassState::Disabled;
  }
}

MlirOptimizationPassState MlirBridgePass::GetPassState(
    const DeviceSet* device_set, const ConfigProto& config_proto,
    const Graph& graph,
    const FunctionLibraryDefinition& function_library) const {
  if (!device_set) {
    // This is not expected in practice.
    VLOG(1) << "Device set is empty!";
    return MlirOptimizationPassState::Disabled;
  }

  return GetPassStateImpl(/*run_tpu_bridge*/ HasTPUDevice(*device_set),
                          config_proto, graph, function_library);
}

// This runs the first phase of the "bridge", transforming the graph in a form
// that can be executed with delegation of some computations to an accelerator.
// This builds on the model of XLA where a subset of the graph is encapsulated
// and attached to a "compile" operation, whose result is fed to an "execute"
// operation. The kernel for these operations is responsible to lower the
// encapsulated graph to a particular device.
Status MlirBridgePass::Run(const std::string& function_name,
                           const ConfigProto& config_proto,
                           mlir::ModuleOp module, const Graph& graph,
                           const FunctionLibraryDefinition& function_library) {
  static absl::once_flag flag;
  absl::call_once(flag, UpdateLogVerbosityIfDefined, "TF_DEBUG_LOG_VERBOSITY");

  // Check if there are TPU devices or TPU ops. If not, then check if the
  // non TPU graph is qualified to run TF2XLA Bridge.
  // This check needs to precede GetPassState for instrumentation purposes.
  bool run_tpu_bridge = HasTPUDevicesAndOps(module);
  if (!run_tpu_bridge && !HasQualifiedNonTPUOp(graph)) {
    VLOG(1)
        << "Skipping MLIR TF2XLA Bridge, no qualified devices or ops found.";
    return OkStatus();
  }

  if (HasTPUPartitionedCallOpInModule(module)) {
    VLOG(1) << "Skipping MLIR TF2XLA Bridge. This is an inference graph, "
               "Session V1 Bridge should be used during execution of "
               "TPUPartitionedCall.";
    return OkStatus();
  }

  // TODO(b/241853328): Add caching of pass state and call logging/metrics
  // related to graph analysis from here.
  auto pass_state =
      GetPassStateImpl(run_tpu_bridge, config_proto, graph, function_library);

  if (pass_state == MlirOptimizationPassState::Disabled) {
    // GetPassState is called before run() and run() will only be called if the
    // pass is not disabled. However, the graph may have been updated between
    // when the pass state was originally calculated and now, so this check is
    // required to reflect any possible changes.
    VLOG(1) << "MlirBridgePass is disabled and will not run.";
    return OkStatus();
  }

  bool fallback_enabled = false;
  if (run_tpu_bridge) {
    if (pass_state == MlirOptimizationPassState::FallbackEnabled) {
      // We set `uses_uninitialized_resource_args` to false here because the
      // first phase of the bridge is not affected by uninitialized resource
      // args.
      // TODO (b/241853328) Consider moving logging if caching for graph
      // analysis or GetPassState is added
      LogGraphFeatures(graph, &function_library, config_proto,
                       /*uses_uninitialized_resource_args=*/false,
                       /*is_v1_compat=*/false);
      fallback_enabled = true;
    }
    VLOG(1) << "Running MLIR TPU Bridge";
    mlir_bridge_gauge_v2->GetCell()->Set(true);

    TF_RETURN_IF_ERROR(
        tensorflow::tf2xla::v2::RunFunctionTf2xlaClusteringBridge(
            module, tf2xla::v2::DeviceType::XLA_TPU_JIT, fallback_enabled,
            function_name));

    TF_RETURN_IF_ERROR(
        tensorflow::tfrt_compiler::RunLowerClusterToRuntimeOpsPassPipeline(
            module, tsl::DeviceType(DEVICE_TPU_XLA_JIT), function_name));
  } else {
    VLOG(1) << "Running GPU/CPU Bridge";
    TF_RETURN_IF_ERROR(
        tensorflow::tf2xla::v2::RunFunctionTf2xlaClusteringBridge(
            module, tf2xla::v2::DeviceType::XLA_GPU_JIT, fallback_enabled,
            function_name));

    TF_RETURN_IF_ERROR(
        tensorflow::tfrt_compiler::RunLowerClusterToRuntimeOpsPassPipeline(
            module, tsl::DeviceType(DEVICE_GPU_XLA_JIT), function_name));
  }

  return tensorflow::tf2xla::v2::ExportFromTensorflowDialectToExecutor(
      module, function_name);
}

MlirOptimizationPassState MlirBridgeV1CompatPass::GetPassState(
    const DeviceSet* device_set, const ConfigProto& config_proto,
    const Graph& graph,
    const FunctionLibraryDefinition& function_library) const {
  // Skip MLIR TPU Bridge if no TPU devices found.
  if (device_set && !HasTPUDevice(*device_set))
    return MlirOptimizationPassState::Disabled;
  // We set `uses_uninitialized_resource_args` to false here because the first
  // phase of the bridge is not affected by uninitialized resource args.
  MlirBridgeRolloutPolicy policy = GetMlirBridgeRolloutPolicy(
      graph, /*function_library=*/&function_library, config_proto,
      /*run_tpu_bridge*/ true,
      /*uses_uninitialized_resource_args=*/false, /*is_v1_compat=*/true,
      /*record_stats=*/false);
  switch (policy) {
    case MlirBridgeRolloutPolicy::kEnabledByUser:
      return MlirOptimizationPassState::Enabled;
    case MlirBridgeRolloutPolicy::kEnabledAfterGraphAnalysis:
      return MlirOptimizationPassState::FallbackEnabled;
    case MlirBridgeRolloutPolicy::kDisabledByUser:
      VLOG(1) << "Skipping MLIR TPU Bridge V1 Compat, MLIR TPU bridge disabled "
                 "by user. Old bridge will evaluate.";
      metrics::UpdateTfMlirBridgeFirstPhaseCounter("tpu", "v1", true,
                                                   "disabled_by_user");
      return MlirOptimizationPassState::Disabled;
    case MlirBridgeRolloutPolicy::kDisabledAfterGraphAnalysis:
      VLOG(1) << "Skipping MLIR TPU Bridge V1 Compat, MLIR TPU bridge disabled "
                 "because graph has unsupported features. Old bridge will "
                 "evaluate.";
      metrics::UpdateTfMlirBridgeFirstPhaseCounter("tpu", "v1", true,
                                                   "invalid_graph");
      // We set `uses_uninitialized_resource_args` to false here because the
      // first phase of the bridge is not affected by uninitialized resource
      // args.
      // For Invalid Graph Analysis we need to log here because Run will not be
      // called.
      LogGraphFeatures(graph, &function_library, config_proto,
                       /*uses_uninitialized_resource_args=*/false,
                       /*is_v1_compat=*/true);
      return MlirOptimizationPassState::Disabled;
  }
}

Status MlirBridgeV1CompatPass::Run(const GraphOptimizationPassOptions& options,
                                   mlir::ModuleOp module) {
  static absl::once_flag flag;
  absl::call_once(flag, UpdateLogVerbosityIfDefined, "TF_DEBUG_LOG_VERBOSITY");

  // Skip function graphs as MlirBridgePass will be used instead.
  if (options.is_function_graph) return OkStatus();

  // Skip MLIR TPU Bridge if no TPU devices or TPU ops found.
  if (!HasTPUDevicesAndOps(module)) {
    VLOG(1) << "Skipping MLIR TPU Bridge V1 Compat, no TPU devices or TPU ops "
               "found";
    return OkStatus();
  }

  MlirOptimizationPassState pass_state =
      GetPassState(/*device_set=*/nullptr, options.session_options->config,
                   **options.graph, *options.flib_def);

  // Set device_set to nullptr here as the device specific checks are performed
  // based on the devices in the module.
  if (pass_state == MlirOptimizationPassState::Disabled) {
    // GetPassState is called before run() and run() will only be called if the
    // pass is not disabled. However, the graph may have been updated between
    // when the pass state was originally calculated and now, so this check is
    // required to reflect any possible changes.
    VLOG(1) << "Skipping MLIR TPU Bridge V1 Compat, session flag not enabled";
    mlir_bridge_gauge_v1->GetCell()->Set(false);
    return OkStatus();
  }

  // 1) If the MLIR module contains a TPUPartitionedCall, we skip here
  // 2) When TPUPartitionedCall starts executing, it calls MLIR bridge as a
  // part of PRE_PLACEMENT optimization
  // 3) This MLIR bridge version is V1 Compat
  if (HasTPUPartitionedCallOpInModule(module)) {
    VLOG(1)
        << "Skipping MLIR TPU Bridge V1 Compat. This is an inference graph, V1 "
           "Compat should be used during execution of TPUPartitionedCall.";
    return OkStatus();
  }

  bool fallback_enabled = false;
  if (pass_state == MlirOptimizationPassState::FallbackEnabled) {
    // We set `uses_uninitialized_resource_args` to false here because the first
    // phase of the bridge is not affected by uninitialized resource args.
    // TODO (b/241853328) Consider moving logging if caching for graph analysis
    // or GetPassState is added
    LogGraphFeatures(**options.graph, options.flib_def,
                     options.session_options->config,
                     /*uses_uninitialized_resource_args=*/false,
                     /*is_v1_compat=*/true);
    fallback_enabled = true;
  }

  VLOG(1) << "Running MLIR TPU Bridge V1 Compat";
  mlir_bridge_gauge_v1->GetCell()->Set(true);
  TF_RETURN_IF_ERROR(tensorflow::tf2xla::v1::RunSessionTf2xlaClusteringBridge(
      module, fallback_enabled));

  auto lower_cluster_to_runtime_ops_pass_pipeline =
      RunLowerToRuntimeOpsOnSubmodule(module, fallback_enabled);
  if (!lower_cluster_to_runtime_ops_pass_pipeline.ok()) {
    VLOG(1) << "Error while lowering cluster to runtime ops: "
            << lower_cluster_to_runtime_ops_pass_pipeline;
    return lower_cluster_to_runtime_ops_pass_pipeline;
  }

  return tensorflow::tf2xla::v1::ExportFromTensorflowDialectToExecutor(module);
}

}  // namespace tensorflow
