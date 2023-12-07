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

#ifndef XLA_FFI_FFI_API_H_
#define XLA_FFI_FFI_API_H_

#include <string_view>

#include "xla/ffi/api/api.h"
#include "xla/ffi/api/c_api.h"
#include "xla/ffi/api/c_api_internal.h"  // IWYU pragma: keep
#include "xla/ffi/call_frame.h"
#include "xla/hlo/ir/hlo_computation.h"
#include "xla/service/service_executable_run_options.h"
#include "xla/status.h"
#include "xla/statusor.h"

namespace xla::ffi {

// This is an implementation of XLA FFI API defined in `api/c_api.h` header. It
// should be linked statically into the "main" XLA binary, and third party FFI
// handlers can be linked and registered dynamically.
//
// FFI handlers registered statically (and built from the same XLA commit with
// the same toolchain) can also use `api/c_api_internal.h` to get access to
// various internal data structures.

//===----------------------------------------------------------------------===//
// Calling XLA FFI handlers
//===----------------------------------------------------------------------===//

struct CallOptions {
  const ServiceExecutableRunOptions* run_options = nullptr;
  const HloComputation* called_computation = nullptr;
};

// Takes ownership of the XLA FFI error and returns underlying status. Frees
// `error` if it's not nullptr; returns OK status otherwise.
Status TakeStatus(XLA_FFI_Error* error);

Status Call(Ffi& handler, CallFrame& call_frame,
            const CallOptions& options = {});

Status Call(XLA_FFI_Handler* handler, CallFrame& call_frame,
            const CallOptions& options = {});

//===----------------------------------------------------------------------===//
// XLA FFI registry
//===----------------------------------------------------------------------===//

// Returns registered FFI handler for a given name, or an error if it's not
// found in the static registry.
StatusOr<XLA_FFI_Handler*> FindHandler(std::string_view name);

//===----------------------------------------------------------------------===//
// XLA FFI Api Implementation
//===----------------------------------------------------------------------===//

XLA_FFI_Api* GetXlaFfiApi();

}  // namespace xla::ffi

#endif  // XLA_FFI_FFI_API_H_
