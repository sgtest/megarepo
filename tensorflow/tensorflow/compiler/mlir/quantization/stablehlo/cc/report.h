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
#ifndef TENSORFLOW_COMPILER_MLIR_QUANTIZATION_STABLEHLO_CC_REPORT_H_
#define TENSORFLOW_COMPILER_MLIR_QUANTIZATION_STABLEHLO_CC_REPORT_H_

#include "tensorflow/compiler/mlir/quantization/stablehlo/quantization_config.pb.h"

namespace mlir::quant::stablehlo {

// A class that manages information about `QuantizableUnit`s post-quantization,
// internally in the form of `QuantizationUnits`. It is used to collect
// quantization summary from a quantized `ModuleOp` and emit it in a human- and
// machine-readable format.
class QuantizationReport {
 public:
  QuantizationReport() = default;

  // Adds a `QuantizationResult` to the report.
  void AddQuantizationResult(
      ::stablehlo::quantization::QuantizationResult&& result);

  // Returns `QuantizationResults` that are registered in this report.
  const ::stablehlo::quantization::QuantizationResults& GetQuantizationResults()
      const {
    return quantization_results_;
  }

 private:
  // Quantization results that are registered in this report. A quantization
  // result may be added manually by calling `AddQuantizationResult`.
  ::stablehlo::quantization::QuantizationResults quantization_results_;
};

}  // namespace mlir::quant::stablehlo

#endif  // TENSORFLOW_COMPILER_MLIR_QUANTIZATION_STABLEHLO_CC_REPORT_H_
