/* Copyright 2024 The TensorFlow Authors. All Rights Reserved.

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
#include "tensorflow/compiler/mlir/lite/quantization/stablehlo/quantization.h"

#include <string>
#include <unordered_set>
#include <vector>

#include "absl/container/flat_hash_map.h"
#include "absl/status/status.h"
#include "absl/status/statusor.h"
#include "absl/strings/string_view.h"
#include "mlir/IR/BuiltinOps.h"  // from @llvm-project
#include "tensorflow/cc/saved_model/constants.h"
#include "tensorflow/cc/saved_model/loader.h"
#include "tensorflow/compiler/mlir/quantization/stablehlo/cc/static_range_ptq.h"
#include "tensorflow/compiler/mlir/quantization/stablehlo/quantization_config.pb.h"
#include "tensorflow/compiler/mlir/quantization/tensorflow/python/py_function_lib.h"
#include "tensorflow/core/protobuf/meta_graph.pb.h"

namespace tensorflow {
namespace {

using ::mlir::quant::stablehlo::StaticRangePtqComponent;
using ::stablehlo::quantization::QuantizationConfig;
using ::tensorflow::SignatureDef;
using ::tensorflow::quantization::PyFunctionLibrary;

// Returns signature key -> `SignatureDef` mapping, excluding the signature for
// initialization op, which is only used during initialization.
// TODO: b/314124142 - Remove the need for this function.
absl::flat_hash_map<std::string, SignatureDef> GetSignatureDefMapFromBundle(
    const SavedModelBundle& saved_model_bundle) {
  // Translate protobuf::Map -> absl::flat_hash_map.
  const protobuf::Map<std::string, SignatureDef>& signatures =
      saved_model_bundle.GetSignatures();
  absl::flat_hash_map<std::string, SignatureDef> signature_def_map(
      signatures.begin(), signatures.end());

  // Init op is only used during initialization and it's not a target for
  // quantization.
  signature_def_map.erase(kSavedModelInitOpSignatureKey);
  return signature_def_map;
}

// Retrieves the function name -> function alias mapping from the
// `SavedModelBundle`.
// TODO: b/314124142 - Remove the need for this function.
absl::flat_hash_map<std::string, std::string> GetFunctionAliases(
    const SavedModelBundle& saved_model_bundle) {
  const protobuf::Map<std::string, std::string>& function_aliases =
      saved_model_bundle.meta_graph_def.meta_info_def().function_aliases();
  return absl::flat_hash_map<std::string, std::string>(function_aliases.begin(),
                                                       function_aliases.end());
}

}  // namespace

absl::StatusOr<mlir::ModuleOp> RunQuantization(
    const SavedModelBundle* saved_model_bundle,
    const absl::string_view saved_model_dir,
    const std::unordered_set<std::string>& saved_model_tags,
    const QuantizationConfig& quantization_config,
    const PyFunctionLibrary* quantization_py_function_lib,
    mlir::ModuleOp module_op) {
  if (saved_model_bundle == nullptr) {
    return absl::InvalidArgumentError(
        "Failed to run quantization. `saved_model_bundle` should not be "
        "nullptr.");
  }

  if (quantization_py_function_lib == nullptr) {
    return absl::InvalidArgumentError(
        "Failed to run quantization. `quantization_py_function_lib` should not "
        "be nullptr.");
  }

  const absl::flat_hash_map<std::string, SignatureDef> signature_def_map =
      GetSignatureDefMapFromBundle(*saved_model_bundle);

  std::vector<std::string> exported_names;
  for (const auto& [key, value_unused] : signature_def_map) {
    exported_names.push_back(key);
  }

  StaticRangePtqComponent static_range_ptq_component(
      module_op.getContext(), quantization_py_function_lib, saved_model_dir,
      /*signature_keys=*/exported_names, saved_model_tags, signature_def_map,
      GetFunctionAliases(*saved_model_bundle));
  const absl::StatusOr<mlir::ModuleOp> quantized_module_op =
      static_range_ptq_component.Run(module_op, quantization_config);
  if (!quantized_module_op.ok()) {
    return absl::InternalError("Failed to run quantization. Status msg: " +
                               quantized_module_op.status().ToString());
  }
  return quantized_module_op;
}

}  // namespace tensorflow
