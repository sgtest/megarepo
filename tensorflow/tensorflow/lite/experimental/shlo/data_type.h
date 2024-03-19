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

#ifndef TENSORFLOW_LITE_EXPERIMENTAL_SHLO_DATA_TYPE_H_
#define TENSORFLOW_LITE_EXPERIMENTAL_SHLO_DATA_TYPE_H_

#include <cstdint>
#include <limits>

#include "tensorflow/lite/experimental/shlo/bf16.h"
#include "tensorflow/lite/experimental/shlo/f16.h"

namespace shlo_ref {

// For more information on StableHLO types, see the spec., search for "Element
// types". The SHLO Device Profile does not include unsigned or 64 bit types.
enum class DataType {
  kI1,
  kSI4,
  kSI8,
  kSI16,
  kSI32,
  kBF16,
  kF16,
  kF32,
};

// Storage provides the corresponding C++ type for the given DataType.
template <DataType data_type>
struct Storage {};

template <>
struct Storage<DataType::kI1> {
  using Type = bool;
  static constexpr Type kMinValue = std::numeric_limits<Type>::lowest();
  static constexpr Type kMaxValue = std::numeric_limits<Type>::max();
};
template <>
struct Storage<DataType::kSI4> {
  using Type = int8_t;
  static constexpr Type kMinValue = -8;
  static constexpr Type kMaxValue = 7;
};
template <>
struct Storage<DataType::kSI8> {
  using Type = int8_t;
  static constexpr Type kMinValue = std::numeric_limits<Type>::lowest();
  static constexpr Type kMaxValue = std::numeric_limits<Type>::max();
};
template <>
struct Storage<DataType::kSI16> {
  using Type = int16_t;
  static constexpr Type kMinValue = std::numeric_limits<Type>::lowest();
  static constexpr Type kMaxValue = std::numeric_limits<Type>::max();
};
template <>
struct Storage<DataType::kSI32> {
  using Type = int32_t;
  static constexpr Type kMinValue = std::numeric_limits<Type>::lowest();
  static constexpr Type kMaxValue = std::numeric_limits<Type>::max();
};
template <>
struct Storage<DataType::kBF16> {
  using Type = BF16;
  static constexpr Type kMinValue = std::numeric_limits<Type>::lowest();
  static constexpr Type kMaxValue = std::numeric_limits<Type>::max();
};
template <>
struct Storage<DataType::kF16> {
  using Type = F16;
  static constexpr Type kMinValue = std::numeric_limits<Type>::lowest();
  static constexpr Type kMaxValue = std::numeric_limits<Type>::max();
};
template <>
struct Storage<DataType::kF32> {
  using Type = float;
  static constexpr Type kMinValue = std::numeric_limits<Type>::lowest();
  static constexpr Type kMaxValue = std::numeric_limits<Type>::max();
};

template <DataType data_type>
using StorageType = typename Storage<data_type>::Type;

constexpr bool IsBool(DataType data_type) { return data_type == DataType::kI1; }

constexpr bool IsSignedInteger(DataType data_type) {
  return data_type == DataType::kSI4 || data_type == DataType::kSI8 ||
         data_type == DataType::kSI16 || data_type == DataType::kSI32;
}

constexpr bool IsUnsignedInteger(DataType data_type) { return false; }

constexpr bool IsInteger(DataType data_type) {
  return IsSignedInteger(data_type) || IsUnsignedInteger(data_type);
}

constexpr bool IsFloat(DataType data_type) {
  return data_type == DataType::kBF16 || data_type == DataType::kF16 ||
         data_type == DataType::kF32;
}

template <DataType data_type>
constexpr int64_t SizeOf() {
  return sizeof(StorageType<data_type>);
}

constexpr int64_t SizeOf(DataType data_type) {
  switch (data_type) {
    case DataType::kI1:
      return SizeOf<DataType::kI1>();
    case DataType::kSI4:
      return SizeOf<DataType::kSI4>();
    case DataType::kSI8:
      return SizeOf<DataType::kSI8>();
    case DataType::kSI16:
      return SizeOf<DataType::kSI16>();
    case DataType::kSI32:
      return SizeOf<DataType::kSI32>();
    case DataType::kBF16:
      return SizeOf<DataType::kBF16>();
    case DataType::kF16:
      return SizeOf<DataType::kF16>();
    case DataType::kF32:
      return SizeOf<DataType::kF32>();
      break;
  }
}

}  // namespace shlo_ref

#endif  // TENSORFLOW_LITE_EXPERIMENTAL_SHLO_DATA_TYPE_H_
