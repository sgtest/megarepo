/* Copyright 2019 The TensorFlow Authors. All Rights Reserved.

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
#include "xla/service/optimize_input_output_buffer_alias.h"

#include <cstdint>
#include <vector>

#include "absl/algorithm/container.h"
#include "absl/container/flat_hash_set.h"
#include "absl/log/log.h"
#include "absl/strings/string_view.h"
#include "absl/types/span.h"
#include "xla/hlo/ir/hlo_input_output_alias_config.h"
#include "xla/hlo/ir/hlo_module.h"
#include "xla/layout_util.h"
#include "xla/shape.h"
#include "xla/shape_util.h"
#include "xla/status_macros.h"
#include "xla/statusor.h"
#include "tsl/platform/errors.h"

namespace xla {

StatusOr<bool> OptimizeInputOutputBufferAlias::Build(
    absl::Span<const Shape> input_shapes, const Shape& output_shape,
    HloInputOutputAliasConfig* alias_config,
    HloBufferDonorConfig* buffer_donor_config) {
  bool changed = false;
  if (output_shape.is_dynamic()) {
    // Restrict dynamic shape input-output aliasing due to potential
    // dynamic shape size calculation mismatch.
    return false;
  }

  // Collects all buffer donors in a vector.
  struct DonorEntry {
    int64_t param_number;
    ShapeIndex index;
    int64_t shape_size;
  };
  std::vector<DonorEntry> donor_vectors;

  for (int64_t param_number = 0; param_number < input_shapes.size();
       ++param_number) {
    const Shape& input_shape = input_shapes[param_number];
    TF_RET_CHECK(LayoutUtil::HasLayout(input_shape));
    VLOG(1) << "input_shape: " << input_shape.ToString();
    ShapeUtil::ForEachSubshape(input_shape, [&](const Shape& subshape,
                                                const ShapeIndex& index) {
      if (!LayoutUtil::IsDenseArray(subshape) || subshape.is_dynamic()) {
        return;
      }
      if (alias_config->ParameterHasAlias(param_number, index)) {
        return;
      }
      if (registered_buffer_donor_only_ &&
          !buffer_donor_config->ParameterIsBufferDonor(param_number, index)) {
        return;
      }
      donor_vectors.emplace_back(
          DonorEntry{param_number, index, shape_size_fn_(subshape)});
    });
  }

  // Collects all buffer donees in a vector.
  struct DoneeEntry {
    ShapeIndex index;
    int64_t shape_size;
  };
  std::vector<DoneeEntry> donee_vectors;
  TF_RET_CHECK(LayoutUtil::HasLayout(output_shape));
  VLOG(1) << "output_shape: " << output_shape.ToString();
  ShapeUtil::ForEachSubshape(
      output_shape, [&](const Shape& subshape, const ShapeIndex& index) {
        if (!LayoutUtil::IsDenseArray(subshape)) {
          return;
        }
        if (alias_config->OutputHasAlias(index)) {
          return;
        }
        donee_vectors.emplace_back(DoneeEntry{index, shape_size_fn_(subshape)});
      });

  // Sort donor and donees by their shape size in non-increasing order.
  absl::c_stable_sort(donor_vectors,
                      [](const DonorEntry& a, const DonorEntry& b) -> bool {
                        return a.shape_size > b.shape_size;
                      });
  absl::c_stable_sort(donee_vectors,
                      [](const DoneeEntry& a, const DoneeEntry& b) -> bool {
                        return a.shape_size > b.shape_size;
                      });

  // Match donors and donees with two pointers. The larger size a donee has, the
  // more prioritized the donee will get matched.
  int64_t donor_vector_index = 0;
  int64_t donee_vector_index = 0;
  while (donor_vector_index < donor_vectors.size() &&
         donee_vector_index < donee_vectors.size()) {
    const auto& donor = donor_vectors[donor_vector_index];
    const auto& donee = donee_vectors[donee_vector_index];
    if (donor.shape_size > donee.shape_size) {
      donor_vector_index += 1;
    } else if (donor.shape_size < donee.shape_size) {
      donee_vector_index += 1;
    } else {
      // The current donor and donee match.
      TF_RETURN_IF_ERROR(alias_config->SetUpAlias(
          donee.index, donor.param_number, donor.index));
      TF_RETURN_IF_ERROR(buffer_donor_config->RemoveBufferDonor(
          donor.param_number, donor.index));
      donor_vector_index += 1;
      donee_vector_index += 1;
      changed = true;
    }
  }

  return changed;
}

StatusOr<bool> OptimizeInputOutputBufferAlias::Run(
    HloModule* module,
    const absl::flat_hash_set<absl::string_view>& execution_threads) {
  // We exactly follow HloInputOutputAliasConfig::Verify to create input_shapes
  // and output_shape.
  const auto& entry_computation_layout = module->entry_computation_layout();
  std::vector<Shape> input_shapes;
  for (int64_t i = 0; i < module->entry_computation()->num_parameters(); ++i) {
    input_shapes.push_back(entry_computation_layout.parameter_shape(i));
  }
  const Shape& output_shape = entry_computation_layout.result_shape();

  HloInputOutputAliasConfig* alias_config =
      &module->input_output_alias_config();
  HloBufferDonorConfig* buffer_donor_config = &module->buffer_donor_config();

  TF_ASSIGN_OR_RETURN(bool changed, Build(input_shapes, output_shape,
                                          alias_config, buffer_donor_config));
  TF_RETURN_IF_ERROR(alias_config->Verify(*module, shape_size_fn_));

  return changed;
}

}  // namespace xla
