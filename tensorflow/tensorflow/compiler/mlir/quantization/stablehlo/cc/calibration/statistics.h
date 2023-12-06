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
#ifndef TENSORFLOW_COMPILER_MLIR_QUANTIZATION_STABLEHLO_CC_CALIBRATION_STATISTICS_H_
#define TENSORFLOW_COMPILER_MLIR_QUANTIZATION_STABLEHLO_CC_CALIBRATION_STATISTICS_H_

#include "absl/status/status.h"
#include "tensorflow/compiler/mlir/quantization/tensorflow/python/py_function_lib.h"
#include "tensorflow/compiler/mlir/quantization/tensorflow/quantization_options.pb.h"
#include "tensorflow/core/framework/graph.pb.h"

namespace stablehlo::quantization {

// Adds calibrated min / max values to CustomAggregator nodes in `graph_def`.
// The min and max values will be added to the "min" and "max" attributes,
// respectively. `calibration_options` provides the strategy to retrieve min and
// max values.
absl::Status AddCalibrationStatistics(
    tensorflow::GraphDef& graph_def,
    const tensorflow::quantization::CalibrationOptions& calibration_options,
    const tensorflow::quantization::PyFunctionLibrary& py_function_library);

}  // namespace stablehlo::quantization

#endif  // TENSORFLOW_COMPILER_MLIR_QUANTIZATION_STABLEHLO_CC_CALIBRATION_STATISTICS_H_
