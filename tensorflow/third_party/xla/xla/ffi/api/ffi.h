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

#ifndef XLA_FFI_API_FFI_H_
#define XLA_FFI_API_FFI_H_

#ifdef TENSORFLOW_COMPILER_XLA_FFI_FFI_H_
#error Two different XLA FFI implementations cannot be included together
#endif  // XLA_FFI_API_H_

#include <cstddef>
#include <cstdint>
#include <limits>
#include <optional>
#include <ostream>
#include <string>
#include <type_traits>
#include <utility>
#include <vector>

#include "xla/ffi/api/c_api.h"

// IWYU pragma: begin_exports
#include "xla/ffi/api/api.h"
// IWYU pragma: end_exports

namespace xla::ffi {

enum class DataType : uint8_t {
  INVALID = XLA_FFI_DataType_INVALID,
  PRED = XLA_FFI_DataType_PRED,
  S8 = XLA_FFI_DataType_S8,
  S16 = XLA_FFI_DataType_S16,
  S32 = XLA_FFI_DataType_S32,
  S64 = XLA_FFI_DataType_S64,
  U8 = XLA_FFI_DataType_U8,
  U16 = XLA_FFI_DataType_U16,
  U32 = XLA_FFI_DataType_U32,
  U64 = XLA_FFI_DataType_U64,
  F16 = XLA_FFI_DataType_F16,
  F32 = XLA_FFI_DataType_F32,
  F64 = XLA_FFI_DataType_F64,
  BF16 = XLA_FFI_DataType_BF16,
};

inline std::ostream& operator<<(std::ostream& os, const DataType dtype) {
  static constexpr const char* kDataTypeNames[] = {
      "PRED", "S8",  "S16", "S32", "S64", "U8",   "U16",
      "U32",  "U64", "F16", "F32", "F64", "BF16",
  };
  return os << kDataTypeNames[static_cast<int>(dtype)];
}

//===----------------------------------------------------------------------===//
// Span is non-owning view into contiguous values of type `T`.
//===----------------------------------------------------------------------===//

// TODO(ezhulenev): Replace with `std::span` when C++20 is available.
template <typename T>
class Span {
 public:
  constexpr Span() : data_(nullptr), size_(0) {}

  Span(T* data, size_t size) : data_(data), size_(size) {}
  Span(const std::vector<std::remove_const_t<T>>& vec)  // NOLINT
      : Span(vec.data(), vec.size()) {}

  T& operator[](size_t index) const { return data_[index]; }

  size_t size() const { return size_; }

  T* begin() const { return data_; }
  T* end() const { return data_ + size_; }

 private:
  T* data_;
  size_t size_;
};

//===----------------------------------------------------------------------===//
// Error
//===----------------------------------------------------------------------===//

class Error {
 public:
  Error() = default;
  Error(XLA_FFI_Error_Code errc, std::string message)
      : errc_(errc), message_(std::move(message)) {}

  static Error Success() { return Error(); }

  bool success() const { return errc_ == XLA_FFI_Error_Code_OK; }
  bool failure() const { return !success(); }

  std::optional<XLA_FFI_Error_Code> errc() const { return errc_; }
  const std::string& message() const { return message_; }

 private:
  XLA_FFI_Error_Code errc_;
  std::string message_;
};

//===----------------------------------------------------------------------===//
// Arguments
//===----------------------------------------------------------------------===//

struct BufferBase {
  DataType dtype;
  void* data;
  Span<const int64_t> dimensions;
};

namespace internal {

// A workaround for the fact that a static_assertion can be evaluated
// whether or not the template is instantiated
template <DataType dtype>
struct always_false : std::false_type {};

template <DataType dtype>
struct PtrType {
  static_assert(always_false<dtype>::value, "unsupported data type");
};

// clang-format off
template <> struct PtrType<DataType::PRED> { using Type = bool; };
template <> struct PtrType<DataType::U8>   { using Type = uint8_t; };
template <> struct PtrType<DataType::U16>  { using Type = uint16_t; };
template <> struct PtrType<DataType::U32>  { using Type = uint32_t; };
template <> struct PtrType<DataType::U64>  { using Type = uint64_t; };
template <> struct PtrType<DataType::S8>   { using Type = int8_t; };
template <> struct PtrType<DataType::S16>  { using Type = int16_t; };
template <> struct PtrType<DataType::S32>  { using Type = int32_t; };
template <> struct PtrType<DataType::S64>  { using Type = int64_t; };
template <> struct PtrType<DataType::F16>  { using Type = uint16_t; };
template <> struct PtrType<DataType::F32>  { using Type = float; };
template <> struct PtrType<DataType::F64>  { using Type = double; };
template <> struct PtrType<DataType::BF16> { using Type = uint16_t; };
// clang-format on

inline constexpr size_t kDynamicRank = std::numeric_limits<size_t>::max();

}  // namespace internal

template <DataType dtype, size_t rank = internal::kDynamicRank>
struct Buffer {
  typename internal::PtrType<dtype>::Type* data;
  Span<const int64_t> dimensions;
};

// clang-format off
template <DataType dtype> using BufferR0 = Buffer<dtype, 0>;
template <DataType dtype> using BufferR1 = Buffer<dtype, 1>;
template <DataType dtype> using BufferR2 = Buffer<dtype, 2>;
template <DataType dtype> using BufferR3 = Buffer<dtype, 3>;
template <DataType dtype> using BufferR4 = Buffer<dtype, 4>;
// clang-format on

//===----------------------------------------------------------------------===//
// Arguments decoding
//===----------------------------------------------------------------------===//

inline std::ostream& operator<<(std::ostream& os, const XLA_FFI_ArgType type) {
  switch (type) {
    case XLA_FFI_ArgType_BUFFER:
      return os << "buffer";
  }
}

template <>
struct ArgDecoding<BufferBase> {
  XLA_ATTRIBUTE_ALWAYS_INLINE
  static std::optional<BufferBase> Decode(XLA_FFI_ArgType type, void* arg,
                                          DiagnosticEngine& diagnostic) {
    if (type != XLA_FFI_ArgType_BUFFER) {
      return diagnostic.Emit("Wrong argument type: expected ")
             << XLA_FFI_ArgType_BUFFER << " but got " << type;
    }
    auto* buf = reinterpret_cast<XLA_FFI_Buffer*>(arg);
    return BufferBase{static_cast<DataType>(buf->dtype), buf->data,
                      Span<const int64_t>(buf->dims, buf->rank)};
  }
};

template <DataType dtype, size_t rank>
struct ArgDecoding<Buffer<dtype, rank>> {
  XLA_ATTRIBUTE_ALWAYS_INLINE
  static std::optional<Buffer<dtype, rank>> Decode(
      XLA_FFI_ArgType type, void* arg, DiagnosticEngine& diagnostic) {
    if (type != XLA_FFI_ArgType_BUFFER) {
      return diagnostic.Emit("Wrong argument type: expected ")
             << XLA_FFI_ArgType_BUFFER << " but got " << type;
    }
    auto* buf = reinterpret_cast<XLA_FFI_Buffer*>(arg);
    if (auto actual_dtype = static_cast<DataType>(buf->dtype);
        actual_dtype != dtype) {
      return diagnostic.Emit("Wrong buffer dtype: expected ")
             << dtype << " but got " << actual_dtype;
    }
    auto* data =
        static_cast<typename internal::PtrType<dtype>::Type*>(buf->data);
    if constexpr (rank != internal::kDynamicRank) {
      if (buf->rank != rank) {
        diagnostic.Emit("Wrong buffer rank: expected ")
            << rank << " but got " << buf->rank;
        return std::nullopt;
      }
    }
    return Buffer<dtype, rank>{data, Span<const int64_t>(buf->dims, rank)};
  }
};

//===----------------------------------------------------------------------===//
// Result encoding
//===----------------------------------------------------------------------===//

template <>
struct ResultEncoding<Error> {
  static XLA_FFI_Error* Encode(XLA_FFI_Api* api, Error error) {
    if (error.success()) return nullptr;

    XLA_FFI_Error_Create_Args args;
    args.struct_size = XLA_FFI_Error_Create_Args_STRUCT_SIZE;
    args.priv = nullptr;
    args.errc = *error.errc();
    args.message = error.message().c_str();
    return api->XLA_FFI_Error_Create(&args);
  }
};

//===----------------------------------------------------------------------===//
// PlatformStream
//===----------------------------------------------------------------------===//

template <typename T>
struct PlatformStream {};

template <typename T>
struct CtxDecoding<PlatformStream<T>> {
  using Type = T;

  static_assert(std::is_pointer_v<T>, "stream type must be a pointer");

  static std::optional<Type> Decode(const XLA_FFI_Api* api,
                                    XLA_FFI_ExecutionContext* ctx,
                                    DiagnosticEngine& diagnostic) {
    XLA_FFI_Stream_Get_Args args;
    args.struct_size = XLA_FFI_Stream_Get_Args_STRUCT_SIZE;
    args.priv = nullptr;
    args.ctx = ctx;
    args.stream = nullptr;

    if (XLA_FFI_Error* error = api->XLA_FFI_Stream_Get(&args); error) {
      diagnostic.Emit("Failed to get platform stream: ")
          << GetErrorMessage(api, error);
      DestroyError(api, error);
      return std::nullopt;
    }

    return reinterpret_cast<T>(args.stream);
  }

  static const char* GetErrorMessage(const XLA_FFI_Api* api,
                                     XLA_FFI_Error* error) {
    XLA_FFI_Error_GetMessage_Args args;
    args.struct_size = XLA_FFI_Error_GetMessage_Args_STRUCT_SIZE;
    args.priv = nullptr;
    args.error = error;
    api->XLA_FFI_Error_GetMessage(&args);
    return args.message;
  }

  static void DestroyError(const XLA_FFI_Api* api, XLA_FFI_Error* error) {
    XLA_FFI_Error_Destroy_Args args;
    args.struct_size = XLA_FFI_Error_Destroy_Args_STRUCT_SIZE;
    args.priv = nullptr;
    args.error = error;
    api->XLA_FFI_Error_Destroy(&args);
  }
};

}  // namespace xla::ffi

#endif  // XLA_FFI_API_FFI_H_
