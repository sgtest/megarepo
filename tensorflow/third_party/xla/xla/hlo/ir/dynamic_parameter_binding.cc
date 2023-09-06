/* Copyright 2018 The TensorFlow Authors. All Rights Reserved.

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

#include "xla/hlo/ir/dynamic_parameter_binding.h"

#include <optional>
#include <ostream>
#include <string>
#include <vector>

#include "xla/hlo/ir/hlo_computation.h"
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/hlo/ir/hlo_module.h"

namespace xla {

Status DynamicParameterBinding::Bind(
    const DynamicParameter& dynamic_parameter,
    const DynamicDimension& dynamic_dimension) {
  auto result = bindings_.emplace(dynamic_dimension, dynamic_parameter);
  TF_RET_CHECK(result.second);
  return OkStatus();
}

std::optional<DynamicParameterBinding::DynamicParameter>
DynamicParameterBinding::GetBinding(
    const DynamicDimension& dynamic_dimension) const {
  auto param_iter = bindings_.find(dynamic_dimension);
  if (param_iter == bindings_.end()) {
    return std::nullopt;
  }
  return param_iter->second;
}

std::string DynamicParameterBinding::ToString() const {
  std::vector<std::string> pieces;
  pieces.push_back("DynamicParameterBinding: ");
  for (const auto& binding : bindings_) {
    const DynamicDimension& dynamic_dimension = binding.first;
    const DynamicParameter& dynamic_param = binding.second;
    pieces.push_back(absl::StrFormat(
        " -- Input param number %lld at %s has dim %lld as dynamic"
        " dimension, which is represented by param number %lld at "
        "%s",
        dynamic_dimension.parameter_num,
        dynamic_dimension.parameter_index.ToString(),
        dynamic_dimension.dimension, dynamic_param.parameter_num,
        dynamic_param.parameter_index.ToString()));
  }
  return absl::StrJoin(pieces, "\n");
}

Status DynamicParameterBinding::ForEachBinding(BindingFn fn) const {
  for (const auto& binding : bindings_) {
    TF_RETURN_IF_ERROR(fn(binding.second, binding.first));
  }
  return OkStatus();
}

Status DynamicParameterBinding::Verify(const HloModule& module) const {
  const HloComputation* entry = module.entry_computation();
  return ForEachBinding([&](const DynamicParameter& dynamic_parameter,
                            const DynamicDimension& dynamic_dimension)
                            -> Status {
    TF_RET_CHECK(dynamic_parameter.parameter_num >= 0 &&
                 dynamic_parameter.parameter_num < entry->num_parameters());
    TF_RET_CHECK(dynamic_dimension.parameter_num < entry->num_parameters());
    TF_RET_CHECK(ShapeUtil::IndexIsValid(
        entry->parameter_instruction(dynamic_parameter.parameter_num)->shape(),
        dynamic_parameter.parameter_index));
    TF_RET_CHECK(ShapeUtil::IndexIsValid(
        entry->parameter_instruction(dynamic_dimension.parameter_num)->shape(),
        dynamic_dimension.parameter_index));
    TF_RET_CHECK(
        dynamic_dimension.dimension <
        ShapeUtil::GetSubshape(
            entry->parameter_instruction(dynamic_dimension.parameter_num)
                ->shape(),
            dynamic_dimension.parameter_index)
            .rank());
    return OkStatus();
  });
}

std::ostream& operator<<(std::ostream& out,
                         const DynamicParameterBinding& binding) {
  out << binding.ToString();
  return out;
}

}  // namespace xla
