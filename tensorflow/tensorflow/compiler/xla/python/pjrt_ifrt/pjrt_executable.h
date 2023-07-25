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

#ifndef TENSORFLOW_COMPILER_XLA_PYTHON_PJRT_IFRT_PJRT_EXECUTABLE_H_
#define TENSORFLOW_COMPILER_XLA_PYTHON_PJRT_IFRT_PJRT_EXECUTABLE_H_

#include <memory>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include "llvm/Support/ExtensibleRTTI.h"
#include "tensorflow/compiler/xla/client/xla_computation.h"
#include "tensorflow/compiler/xla/pjrt/pjrt_client.h"
#include "tensorflow/compiler/xla/python/ifrt/executable.h"
#include "tensorflow/compiler/xla/python/ifrt/host_callback.h"
#include "tensorflow/compiler/xla/python/pjrt_ifrt/pjrt_client.h"
#include "tensorflow/compiler/xla/python/pjrt_ifrt/pjrt_host_callback.h"
#include "tensorflow/compiler/xla/statusor.h"

namespace xla {
namespace ifrt {

// PjRt-compatible `Executable` interface.
class PjRtCompatibleExecutable
    : public llvm::RTTIExtends<PjRtCompatibleExecutable, Executable> {
 public:
  // APIs that allow direct access to `xla::PjRtExecutable` for PjRt-only
  // operations.
  virtual xla::PjRtExecutable* pjrt_executable() = 0;

  static char ID;  // NOLINT
};

// PjRt-compatible `LoadedExecutable` interface.
class PjRtCompatibleLoadedExecutable
    : public llvm::RTTIExtends<PjRtCompatibleLoadedExecutable,
                               LoadedExecutable> {
 public:
  // APIs that allow direct access to `xla::PjRtLoadedExecutable` for PjRt-only
  // operations.
  virtual xla::PjRtLoadedExecutable* pjrt_loaded_executable() = 0;
  virtual std::shared_ptr<xla::PjRtLoadedExecutable>
  shared_ptr_pjrt_loaded_executable() = 0;

  static char ID;  // NOLINT
};

// `Executable` implementation that wraps a `xla::PjRtExecutable`.
class PjRtExecutable final
    : public llvm::RTTIExtends<PjRtExecutable, PjRtCompatibleExecutable> {
 public:
  // Creates PjRtExecutable from xla::PjRtExecutable.
  static StatusOr<std::unique_ptr<Executable>> Create(
      std::unique_ptr<xla::PjRtExecutable> pjrt_executable);
  static StatusOr<std::unique_ptr<Executable>> Create(
      std::shared_ptr<xla::PjRtExecutable> pjrt_executable);

  // PjRtCompatibleExecutable implementation.

  xla::PjRtExecutable* pjrt_executable() override {
    DCHECK(this);
    return pjrt_executable_.get();
  }

  // Executable implementation.

  ~PjRtExecutable() override = default;

  absl::string_view name() const override {
    DCHECK(this);
    return pjrt_executable_->name();
  }

  std::optional<std::vector<OpSharding>> GetParameterShardings()
      const override {
    DCHECK(this);
    return pjrt_executable_->GetParameterShardings();
  }

  std::optional<std::vector<OpSharding>> GetOutputShardings() const override {
    DCHECK(this);
    return pjrt_executable_->GetOutputShardings();
  }

  StatusOr<std::optional<std::string>> Fingerprint() const override;

  StatusOr<std::string> Serialize() const override;

  int num_devices() const override {
    DCHECK(this);
    return pjrt_executable_->num_replicas() *
           pjrt_executable_->num_partitions();
  }
  int64_t SizeOfGeneratedCodeInBytes() const override {
    DCHECK(this);
    return pjrt_executable_->SizeOfGeneratedCodeInBytes();
  }
  StatusOr<CompiledMemoryStats> GetCompiledMemoryStats() const override {
    DCHECK(this);
    return pjrt_executable_->GetCompiledMemoryStats();
  }

  StatusOr<std::vector<std::shared_ptr<HloModule>>> GetHloModules()
      const override {
    DCHECK(this);
    return pjrt_executable_->GetHloModules();
  }

  StatusOr<absl::flat_hash_map<std::string, Executable::CostAnalysisValue>>
  GetCostAnalysis() const override {
    return pjrt_executable_->GetCostAnalysis();
  }

  static char ID;  // NOLINT

 protected:
  explicit PjRtExecutable(std::shared_ptr<xla::PjRtExecutable> pjrt_executable)
      : pjrt_executable_(std::move(pjrt_executable)) {}

  std::shared_ptr<xla::PjRtExecutable> pjrt_executable_;
};

// `LoadedExecutable` implementation that wraps a `xla::PjRtLoadedExecutable`.
class PjRtLoadedExecutable final
    : public llvm::RTTIExtends<PjRtLoadedExecutable,
                               PjRtCompatibleLoadedExecutable> {
 public:
  using LoadedExecutable::ExecuteOptions;
  using LoadedExecutable::ExecuteResult;

  // Creates PjRtExecutable from xla::PjRtLoadedExecutable. We expect that
  // xla::PjRtLoadedExecutable has fixed output dtypes/shapes/shardings.
  // PjRtLoadedExecutable::GetHloModules() must be implemented.
  static StatusOr<std::unique_ptr<LoadedExecutable>> Create(
      PjRtCompatibleClient* client,
      std::unique_ptr<xla::PjRtLoadedExecutable> pjrt_loaded_executable,
      std::vector<tsl::RCReference<LoadedHostCallback>> loaded_host_callbacks);
  static StatusOr<std::unique_ptr<LoadedExecutable>> Create(
      PjRtCompatibleClient* client,
      std::shared_ptr<xla::PjRtLoadedExecutable> pjrt_loaded_executable,
      std::vector<tsl::RCReference<LoadedHostCallback>> loaded_host_callbacks);

  // Creates PjRtExecutable from an MHLO or StableHLO MLIR module. We expect
  // that xla::PjRtLoadedExecutable has fixed output dtypes/shapes/shardings. If
  // options.executable_build_options has use_auto_spmd_partitioning or
  // allow_spmd_sharding_propagation_to_output enabled,
  // PjRtLoadedExecutable::GetHloModules() must be implemented.
  static StatusOr<std::unique_ptr<LoadedExecutable>> Create(
      PjRtCompatibleClient* client, mlir::ModuleOp module,
      xla::CompileOptions compile_options,
      std::vector<tsl::RCReference<LoadedHostCallback>> loaded_host_callbacks);
  // TODO(phawkins): remove the XlaComputation overload.
  static StatusOr<std::unique_ptr<LoadedExecutable>> Create(
      PjRtCompatibleClient* client, const XlaComputation& computation,
      xla::CompileOptions compile_options,
      std::vector<tsl::RCReference<LoadedHostCallback>> loaded_host_callbacks);

  // PjRtCompatibleLoadedExecutable implementation.

  xla::PjRtLoadedExecutable* pjrt_loaded_executable() override {
    DCHECK(this);
    return pjrt_loaded_executable_.get();
  }
  std::shared_ptr<xla::PjRtLoadedExecutable> shared_ptr_pjrt_loaded_executable()
      override {
    DCHECK(this);
    return pjrt_loaded_executable_;
  }

  // LoadedExecutable implementation.

  ~PjRtLoadedExecutable() override;

  absl::string_view name() const override {
    DCHECK(this);
    return pjrt_loaded_executable_->name();
  }

  std::optional<std::vector<OpSharding>> GetParameterShardings()
      const override {
    DCHECK(this);
    return pjrt_loaded_executable_->GetParameterShardings();
  }

  std::optional<std::vector<OpSharding>> GetOutputShardings() const override {
    DCHECK(this);
    return pjrt_loaded_executable_->GetOutputShardings();
  }

  StatusOr<std::optional<std::string>> Fingerprint() const override;

  StatusOr<std::string> Serialize() const override;

  int num_devices() const override {
    DCHECK(this);
    return pjrt_loaded_executable_->num_replicas() *
           pjrt_loaded_executable_->num_partitions();
  }
  int64_t SizeOfGeneratedCodeInBytes() const override {
    DCHECK(this);
    return pjrt_loaded_executable_->SizeOfGeneratedCodeInBytes();
  }
  StatusOr<CompiledMemoryStats> GetCompiledMemoryStats() const override {
    DCHECK(this);
    return pjrt_loaded_executable_->GetCompiledMemoryStats();
  }

  StatusOr<std::vector<std::shared_ptr<HloModule>>> GetHloModules()
      const override {
    DCHECK(this);
    return pjrt_loaded_executable_->GetHloModules();
  }

  StatusOr<std::vector<std::vector<absl::string_view>>> GetOutputMemoryKinds()
      const override {
    DCHECK(this);
    return pjrt_loaded_executable_->GetOutputMemoryKinds();
  }

  PjRtCompatibleClient* client() const override {
    DCHECK(this);
    return client_;
  }
  StatusOr<ExecuteResult> Execute(absl::Span<tsl::RCReference<Array>> args,
                                  const ExecuteOptions& options,
                                  std::optional<DeviceList> devices) override;

  Future<Status> Delete() override;
  bool IsDeleted() const override {
    DCHECK(this);
    return pjrt_loaded_executable_->IsDeleted();
  }

  absl::Span<const LoadedExecutable::LogicalDeviceIds>
  addressable_device_logical_ids() const override {
    DCHECK(this);
    return pjrt_loaded_executable_->addressable_device_logical_ids();
  }
  absl::Span<Device* const> addressable_devices() const override {
    DCHECK(this);
    return pjrt_loaded_executable_->addressable_devices();
  }

  StatusOr<absl::flat_hash_map<std::string, Executable::CostAnalysisValue>>
  GetCostAnalysis() const override {
    return pjrt_loaded_executable_->GetCostAnalysis();
  }

  static char ID;  // NOLINT

 private:
  static StatusOr<std::unique_ptr<LoadedExecutable>> CreateInternal(
      PjRtCompatibleClient* client,
      std::shared_ptr<xla::PjRtLoadedExecutable> pjrt_loaded_executable,
      const xla::Shape& result_shape,
      const xla::HloSharding* result_hlo_sharding,
      std::vector<tsl::RCReference<LoadedHostCallback>> loaded_host_callbacks);

  PjRtLoadedExecutable(
      PjRtCompatibleClient* client,
      std::shared_ptr<xla::PjRtLoadedExecutable> pjrt_loaded_executable,
      DeviceList devices,
      std::vector<tsl::RCReference<LoadedHostCallback>>
          all_loaded_host_callbacks,
      std::vector<PjRtHostSendAndRecvLoadedHostCallback*>
          host_send_recv_callbacks,
      std::vector<DType> output_dtypes, std::vector<Shape> output_shapes,
      std::vector<std::shared_ptr<const Sharding>> output_shardings);

  PjRtCompatibleClient* client_;
  std::shared_ptr<xla::PjRtLoadedExecutable> pjrt_loaded_executable_;
  DeviceList devices_;
  std::shared_ptr<std::vector<tsl::RCReference<LoadedHostCallback>>>
      all_loaded_host_callbacks_;
  std::vector<PjRtHostSendAndRecvLoadedHostCallback*> host_send_recv_callbacks_;
  std::vector<DType> output_dtypes_;
  std::vector<Shape> output_shapes_;
  std::vector<std::shared_ptr<const Sharding>> output_shardings_;
};

}  // namespace ifrt
}  // namespace xla

#endif  // TENSORFLOW_COMPILER_XLA_PYTHON_PJRT_IFRT_PJRT_EXECUTABLE_H_
