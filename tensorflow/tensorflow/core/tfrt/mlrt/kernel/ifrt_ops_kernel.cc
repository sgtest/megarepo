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

#include <optional>
#include <string>
#include <vector>

#include "absl/log/check.h"
#include "absl/log/log.h"
#include "absl/status/status.h"
#include "absl/strings/str_cat.h"
#include "absl/strings/string_view.h"
#include "absl/types/span.h"
#include "xla/hlo/ir/hlo_sharding.h"
#include "xla/xla_data.pb.h"
#include "tensorflow/core/framework/tensor.h"
#include "tensorflow/core/platform/protobuf.h"  // IWYU pragma: keep
#include "tensorflow/core/tfrt/ifrt/ifrt_config.pb.h"
#include "tensorflow/core/tfrt/ifrt/ifrt_model_context.h"
#include "tensorflow/core/tfrt/ifrt/sharding_utils.h"
#include "tensorflow/core/tfrt/mlrt/bytecode/bytecode.h"
#include "tensorflow/core/tfrt/mlrt/interpreter/context.h"
#include "tensorflow/core/tfrt/mlrt/kernel/context.h"
#include "tensorflow/core/tfrt/mlrt/kernel/kernel.h"
#include "tensorflow/core/tfrt/utils/fallback_tensor.h"
#include "tsl/platform/statusor.h"

namespace tensorflow {
namespace tf_mlrt {

namespace {

absl::Status IfrtLoadVariable(
    tensorflow::ifrt_serving::IfrtModelContext& ifrt_model_context,
    const tensorflow::Tensor& variable,
    absl::string_view sharding_config_proto_text, absl::string_view name) {
  tensorflow::ifrt_serving::VariableDeviceShardingConfigProto sharding_config;

  if (!tensorflow::protobuf::TextFormat::ParseFromString(
          std::string(sharding_config_proto_text), &sharding_config)) {
    return absl::InvalidArgumentError(absl::StrCat(
        "Attribute: ", sharding_config_proto_text, " cannot be parsed"));
  }

  std::vector<int> device_ids{sharding_config.device_ids().begin(),
                              sharding_config.device_ids().end()};
  TF_ASSIGN_OR_RETURN(xla::HloSharding hlo_sharding,
                      xla::HloSharding::FromProto(sharding_config.sharding()));
  TF_ASSIGN_OR_RETURN(
      auto result_array,
      tensorflow::ifrt_serving::MakeArrayFromTensor(
          *ifrt_model_context.GetClient(), variable, absl::MakeSpan(device_ids),
          hlo_sharding, ifrt_model_context.GetThreadPoolDevice()));

  return ifrt_model_context.RegisterLoadedVariable(name, result_array);
}

struct MlrtIfrtLoadVariableKernel : mlrt::KernelFrame {
  using KernelFrame::KernelFrame;

  static constexpr char kName[] = "tf_mlrt.ifrt_load_variable";

  const tensorflow::Tensor& variable() const {
    DCHECK_GE(arguments().size(), 1);
    return arguments()[0].Get<tensorflow::tfrt_stub::FallbackTensor>().tensor();
  }

  absl::string_view sharding_config_proto_text() const {
    DCHECK_EQ(attributes().size(), 2);
    return attributes().GetAs<mlrt::bc::String>(0).Get();
  }
  absl::string_view name() const {
    DCHECK_EQ(attributes().size(), 2);
    return attributes().GetAs<mlrt::bc::String>(1).Get();
  }

  Context& context() { return execution_context().GetUserContext<Context>(); }
  void Invoke();
};

void MlrtIfrtLoadVariableKernel::Invoke() {
  std::optional<tensorflow::ifrt_serving::IfrtModelContext*>
      ifrt_model_context =
          context()
              .resource_context()
              .GetResource<tensorflow::ifrt_serving::IfrtModelContext>(
                  "IfrtModelContext");
  if (!ifrt_model_context.has_value()) {
    execution_context().Fail(absl::FailedPreconditionError(
        "LoadVariableOp: failed to fetch IfrtModelContext: "));
    return;
  }

  auto status = IfrtLoadVariable(**ifrt_model_context, variable(),
                                 sharding_config_proto_text(), name());
  if (!status.ok()) {
    execution_context().Fail(status);
    return;
  }
}
void RegisterTfMlrtIfrtKernels(mlrt::KernelRegistry& registry) {
  registry.Register<MlrtIfrtLoadVariableKernel>();
}

}  // namespace

const bool kUnused = [] {
  RegisterTfMlrtIfrtKernels(GetTfMlrtOptionalKernelRegistry());
  return true;
}();

}  // namespace tf_mlrt
}  // namespace tensorflow
