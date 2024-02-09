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

#include "tensorflow/core/framework/common_shape_fns.h"
#include "tensorflow/core/framework/op.h"

namespace tensorflow {
namespace tfrt_stub {

REGISTER_OP("IfrtCall")
    .Input("args: Tin")
    .Output("results: Tout")
    .Attr("Tin: list(type) >= 0")
    .Attr("Tout: list(type) >= 0")
    .Attr("program_id: int")
    .Attr("variable_names: list(string)")
    .Attr("variable_arg_indices: list(int)")
    .SetIsStateful()
    .SetShapeFn(tensorflow::shape_inference::UnknownShape)
    .Doc(R"(
Calls an IFRT program identified by the given program id.

This op looks up a `ServingExecutable` from `ServingExecutableRegistry` using
the program id, calls the executable with the op's inputs as arguments, and
returns its results as the op's outputs.

Note that this op is not part of a stable interface. Users must not use this op
in their SavedModel and instead rely on Ifrt Serving's mechanism that
automatically inserts this op with graph rewrite.

program_id: int64 id that can be used to look up compiled programs from
ServingExecutableRegistry`.

variable_names: names of variable tensors. A name can be used to look up
corresponding loaded array of that variable tensor.

variable_arg_indices: variable_arg_indices[k] indicates the position of tensor
`variable_names[k]` in the argument list of the TPU program. This array must be
in sorted ascending order.
)");

}  // namespace tfrt_stub
}  // namespace tensorflow
