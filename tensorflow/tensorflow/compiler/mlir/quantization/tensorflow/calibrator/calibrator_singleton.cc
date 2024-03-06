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
#include "tensorflow/compiler/mlir/quantization/tensorflow/calibrator/calibrator_singleton.h"

#include <cstdint>
#include <memory>
#include <optional>
#include <string>
#include <vector>

#include "absl/base/attributes.h"
#include "absl/base/const_init.h"
#include "absl/strings/string_view.h"
#include "absl/synchronization/mutex.h"
#include "absl/types/span.h"
#include "tensorflow/compiler/mlir/quantization/tensorflow/calibrator/calibration_statistics.pb.h"
#include "tensorflow/compiler/mlir/quantization/tensorflow/calibrator/calibration_statistics_collector_average_min_max.h"
#include "tensorflow/compiler/mlir/quantization/tensorflow/calibrator/calibration_statistics_collector_histogram.h"
#include "tensorflow/compiler/mlir/quantization/tensorflow/calibrator/calibration_statistics_collector_min_max.h"
#include "tensorflow/core/framework/tensor.h"

namespace tensorflow {
namespace calibrator {

ABSL_CONST_INIT absl::Mutex CalibratorSingleton::lock_(absl::kConstInit);

CalibratorSingleton& CalibratorSingleton::GetInstance() {
  static CalibratorSingleton* calibrator = new CalibratorSingleton();
  return *calibrator;
}

void CalibratorSingleton::ClearCollectedInformation() {
  absl::MutexLock lock(&lock_);

  CalibratorSingleton& instance = GetInstance();
  instance.id_to_collector_.clear();
}

void CalibratorSingleton::ClearData(absl::string_view id) {
  absl::MutexLock lock(&lock_);

  CalibratorSingleton& instance = GetInstance();

  const std::string id_str{id};
  instance.id_to_collector_[id_str].reset(nullptr);
}

void CalibratorSingleton::Report(absl::string_view id,
                                 absl::Span<float> data_span,
                                 const CalibrationOptions& calib_opts) {
  absl::MutexLock lock(&lock_);

  CalibratorSingleton& instance = GetInstance();

  const std::string id_str{id};
  AssignIfNotExists(id_str, calib_opts);
  instance.id_to_collector_[id_str]->Collect(data_span);
}

void CalibratorSingleton::Report(absl::string_view id,
                                 const std::vector<float>& data_vec,
                                 const CalibrationOptions& calib_opts) {
  absl::MutexLock lock(&lock_);

  CalibratorSingleton& instance = GetInstance();

  const std::string id_str{id};
  AssignIfNotExists(id_str, calib_opts);
  instance.id_to_collector_[id_str]->Collect(data_vec);
}

void CalibratorSingleton::Report(absl::string_view id,
                                 const Tensor& data_tensor,
                                 const CalibrationOptions& calib_opts) {
  absl::MutexLock lock(&lock_);

  CalibratorSingleton& instance = GetInstance();

  const std::string id_str{id};
  AssignIfNotExists(id_str, calib_opts);
  instance.id_to_collector_[id_str]->Collect(data_tensor);
}

std::optional<CalibrationStatistics> CalibratorSingleton::GetStatistics(
    absl::string_view id) {
  absl::MutexLock lock(&lock_);

  CalibratorSingleton& instance = GetInstance();

  const std::string id_str{id};

  if (!instance.id_to_collector_[id_str]) {
    return std::nullopt;
  }

  return instance.id_to_collector_[id_str]->GetStatistics();
}

int64_t CalibratorSingleton::IssueNewId() {
  CalibratorSingleton& instance = GetInstance();
  return instance.next_id_++;
}

void CalibratorSingleton::AssignIfNotExists(
    std::string id_str, const CalibrationOptions& calib_opts) {
  CalibratorSingleton& instance = GetInstance();

  if (!instance.id_to_collector_[id_str]) {
    CalibrationOptions::CalibrationMethod calib_method =
        calib_opts.calibration_method();

    switch (calib_method) {
      case CalibrationOptions::CALIBRATION_METHOD_AVERAGE_MIN_MAX:
        instance.id_to_collector_[id_str] =
            std::make_unique<CalibrationStatisticsCollectorAverageMinMax>();
        break;
      case CalibrationOptions::CALIBRATION_METHOD_HISTOGRAM_PERCENTILE:
      case CalibrationOptions::CALIBRATION_METHOD_HISTOGRAM_MSE_BRUTEFORCE:
      case CalibrationOptions::CALIBRATION_METHOD_HISTOGRAM_MSE_SYMMETRIC:
      case CalibrationOptions::CALIBRATION_METHOD_HISTOGRAM_MSE_MAX_FREQUENCY:
        instance.id_to_collector_[id_str] =
            std::make_unique<CalibrationStatisticsCollectorHistogram>(
                calib_opts);
        break;
      case CalibrationOptions::CALIBRATION_METHOD_MIN_MAX:
      default:
        instance.id_to_collector_[id_str] =
            std::make_unique<CalibrationStatisticsCollectorMinMax>();
    }
  }
}

}  // namespace calibrator
}  // namespace tensorflow
