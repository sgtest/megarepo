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
#include "tensorflow/compiler/mlir/quantization/tensorflow/python/quantize_model.h"

#include <memory>
#include <optional>
#include <string>
#include <unordered_set>
#include <utility>
#include <vector>

#include "absl/algorithm/container.h"
#include "absl/container/flat_hash_map.h"
#include "absl/container/flat_hash_set.h"
#include "absl/log/log.h"
#include "absl/status/status.h"
#include "absl/status/statusor.h"
#include "absl/strings/str_cat.h"
#include "absl/strings/string_view.h"
#include "absl/types/span.h"
#include "llvm/ADT/SmallVector.h"
#include "mlir/IR/BuiltinOps.h"  // from @llvm-project
#include "mlir/IR/OwningOpRef.h"  // from @llvm-project
#include "mlir/Pass/PassManager.h"  // from @llvm-project
#include "mlir/Support/LogicalResult.h"  // from @llvm-project
#include "tensorflow/cc/saved_model/loader.h"
#include "tensorflow/compiler/mlir/quantization/stablehlo/cc/context.h"
#include "tensorflow/compiler/mlir/quantization/stablehlo/cc/io.h"
#include "tensorflow/compiler/mlir/quantization/stablehlo/cc/post_calibration.h"
#include "tensorflow/compiler/mlir/quantization/stablehlo/cc/pre_calibration.h"
#include "tensorflow/compiler/mlir/quantization/stablehlo/cc/saved_model_export.h"
#include "tensorflow/compiler/mlir/quantization/stablehlo/cc/saved_model_import.h"
#include "tensorflow/compiler/mlir/quantization/stablehlo/quantization_config.pb.h"
#include "tensorflow/compiler/mlir/quantization/tensorflow/cc/convert_asset_args.h"
#include "tensorflow/compiler/mlir/quantization/tensorflow/cc/run_passes.h"
#include "tensorflow/compiler/mlir/quantization/tensorflow/exported_model.pb.h"
#include "tensorflow/compiler/mlir/quantization/tensorflow/python/unfreeze_constants.h"
#include "tensorflow/compiler/mlir/quantization/tensorflow/quantization_options.pb.h"
#include "tensorflow/compiler/mlir/quantization/tensorflow/quantize_passes.h"
#include "tensorflow/compiler/mlir/quantization/tensorflow/quantize_preprocess.h"
#include "tensorflow/compiler/mlir/tensorflow/translate/mlir_import_options.h"
#include "tensorflow/compiler/mlir/tensorflow/translate/tf_mlir_translate.h"
#include "tensorflow/core/framework/graph.pb.h"
#include "tensorflow/core/framework/node_def.pb.h"
#include "tensorflow/core/platform/statusor.h"
#include "tensorflow/core/protobuf/meta_graph.pb.h"
#include "tensorflow/core/protobuf/saver.pb.h"
#include "tsl/platform/errors.h"
#include "tsl/platform/statusor.h"

namespace tensorflow {
namespace quantization {
namespace {

using ::mlir::quant::stablehlo::AddExportPasses;
using ::mlir::quant::stablehlo::ConvertMlirModuleToExportedModel;
using ::mlir::quant::stablehlo::CreateMlirContextForQuantization;
using ::mlir::quant::stablehlo::ExportOptions;
using ::mlir::quant::stablehlo::FunctionAlias;
using ::mlir::quant::stablehlo::FunctionName;
using ::mlir::quant::stablehlo::GetFunctionAliases;
using ::mlir::quant::stablehlo::kExportStepSuffix;
using ::mlir::quant::stablehlo::PostCalibrationComponent;
using ::mlir::quant::stablehlo::PreCalibrationComponent;
using ::mlir::quant::stablehlo::UpdateFunctionAliases;
using ::stablehlo::quantization::DebuggerConfig;
using ::stablehlo::quantization::QuantizationConfig;
using ::stablehlo::quantization::io::GetLocalTmpFileName;

absl::StatusOr<mlir::OwningOpRef<mlir::ModuleOp>> ImportAndPreprocessSavedModel(
    absl::string_view saved_model_path,
    const std::vector<std::string> &signature_keys,
    const std::unordered_set<std::string> &tags, mlir::MLIRContext *context,
    const bool is_inliner_run, const bool run_tf_to_stablehlo,
    const bool deserialize_xla_call_module,
    absl::flat_hash_map<std::string, std::string> &function_aliases) {
  // Convert the SavedModelBundle to an MLIR module.
  MLIRImportOptions import_options;
  import_options.upgrade_legacy = true;
  import_options.lift_variables = false;
  import_options.include_variables_in_initializers = true;
  auto bundle = std::make_unique<SavedModelBundle>();

  // TODO: b/213406917 - Add support for the object graph based saved model.
  std::vector<std::string> exported_names = signature_keys;
  absl::StatusOr<mlir::OwningOpRef<mlir::ModuleOp>> module =
      SavedModelSignatureDefsToMlirImport(saved_model_path, tags,
                                          absl::MakeSpan(exported_names),
                                          context, import_options, &bundle);
  if (!module.status().ok()) {
    return absl::InternalError(absl::StrCat("Failed to import SavedModel: ",
                                            module.status().message()));
  }

  mlir::OwningOpRef<mlir::ModuleOp> module_ref = std::move(module).value();
  UpdateFunctionAliases(function_aliases, *module_ref);

  // Collect the names of the functions that have aliases so that they may not
  // be inlined.
  absl::flat_hash_set<std::string> aliased_function_names;
  absl::c_for_each(function_aliases, [&](const auto &aliases) {
    return aliased_function_names.insert(aliases.first);
  });

  TF_RETURN_IF_ERROR(PreprocessAndFreezeGraph(
      /*mlir_dump_file_prefix=*/kDefaultTfQuantMlirDumpFilePrefix,
      /*is_inliner_run=*/is_inliner_run,
      /*noinline_functions=*/aliased_function_names, module_ref.get(), context,
      bundle ? bundle->GetSession() : nullptr, run_tf_to_stablehlo,
      deserialize_xla_call_module));
  return module_ref;
}

absl::StatusOr<ExportedModel> ModuleOpToExportedModel(
    mlir::ModuleOp module_op, mlir::MLIRContext *ctx,
    absl::string_view step_name, const bool unfreeze_constants,
    const absl::flat_hash_map<std::string, std::string> &function_aliases) {
  TF_ASSIGN_OR_RETURN(const std::string checkpoint_dir, GetLocalTmpFileName());

  const auto export_opts =
      ExportOptions{/*duplicate_shape_determining_constants=*/true,
                    unfreeze_constants, checkpoint_dir,
                    /*debug_name=*/absl::StrCat(step_name, kExportStepSuffix)};

  TF_ASSIGN_OR_RETURN(const llvm::SmallVector<AssetFileDef> asset_file_defs,
                      RunExportPasses(export_opts, *ctx, module_op));

  return ConvertMlirModuleToExportedModel(
      module_op, checkpoint_dir, function_aliases,
      {asset_file_defs.begin(), asset_file_defs.end()});
}

}  // namespace

absl::StatusOr<ExportedModel> QuantizeQatModel(
    const absl::string_view saved_model_path,
    const std::vector<std::string> &signature_keys,
    const std::unordered_set<std::string> &tags,
    const QuantizationOptions &quantization_options) {
  // Convert the SavedModelBundle to an MLIR module.
  std::unique_ptr<mlir::MLIRContext> context =
      CreateMlirContextForQuantization();

  absl::StatusOr<absl::flat_hash_map<FunctionName, FunctionAlias>>
      function_aliases = GetFunctionAliases(saved_model_path, tags);
  if (!function_aliases.ok()) {
    return absl::InternalError(absl::StrCat(
        "Failed to get function alias: ", function_aliases.status().message()));
  }

  absl::StatusOr<mlir::OwningOpRef<mlir::ModuleOp>> module =
      ImportAndPreprocessSavedModel(
          saved_model_path, signature_keys, tags, context.get(),
          /*is_inliner_run=*/true,
          /*run_tf_to_stablehlo=*/false,
          /*deserialize_xla_call_module=*/false, *function_aliases);
  if (!module.status().ok()) {
    return absl::InternalError(
        absl::StrCat("Failed to import and preprocess SavedModel: ",
                     module.status().message()));
  }
  mlir::OwningOpRef<mlir::ModuleOp> module_ref = std::move(module).value();

  TF_RETURN_IF_ERROR(RunPasses(
      /*name=*/
      kTfQuantQatStepName, /*add_passes_func=*/
      [&quantization_options](mlir::PassManager &pm) {
        AddQuantizeQatPasses(pm, quantization_options, kTfQuantQatStepName);
      },
      *context, *module_ref));

  return ModuleOpToExportedModel(
      *module_ref, context.get(), kTfQuantQatStepName,
      /*unfreeze_constants=*/!quantization_options.freeze_all_variables(),
      *function_aliases);
}

absl::StatusOr<ExportedModel> QuantizePtqModelPreCalibration(
    const absl::string_view saved_model_path,
    const std::vector<std::string> &signature_keys,
    const std::unordered_set<std::string> &tags,
    const QuantizationOptions &quantization_options) {
  // Convert the SavedModelBundle to an MLIR module.
  std::unique_ptr<mlir::MLIRContext> context =
      CreateMlirContextForQuantization();

  absl::StatusOr<absl::flat_hash_map<FunctionName, FunctionAlias>>
      function_aliases = GetFunctionAliases(saved_model_path, tags);
  if (!function_aliases.ok()) {
    return absl::InternalError(absl::StrCat(
        "Failed to get function alias: ", function_aliases.status().message()));
  }

  const bool is_stablehlo = quantization_options.op_set() == OpSet::STABLEHLO;
  absl::StatusOr<mlir::OwningOpRef<mlir::ModuleOp>> module =
      ImportAndPreprocessSavedModel(
          saved_model_path, signature_keys, tags, context.get(),
          /*is_inliner_run=*/true,
          /*run_tf_to_stablehlo=*/is_stablehlo,
          /*deserialize_xla_call_module=*/false, *function_aliases);
  if (!module.status().ok()) {
    return absl::InternalError(
        absl::StrCat("Failed to import and preprocess SavedModel: ",
                     module.status().message()));
  }
  mlir::OwningOpRef<mlir::ModuleOp> module_ref = std::move(module).value();

  // Use StableHLO Quantizer option if opset is specified.
  if (is_stablehlo) {
    QuantizationConfig quantization_config;
    *quantization_config.mutable_debugger_config() =
        quantization_options.debugger_config();
    PreCalibrationComponent pre_calibration_component(context.get());
    TF_ASSIGN_OR_RETURN(*module_ref, pre_calibration_component.Run(
                                         *module_ref, quantization_config));
  } else {
    TF_RETURN_IF_ERROR(RunPasses(
        /*name=*/
        kTfQuantPtqPreCalibrationStepName, /*add_passes_func=*/
        [&quantization_options](mlir::PassManager &pm) {
          AddQuantizePtqPreCalibrationPasses(pm, quantization_options);
        },
        *context, *module_ref));
  }

  return ModuleOpToExportedModel(
      *module_ref, context.get(), kTfQuantPtqPreCalibrationStepName,
      /*unfreeze_constants=*/!quantization_options.freeze_all_variables(),
      *function_aliases);
}

absl::StatusOr<ExportedModel> QuantizePtqModelPostCalibration(
    const absl::string_view saved_model_path,
    const std::vector<std::string> &signature_keys,
    const std::unordered_set<std::string> &tags,
    const QuantizationOptions &quantization_options) {
  std::unique_ptr<mlir::MLIRContext> context =
      CreateMlirContextForQuantization();

  absl::StatusOr<absl::flat_hash_map<FunctionName, FunctionAlias>>
      function_aliases = GetFunctionAliases(saved_model_path, tags);
  if (!function_aliases.ok()) {
    return absl::InternalError(absl::StrCat(
        "Failed to get function alias: ", function_aliases.status().message()));
  }

  const bool is_stablehlo = quantization_options.op_set() == OpSet::STABLEHLO;
  // Freezing is required again since variables might have been produced during
  // the pre-calibration step. `is_inliner_run = false` to prevent the functions
  // lifted for quantization from being inlined.
  absl::StatusOr<mlir::OwningOpRef<mlir::ModuleOp>> module =
      ImportAndPreprocessSavedModel(
          saved_model_path, signature_keys, tags, context.get(),
          /*is_inliner_run=*/false,
          /*run_tf_to_stablehlo=*/false,
          /*deserialize_xla_call_module=*/is_stablehlo, *function_aliases);
  if (!module.status().ok()) {
    return absl::InternalError(
        absl::StrCat("Failed to import and preprocess SavedModel: ",
                     module.status().message()));
  }
  mlir::OwningOpRef<mlir::ModuleOp> module_ref = std::move(module).value();

  // Use StableHLO Quantizer option if opset is specified.
  if (is_stablehlo) {
    QuantizationConfig quantization_config{};
    quantization_config.mutable_static_range_ptq_preset()
        ->set_enable_per_channel_quantized_weight(
            quantization_options.enable_per_channel_quantization());
    // When targeting server TPUs quantized types should be unpacked into
    // integer ops.
    quantization_config.mutable_pipeline_config()->set_unpack_quantized_types(
        true);

    PostCalibrationComponent post_calibration_component(context.get());
    TF_ASSIGN_OR_RETURN(*module_ref, post_calibration_component.Run(
                                         *module_ref, quantization_config));
  } else {
    TF_RETURN_IF_ERROR(RunPasses(
        /*name=*/
        kTfQuantPtqPostCalibrationStepName, /*add_passes_func=*/
        [&quantization_options](mlir::PassManager &pm) {
          AddQuantizePtqPostCalibrationPasses(
              pm, quantization_options, kTfQuantPtqPostCalibrationStepName);
        },
        *context, *module_ref));
  }

  return ModuleOpToExportedModel(
      *module_ref, context.get(), kTfQuantPtqPostCalibrationStepName,
      /*unfreeze_constants=*/!quantization_options.freeze_all_variables(),
      *function_aliases);
}

absl::StatusOr<ExportedModel> QuantizePtqDynamicRange(
    const absl::string_view saved_model_path,
    const std::vector<std::string> &signature_keys,
    const std::unordered_set<std::string> &tags,
    const QuantizationOptions &quantization_options) {
  std::unique_ptr<mlir::MLIRContext> context =
      CreateMlirContextForQuantization();

  absl::StatusOr<absl::flat_hash_map<FunctionName, FunctionAlias>>
      function_aliases = GetFunctionAliases(saved_model_path, tags);
  if (!function_aliases.ok()) {
    return absl::InternalError(absl::StrCat(
        "Failed to get function alias: ", function_aliases.status().message()));
  }

  absl::StatusOr<mlir::OwningOpRef<mlir::ModuleOp>> module =
      ImportAndPreprocessSavedModel(
          saved_model_path, signature_keys, tags, context.get(),
          /*is_inliner_run=*/true,
          /*run_tf_to_stablehlo=*/false, /*deserialize_xla_call_module=*/false,
          *function_aliases);

  if (!module.status().ok()) {
    return absl::InternalError(
        absl::StrCat("Failed to import and preprocess SavedModel: ",
                     module.status().message()));
  }

  mlir::OwningOpRef<mlir::ModuleOp> module_ref = std::move(module).value();

  TF_RETURN_IF_ERROR(RunPasses(
      /*name=*/
      kTfQuantPtqDynamicRangeStepName, /*add_passes_func=*/
      [&quantization_options](mlir::PassManager &pm) {
        AddQuantizePtqDynamicRangePasses(pm, quantization_options,
                                         kTfQuantPtqDynamicRangeStepName);
      },
      *context, *module_ref));

  return ModuleOpToExportedModel(
      *module_ref, context.get(), kTfQuantPtqDynamicRangeStepName,
      /*unfreeze_constants=*/!quantization_options.freeze_all_variables(),
      *function_aliases);
}

// TODO: b/297626257 - [Converter Component][TF-Quantizer] Clean up
// quantize_model.cc by factoring out repeated codes
absl::StatusOr<ExportedModel> QuantizeWeightOnly(
    const absl::string_view saved_model_path,
    const QuantizationOptions &quantization_options) {
  std::unique_ptr<mlir::MLIRContext> context =
      CreateMlirContextForQuantization();

  absl::StatusOr<absl::flat_hash_map<FunctionName, FunctionAlias>>
      function_aliases = GetFunctionAliases(
          saved_model_path, {quantization_options.tags().begin(),
                             quantization_options.tags().end()});
  if (!function_aliases.ok()) {
    return absl::InternalError(absl::StrCat(
        "Failed to get function alias: ", function_aliases.status().message()));
  }

  absl::StatusOr<mlir::OwningOpRef<mlir::ModuleOp>> module =
      ImportAndPreprocessSavedModel(
          saved_model_path,
          {quantization_options.signature_keys().begin(),
           quantization_options.signature_keys().end()},
          {quantization_options.tags().begin(),
           quantization_options.tags().end()},
          context.get(), /*is_inliner_run=*/true, /*run_tf_to_stablehlo=*/false,
          /*deserialize_xla_call_module=*/false, *function_aliases);

  if (!module.status().ok()) {
    return absl::InternalError(
        absl::StrCat("Failed to import and preprocess SavedModel: ",
                     module.status().message()));
  }

  mlir::OwningOpRef<mlir::ModuleOp> module_ref = std::move(module).value();

  TF_RETURN_IF_ERROR(RunPasses(
      kTfQuantWeightOnlyStepName,
      /*add_passes_func=*/
      [&quantization_options](mlir::PassManager &pm) {
        AddQuantizeWeightOnlyPasses(pm, quantization_options,
                                    kTfQuantWeightOnlyStepName);
      },
      *context, *module_ref));

  return ModuleOpToExportedModel(
      *module_ref, context.get(), kTfQuantWeightOnlyStepName,
      /*unfreeze_constants=*/!quantization_options.freeze_all_variables(),
      *function_aliases);
}

}  // namespace quantization
}  // namespace tensorflow
