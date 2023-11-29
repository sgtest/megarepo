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

#ifndef XLA_SERVICE_GPU_FUSION_MERGER_TRITON_H_
#define XLA_SERVICE_GPU_FUSION_MERGER_TRITON_H_

#include "absl/container/flat_hash_set.h"
#include "absl/strings/string_view.h"
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/hlo/ir/hlo_module.h"
#include "xla/service/hlo_pass_interface.h"
#include "xla/statusor.h"

namespace xla {
namespace gpu {

// An HLO pass that attempts to merge producer fusions into triton softmax
// fusions.
//
// Producer kernels are only merged if the resulting fusion can be correctly
// tiled. If the result can be tiled, all operations from the auxiliary
// producer fusion will be merged into the triton softmax computation, and this
// computation will replace both the auxiliary and original triton softmax
// fusion.
//
// Auxiliary fusions are not merged into consumer triton fusions if:
// * The auxiliary fusion has multiple users
// * The resulting merged fusion is not tilable
class FusionMergerTriton : public HloModulePass {
 public:
  explicit FusionMergerTriton() = default;
  absl::string_view name() const override { return "fusion-merger-triton"; }

  using HloPassInterface::Run;
  StatusOr<bool> Run(
      HloModule* module,
      const absl::flat_hash_set<absl::string_view>& execution_threads) override;
};

}  // namespace gpu
}  // namespace xla

#endif  // XLA_SERVICE_GPU_FUSION_MERGER_TRITON_H_
