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
#include "tensorflow/compiler/mlir/quantization/stablehlo/cc/calibration/component.h"

#include <string>
#include <unordered_set>
#include <utility>
#include <vector>

#include "absl/base/nullability.h"
#include "absl/container/flat_hash_map.h"
#include "absl/log/die_if_null.h"
#include "absl/log/log.h"
#include "absl/status/status.h"
#include "absl/status/statusor.h"
#include "absl/strings/str_cat.h"
#include "absl/strings/string_view.h"
#include "absl/types/span.h"
#include "mlir/IR/BuiltinOps.h"  // from @llvm-project
#include "mlir/IR/MLIRContext.h"  // from @llvm-project
#include "mlir/IR/OwningOpRef.h"  // from @llvm-project
#include "mlir/Pass/PassManager.h"  // from @llvm-project
#include "mlir/Support/LLVM.h"  // from @llvm-project
#include "tensorflow/compiler/mlir/quantization/stablehlo/cc/calibration/representative_dataset.h"
#include "tensorflow/compiler/mlir/quantization/stablehlo/cc/calibration/statistics.h"
#include "tensorflow/compiler/mlir/quantization/stablehlo/cc/debugger.h"
#include "tensorflow/compiler/mlir/quantization/stablehlo/cc/io.h"
#include "tensorflow/compiler/mlir/quantization/stablehlo/cc/saved_model_export.h"
#include "tensorflow/compiler/mlir/quantization/stablehlo/cc/types.h"
#include "tensorflow/compiler/mlir/quantization/stablehlo/quantization_config.pb.h"
#include "tensorflow/compiler/mlir/quantization/tensorflow/exported_model.pb.h"
#include "tensorflow/compiler/mlir/quantization/tensorflow/python/py_function_lib.h"
#include "tensorflow/compiler/mlir/quantization/tensorflow/quantization_options.pb.h"
#include "tensorflow/core/protobuf/meta_graph.pb.h"
#include "tsl/platform/statusor.h"

namespace mlir::quant::stablehlo {

using ::stablehlo::quantization::AddCalibrationStatistics;
using ::stablehlo::quantization::CreateRepresentativeDatasetFileMap;
using ::stablehlo::quantization::DisableDebugging;
using ::stablehlo::quantization::QuantizationConfig;
using ::stablehlo::quantization::RepresentativeDatasetConfig;
using ::stablehlo::quantization::io::CreateTmpDir;
using ::stablehlo::quantization::io::GetLocalTmpFileName;
using ::tensorflow::AssetFileDef;
using ::tensorflow::SignatureDef;
using ::tensorflow::quantization::ExportedModel;
using ::tensorflow::quantization::PyFunctionLibrary;

CalibrationComponent::CalibrationComponent(
    absl::Nonnull<MLIRContext*> ctx,
    absl::Nonnull<const PyFunctionLibrary*> py_function_lib,
    const absl::string_view src_saved_model_path,
    absl::flat_hash_map<FunctionName, FunctionAlias> function_aliases,
    std::unordered_set<std::string> tags,
    absl::flat_hash_map<std::string, SignatureDef> signature_def_map,
    std::vector<std::string> signature_keys)
    : ctx_(ABSL_DIE_IF_NULL(ctx)),                          // Crash OK
      py_function_lib_(ABSL_DIE_IF_NULL(py_function_lib)),  // Crash OK
      src_saved_model_path_(src_saved_model_path),
      function_aliases_(std::move(function_aliases)),
      tags_(std::move(tags)),
      signature_def_map_(std::move(signature_def_map)),
      signature_keys_(std::move(signature_keys)) {}

absl::StatusOr<ExportedModel> CalibrationComponent::ExportToSavedModel(
    ModuleOp module_op, const absl::string_view dst_saved_model_path) {
  TF_ASSIGN_OR_RETURN(const std::string checkpoint_dir, GetLocalTmpFileName());

  // Clone ModuleOp and function aliases so changes in this pipeline won't
  // be reflected in the original values.
  mlir::OwningOpRef<mlir::ModuleOp> cloned_module_ref(module_op.clone());

  // Disable DumpTensor ops when running calibration.
  DisableDebugging(*cloned_module_ref);

  // `duplicate_shape_determining_constants = false` because the
  // resulting graph of this step is not expected to be loaded on TPU.
  const ExportOptions export_opts = {
      /*duplicate_shape_determining_constants=*/false,
      /*unfreeze_constants=*/false, checkpoint_dir,
      /*debug_name=*/absl::StrCat(kName, kExportStepSuffix)};

  TF_ASSIGN_OR_RETURN(const SmallVector<AssetFileDef> asset_file_defs,
                      RunExportPasses(export_opts, *ctx_, *cloned_module_ref));

  TF_ASSIGN_OR_RETURN(ExportedModel exported_model,
                      ConvertMlirModuleToExportedModel(
                          *cloned_module_ref, checkpoint_dir, function_aliases_,
                          {asset_file_defs.begin(), asset_file_defs.end()}));

  py_function_lib_->SaveExportedModel(dst_saved_model_path, exported_model,
                                      src_saved_model_path_, tags_,
                                      signature_def_map_);

  return exported_model;
}

absl::StatusOr<ModuleOp> CalibrationComponent::Run(
    ModuleOp module_op, const QuantizationConfig& config) {
  // Exports the pre-calibrated model to SavedModel.
  TF_ASSIGN_OR_RETURN(const std::string precalibrated_saved_model_dir,
                      CreateTmpDir());

  TF_ASSIGN_OR_RETURN(
      ExportedModel exported_model,
      ExportToSavedModel(module_op, precalibrated_saved_model_dir));

  // Translates `RepresentativeDatasetConfig`s to signature key ->
  // `RepresentativeDatasetFile` mapping.
  const auto dataset_configs =
      config.static_range_ptq_preset().representative_datasets();
  const std::vector<RepresentativeDatasetConfig> dataset_config_vector(
      dataset_configs.begin(), dataset_configs.end());
  TF_ASSIGN_OR_RETURN(
      const auto representative_dataset_file_map,
      CreateRepresentativeDatasetFileMap(dataset_config_vector));

  // Runs calibration on the exported model. The statistics will be stored in a
  // separate singleton object `CalibratorSingleton` and are directly added to
  // `exported_model` without re-importing it.
  py_function_lib_->RunCalibration(
      precalibrated_saved_model_dir, signature_keys_, tags_,
      config.calibration_options(),
      /*force_graph_mode_calibration=*/true, representative_dataset_file_map);

  if (absl::Status status = AddCalibrationStatistics(
          module_op, config.calibration_options(), *py_function_lib_);
      !status.ok()) {
    LOG(WARNING) << "Some CustomAggregator ops do not have min or max "
                    "values. Parts of the graph are not quantized. "
                 << status;
  }

  return module_op;
}

}  // namespace mlir::quant::stablehlo
