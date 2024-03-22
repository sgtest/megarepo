/* Copyright 2024 The OpenXLA Authors.

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
#include "xla/service/gpu/fusions/custom.h"

#include <cstddef>
#include <cstdint>
#include <memory>
#include <optional>
#include <string>
#include <string_view>
#include <utility>
#include <variant>
#include <vector>

#include "absl/algorithm/container.h"
#include "absl/log/log.h"
#include "absl/status/status.h"
#include "absl/strings/str_cat.h"
#include "llvm/ADT/STLExtras.h"
#include "llvm/ADT/TypeSwitch.h"
#include "mlir/AsmParser/AsmParser.h"  // from @llvm-project
#include "mlir/IR/Attributes.h"  // from @llvm-project
#include "mlir/IR/BuiltinAttributes.h"  // from @llvm-project
#include "mlir/IR/Operation.h"  // from @llvm-project
#include "xla/ffi/api/c_api.h"
#include "xla/ffi/ffi_api.h"
#include "xla/hlo/ir/hlo_casting_utils.h"
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/hlo/ir/hlo_instructions.h"
#include "xla/hlo/ir/hlo_opcode.h"
#include "xla/service/buffer_assignment.h"
#include "xla/service/custom_call_status.h"
#include "xla/service/custom_call_target_registry.h"
#include "xla/service/gpu/backend_configs.pb.h"
#include "xla/service/gpu/cublas_cudnn.h"
#include "xla/service/gpu/fusions/fusion_emitter.h"
#include "xla/service/gpu/hlo_fusion_analysis.h"
#include "xla/service/gpu/hlo_traversal.h"
#include "xla/service/gpu/ir_emission_utils.h"
#include "xla/service/gpu/ir_emitter_context.h"
#include "xla/service/gpu/kernel_arguments.h"
#include "xla/service/gpu/kernels/custom_kernel.h"
#include "xla/service/gpu/kernels/custom_kernel_fusion.h"
#include "xla/service/gpu/matmul_utils.h"
#include "xla/service/gpu/runtime/address_computation_thunk.h"
#include "xla/service/gpu/runtime/custom_call_thunk.h"
#include "xla/service/gpu/runtime/gemm_thunk.h"
#include "xla/service/gpu/runtime/kernel_thunk.h"
#include "xla/service/gpu/thunk.h"
#include "xla/shape.h"
#include "xla/shape_util.h"
#include "xla/status.h"
#include "xla/util.h"
#include "tsl/platform/errors.h"
#include "tsl/platform/statusor.h"

namespace xla {
namespace gpu {
namespace {

absl::StatusOr<std::unique_ptr<Thunk>> BuildCustomKernelThunkForFusion(
    IrEmitterContext& ir_emitter_context, const HloFusionInstruction& fusion,
    CustomKernel custom_kernel) {
  TF_ASSIGN_OR_RETURN(
      auto kernel_arguments,
      KernelArguments::Create(ir_emitter_context.buffer_assignment(), &fusion));

  return std::make_unique<CustomKernelThunk>(
      &fusion, std::move(custom_kernel), std::move(kernel_arguments.args()));
}

absl::StatusOr<BufferAllocation::Slice> GetSliceWithUpdatedOffsetAndSize(
    const BufferAssignment& buffer_assignment, const HloFusionAdaptor& fusion,
    const HloInstruction& fusion_instr, const HloInstruction& start,
    const ShapeIndex& index) {
  if (const auto* param = DynCast<HloParameterInstruction>(&start)) {
    return GetAllocationSlice(buffer_assignment,
                              fusion_instr.operand(param->parameter_number()),
                              index);
  }

  auto slice_adaptor =
      HloFindIf({HloInstructionAdaptor(start)}, fusion,
                [](auto node) { return node.opcode() == HloOpcode::kSlice; });
  if (!slice_adaptor.has_value()) {
    return absl::InternalError(
        "AddressComputationFusion expects at least one sliced operand");
  }

  const auto& slice_instr =
      *static_cast<const HloSliceInstruction*>(&slice_adaptor->instruction());

  if (!IsContiguousSlice(slice_instr)) {
    return absl::InternalError(
        "AddressComputationFusion only handles contiguous slices currently");
  }

  const Shape& src_shape = slice_instr.operand(0)->shape();
  const Shape& dst_shape = slice_instr.shape();
  int64_t size = ShapeUtil::ByteSizeOf(dst_shape);

  const auto* param = Cast<HloParameterInstruction>(slice_instr.operand(0));
  TF_ASSIGN_OR_RETURN(
      BufferAllocation::Slice orig_slice,
      GetAllocationSlice(buffer_assignment,
                         fusion_instr.operand(param->parameter_number()),
                         index));

  // Given this slice
  // f16[1,4,8]{2,1,0} slice(f16[2,8,8]{2,1,0}),
  //                         slice={[1:2], [4:8], [0:8]}
  //
  // The offset of the slice should be:
  //    slice_starts(0) * 8 * 8 * sizeof(f16) +
  //    slice_starts(1) * 8 * sizeof(f16)
  int64_t offset = orig_slice.offset();
  for (auto [start, stride] : llvm::zip(slice_instr.slice_starts(),
                                        *ShapeUtil::ByteStrides(src_shape))) {
    offset += start * stride;
  }

  return BufferAllocation::Slice(orig_slice.allocation(), offset, size);
}

absl::StatusOr<FusionEmissionResult> EmitGemm(
    IrEmitterContext& ir_emitter_context, const HloFusionAdaptor& adaptor,
    const HloFusionInstruction& fusion,
    const HloCustomCallInstruction& custom_call) {
  const BufferAssignment& buffer_assignment =
      ir_emitter_context.buffer_assignment();

  TF_ASSIGN_OR_RETURN(
      BufferAllocation::Slice lhs_slice,
      GetSliceWithUpdatedOffsetAndSize(buffer_assignment, adaptor, fusion,
                                       *custom_call.operand(0), /*index=*/{}));

  TF_ASSIGN_OR_RETURN(
      BufferAllocation::Slice rhs_slice,
      GetSliceWithUpdatedOffsetAndSize(buffer_assignment, adaptor, fusion,
                                       *custom_call.operand(1), /*index=*/{}));

  BufferAllocation::Slice output;
  std::optional<BufferAllocation::Slice> workspace;

  // Result of a legacy cuBLAS custom call can be a tuple if we explicitly
  // allocate workspace buffer in HLO. If result is an array, it means that
  // workspace is not available, and cuBLAS will allocate its own workspace.
  if (custom_call.shape().IsArray()) {
    TF_ASSIGN_OR_RETURN(output,
                        GetAllocationSlice(buffer_assignment, &fusion, {}));
  } else {
    TF_ASSIGN_OR_RETURN(output,
                        GetAllocationSlice(buffer_assignment, &fusion, {0}));
    TF_ASSIGN_OR_RETURN(workspace,
                        GetAllocationSlice(buffer_assignment, &fusion, {1}));
  }

  bool deterministic_ops =
      ir_emitter_context.debug_options().xla_gpu_deterministic_ops();

  TF_ASSIGN_OR_RETURN(
      GemmConfig config,
      GemmConfig::For(static_cast<const HloInstruction*>(&custom_call)));
  auto thunk = std::make_unique<GemmThunk>(
      Thunk::ThunkInfo::WithProfileAnnotation(&custom_call), std::move(config),
      lhs_slice, rhs_slice, output, workspace, deterministic_ops);

  FusionEmissionResult result;
  result.thunks.push_back(std::move(thunk));
  return result;
}

absl::StatusOr<FusionEmissionResult> EmitDynamicSlicedGemm(
    IrEmitterContext& ir_emitter_context, const HloFusionAdaptor& adaptor,
    const HloFusionInstruction& fusion,
    const HloCustomCallInstruction& custom_call) {
  const BufferAssignment& buffer_assignment =
      ir_emitter_context.buffer_assignment();

  std::vector<std::optional<std::vector<BufferAllocation::Slice>>>
      offset_buffer_indices;
  std::vector<std::optional<const Shape>> orig_shapes;
  std::vector<std::optional<const Shape>> sliced_shapes;

  HloDynamicIndexInstruction* slice_instr = nullptr;
  auto get_original_operand_slice =
      [&](const HloInstruction* start,
          const ShapeIndex& index) -> absl::StatusOr<BufferAllocation::Slice> {
    auto* param = DynCast<HloParameterInstruction>(start);
    auto slice_adaptor = HloFindIf(
        {HloInstructionAdaptor(*start)}, adaptor,
        [](auto node) { return node.opcode() == HloOpcode::kDynamicSlice; });
    if (slice_adaptor.has_value()) {
      slice_instr = const_cast<HloDynamicIndexInstruction*>(
          static_cast<const HloDynamicIndexInstruction*>(
              &slice_adaptor->instruction()));

      if (!IsContiguousSlice(slice_instr->operand(0)->shape(),
                             slice_instr->shape())) {
        return absl::InternalError(
            "DynamicAddressComputationFusion only handles contiguous slices "
            "currently");
      }

      param = Cast<HloParameterInstruction>(slice_instr->operand(0));
    }

    return GetAllocationSlice(buffer_assignment,
                              fusion.operand(param->parameter_number()), index);
  };

  auto collect_slice_info = [&]() {
    if (slice_instr == nullptr) {
      offset_buffer_indices.push_back(std::nullopt);
      orig_shapes.push_back(std::nullopt);
      sliced_shapes.push_back(std::nullopt);
      return;
    }

    std::vector<BufferAllocation::Slice> offset_slices;
    for (auto idx_op : slice_instr->index_operands()) {
      const auto* param = Cast<HloParameterInstruction>(idx_op);
      offset_slices.push_back(
          GetAllocationSlice(buffer_assignment,
                             fusion.operand(param->parameter_number()),
                             /*index=*/{})
              .value());
    }
    offset_buffer_indices.push_back(offset_slices);
    orig_shapes.push_back(slice_instr->operand(0)->shape());
    sliced_shapes.push_back(DynCast<HloDynamicSliceInstruction>(slice_instr)
                                ? slice_instr->shape()
                                : slice_instr->operand(1)->shape());
  };

  TF_ASSIGN_OR_RETURN(
      BufferAllocation::Slice lhs_slice,
      get_original_operand_slice(custom_call.operand(0), /*index=*/{}));
  collect_slice_info();

  slice_instr = nullptr;
  TF_ASSIGN_OR_RETURN(
      BufferAllocation::Slice rhs_slice,
      get_original_operand_slice(custom_call.operand(1), /*index=*/{}));
  collect_slice_info();

  slice_instr = nullptr;
  BufferAllocation::Slice output;
  std::optional<BufferAllocation::Slice> workspace = std::nullopt;
  std::optional<BufferAllocation::Slice> slice_workspace_fake = std::nullopt;

  auto get_original_result_slice =
      [&](const HloInstruction* start,
          const ShapeIndex& index) -> absl::StatusOr<BufferAllocation::Slice> {
    auto slice_adaptor = HloFindIf(
        {HloInstructionAdaptor(*start)}, adaptor,
        [](auto node) {
          return node.opcode() == HloOpcode::kDynamicUpdateSlice;
        },
        false);
    if (slice_adaptor.has_value()) {
      slice_instr = const_cast<HloDynamicIndexInstruction*>(
          static_cast<const HloDynamicIndexInstruction*>(
              &slice_adaptor->instruction()));

      if (!IsContiguousSlice(slice_instr->operand(0)->shape(),
                             slice_instr->shape())) {
        return absl::InternalError(
            "DynamicAddressComputationFusion only handles contiguous slices "
            "currently");
      }
    }

    return GetAllocationSlice(buffer_assignment, &fusion, index);
  };

  int64_t out_byte_size = 0;
  if (custom_call.shape().IsArray()) {
    TF_ASSIGN_OR_RETURN(output,
                        get_original_result_slice(&custom_call, /*index=*/{}));
    collect_slice_info();
    // Collect slice info for std::nullopt workspace.
    slice_instr = nullptr;
    collect_slice_info();
    out_byte_size = ShapeUtil::ByteSizeOf(custom_call.shape());
  } else {
    TF_ASSIGN_OR_RETURN(output,
                        get_original_result_slice(&custom_call, /*index=*/{0}));
    collect_slice_info();
    // TODO(vuson): If we want to support slices of workspace, we'd need to
    // start `HloFindIf` with `get-tuple-element` with the right index.
    TF_ASSIGN_OR_RETURN(workspace, GetAllocationSlice(buffer_assignment,
                                                      &fusion, /*index=*/{1}));
    slice_instr = nullptr;
    collect_slice_info();
    out_byte_size = ShapeUtil::ByteSizeOf(custom_call.shape().tuple_shapes(0));
    slice_workspace_fake =
        BufferAllocation::Slice(workspace->allocation(), 0, workspace->size());
  }

  if (absl::c_all_of(offset_buffer_indices, [&](auto offset_slices) {
        return offset_slices == std::nullopt;
      }))
    return absl::InternalError(
        "DynamicAddressComputationFusion expects at least one sliced "
        "operand/result");

  // Creating embedded GEMM thunk.
  bool deterministic_ops =
      ir_emitter_context.debug_options().xla_gpu_deterministic_ops();

  TF_ASSIGN_OR_RETURN(
      GemmConfig config,
      GemmConfig::For(static_cast<const HloInstruction*>(&custom_call)));

  // TODO(vuson): handle cases where LHS and RHS share the same buffer, with
  // different offset. In such cases, the fake slices need to contain the
  // correct offset instead of default value 0.
  int64_t lhs_byte_size =
      ShapeUtil::ByteSizeOf(custom_call.operand(0)->shape());
  BufferAllocation::Slice slice_lhs_fake(lhs_slice.allocation(), 0,
                                         lhs_byte_size);

  int64_t rhs_byte_size =
      ShapeUtil::ByteSizeOf(custom_call.operand(1)->shape());
  BufferAllocation::Slice slice_rhs_fake(rhs_slice.allocation(), 0,
                                         rhs_byte_size);

  BufferAllocation::Slice slice_out_fake(output.allocation(), 0, out_byte_size);
  ThunkSequence seq;
  seq.emplace_back(std::make_unique<GemmThunk>(
      Thunk::ThunkInfo::WithProfileAnnotation(&custom_call), std::move(config),
      slice_lhs_fake, slice_rhs_fake, slice_out_fake, slice_workspace_fake,
      deterministic_ops));

  std::vector<std::optional<const BufferAllocation::Slice>> arguments{
      lhs_slice, rhs_slice, output, workspace};

  auto thunk = std::make_unique<AddressComputationThunk>(
      Thunk::ThunkInfo::WithProfileAnnotation(&custom_call),
      std::make_unique<ThunkSequence>(std::move(seq)), arguments,
      offset_buffer_indices, orig_shapes, sliced_shapes);

  FusionEmissionResult result;
  result.thunks.push_back(std::move(thunk));
  return result;
}

absl::StatusOr<FusionEmissionResult> EmitCustomCall(
    IrEmitterContext& ir_emitter_context, const HloFusionAdaptor& adaptor,
    const HloFusionInstruction& fusion,
    const HloCustomCallInstruction& custom_call) {
  const BufferAssignment& buffer_assignment =
      ir_emitter_context.buffer_assignment();

  const std::string& call_target_name = custom_call.custom_call_target();

  // Typed FFI custom calls is a replacement for legacy custom calls with
  // a rich type safe API. It's under construction and not fully supported.
  bool is_ffi_custom_call =
      custom_call.api_version() == CustomCallApiVersion::API_VERSION_TYPED_FFI;

  void* call_target = CustomCallTargetRegistry::Global()->Lookup(
      call_target_name, std::string(ir_emitter_context.platform_name()));

  absl::StatusOr<ffi::HandlerRegistration> registration =
      ffi::FindHandler(call_target_name, ir_emitter_context.platform_name());

  // At least one implementation should be available at run time.
  bool found_custom_call = !is_ffi_custom_call && call_target != nullptr;
  bool found_ffi_handler = is_ffi_custom_call && registration.ok();

  if (!found_custom_call && !found_ffi_handler) {
    return absl::InternalError(
        "AddressComputationFusion expects custom calls that are emittable as "
        "thunks");
  }

  using Slices = std::vector<std::optional<CustomCallThunk::Slice>>;

  Slices operands;
  // TODO(vuson): add test with custom call with token-typed operands
  for (auto* operand : custom_call.operands()) {
    TF_RETURN_IF_ERROR(ShapeUtil::ForEachSubshapeWithStatus(
        operand->shape(), [&](const Shape& subshape, const ShapeIndex& index) {
          if (subshape.IsToken()) {
            operands.push_back(std::nullopt);
            return absl::OkStatus();
          }
          if (!subshape.IsArray()) {
            return absl::OkStatus();
          }
          TF_ASSIGN_OR_RETURN(auto slice, GetSliceWithUpdatedOffsetAndSize(
                                              buffer_assignment, adaptor,
                                              fusion, *operand, index));
          operands.push_back(CustomCallThunk::Slice{slice, subshape});
          return absl::OkStatus();
        }));
  }

  Slices results;
  TF_RETURN_IF_ERROR(ShapeUtil::ForEachSubshapeWithStatus(
      fusion.shape(), [&](const Shape& subshape, const ShapeIndex& index) {
        if (subshape.IsToken()) {
          results.push_back(std::nullopt);
          return absl::OkStatus();
        }
        if (!subshape.IsArray()) {
          return absl::OkStatus();
        }
        TF_ASSIGN_OR_RETURN(
            auto slice, GetAllocationSlice(buffer_assignment, &fusion, index));
        results.push_back(CustomCallThunk::Slice{slice, subshape});
        return absl::OkStatus();
      }));

  // For legacy custom calls we convert all API versions into the latest
  // status-returning one and pass backend config as an opaque string.
  CustomCallThunk::CustomCallTarget custom_call_target;
  std::string opaque;

  // For XLA FFI handlers we decode opaque backend config into attributes map
  // at IR emission time, so that we do not need to parse MLIR at run time. For
  // FFI handlers backend config must be a compatible MLIR dictionary.
  CustomCallThunk::AttributesMap attributes;

  // For information about this calling convention, see
  // xla/g3doc/custom_call.md.
  switch (custom_call.api_version()) {
    case CustomCallApiVersion::API_VERSION_ORIGINAL:
      using original_call_type =
          void (*)(CustomCallThunk::Stream /*stream*/, void** /*buffers*/,
                   const char* /*opaque*/, size_t /*opaque_len*/);
      custom_call_target = [call_target](CustomCallThunk::Stream stream,
                                         void** buffers, const char* opaque,
                                         size_t opaque_len,
                                         XlaCustomCallStatus*) {
        auto typed_call_target =
            reinterpret_cast<original_call_type>(call_target);
        typed_call_target(stream, buffers, opaque, opaque_len);
      };
      break;
    case CustomCallApiVersion::API_VERSION_STATUS_RETURNING:
    case CustomCallApiVersion::API_VERSION_STATUS_RETURNING_UNIFIED:
      using status_returning_call_type =
          void (*)(CustomCallThunk::Stream /*stream*/, void** /*buffers*/,
                   const char* /*opaque*/, size_t /*opaque_len*/,
                   XlaCustomCallStatus* /*status*/);
      custom_call_target =
          reinterpret_cast<status_returning_call_type>(call_target);
      break;
    case CustomCallApiVersion::API_VERSION_TYPED_FFI:
      // We already checked `handler` above.
      break;
    default:
      return Internal("Unknown custom-call API version enum value: %d",
                      custom_call.api_version());
  }

  auto& backend_config_str = custom_call.raw_backend_config_string();
  switch (custom_call.api_version()) {
    case CustomCallApiVersion::API_VERSION_ORIGINAL:
    case CustomCallApiVersion::API_VERSION_STATUS_RETURNING:
    case CustomCallApiVersion::API_VERSION_STATUS_RETURNING_UNIFIED:
      if (!backend_config_str.empty()) {
        opaque = backend_config_str;
      }
      break;

    case CustomCallApiVersion::API_VERSION_TYPED_FFI:
      if (!backend_config_str.empty()) {
        mlir::Attribute attr = mlir::parseAttribute(
            backend_config_str, ir_emitter_context.mlir_context());
        if (auto dict = attr.dyn_cast_or_null<mlir::DictionaryAttr>()) {
          TF_ASSIGN_OR_RETURN(attributes, BuildAttributesMap(dict));
          break;
        }
        return absl::InternalError(
            "Unsupported backend config. Expected a string parsable into "
            "dictionary attribute");
      }
      break;

    default:
      return Internal("Unknown custom-call API version enum value: %d",
                      custom_call.api_version());
  }

  auto ffi_thunk = [&] {
    auto& called_computations = custom_call.called_computations();
    return std::make_unique<CustomCallThunk>(
        Thunk::ThunkInfo::WithProfileAnnotation(&custom_call),
        registration->handler, std::move(operands), std::move(results),
        std::move(attributes),
        called_computations.empty() ? nullptr : called_computations[0]);
  };

  auto legacy_thunk = [&] {
    return std::make_unique<CustomCallThunk>(
        Thunk::ThunkInfo::WithProfileAnnotation(&custom_call),
        std::move(custom_call_target), std::move(operands), std::move(results),
        std::move(opaque));
  };
  FusionEmissionResult result;
  result.thunks.push_back(found_ffi_handler ? ffi_thunk() : legacy_thunk());
  return result;
}

}  // namespace

absl::StatusOr<FusionEmissionResult> CustomFusion::Emit(
    IrEmitterContext& ir_emitter_context,
    const HloFusionInstruction& fusion) const {
  TF_ASSIGN_OR_RETURN(auto gpu_config,
                      fusion.backend_config<GpuBackendConfig>());
  const FusionBackendConfig& backend_config =
      gpu_config.fusion_backend_config();
  const auto& config = backend_config.custom_fusion_config();

  VLOG(3) << "Lower HLO fusion to a custom fusion " << config.name();

  auto* registry = CustomKernelFusionRegistry::Default();
  auto* custom_kernel_fusion = registry->Lookup(config.name());

  // If custom fusion is not found it means that some of the build targets might
  // not be statically linked into the binary.
  if (custom_kernel_fusion == nullptr) {
    return absl::InternalError(
        absl::StrCat("Custom kernel fusion ", config.name(),
                     " not found in a default registry."));
  }

  // Load custom kernels that can implement a fusion computation.
  TF_ASSIGN_OR_RETURN(std::vector<CustomKernel> kernels,
                      custom_kernel_fusion->LoadKernels(
                          ir_emitter_context.gpu_device_info(),
                          fusion.fused_instructions_computation()));

  // This should never happen, it means that compilation pipeline created a
  // fusion operation that is not supported by a given custom fusion.
  if (kernels.empty()) {
    return absl::InternalError(
        absl::StrCat("Custom kernel fusion ", config.name(),
                     " returned empty custom kernels for a fused computation"));
  }

  // TODO(ezhulenev): Add support for auto tuning to select the best kernel.
  if (kernels.size() != 1) {
    return absl::InternalError("Expected exactly one custom kernel");
  }

  TF_ASSIGN_OR_RETURN(
      auto thunk, BuildCustomKernelThunkForFusion(ir_emitter_context, fusion,
                                                  std::move(kernels[0])));

  FusionEmissionResult result;
  result.thunks.push_back(std::move(thunk));
  return result;
}

absl::StatusOr<FusionEmissionResult> AddressComputationFusion::Emit(
    IrEmitterContext& ir_emitter_context,
    const HloFusionInstruction& fusion) const {
  const HloFusionAdaptor& adaptor = analysis_.fusion();
  auto maybe_custom_call_adaptor = HloFindIf(
      adaptor.GetRoots(), adaptor,
      [](auto node) { return node.opcode() == HloOpcode::kCustomCall; });
  if (maybe_custom_call_adaptor == std::nullopt) {
    return absl::InternalError(
        "AddressComputationFusion requires a CustomCall hero");
  }

  const auto& custom_call = *static_cast<const HloCustomCallInstruction*>(
      &maybe_custom_call_adaptor->instruction());
  // TODO(vuson): these Emit* are mostly duplicated from ir_emitter_unnested
  if (IsLegacyCublasMatmul(custom_call)) {
    return EmitGemm(ir_emitter_context, adaptor, fusion, custom_call);
  }

  return EmitCustomCall(ir_emitter_context, adaptor, fusion, custom_call);
}

absl::StatusOr<FusionEmissionResult> DynamicAddressComputationFusion::Emit(
    IrEmitterContext& ir_emitter_context,
    const HloFusionInstruction& fusion) const {
  const HloFusionAdaptor& adaptor = analysis_.fusion();
  auto maybe_custom_call_adaptor = HloFindIf(
      adaptor.GetRoots(), adaptor,
      [](auto node) { return node.opcode() == HloOpcode::kCustomCall; });
  if (maybe_custom_call_adaptor == std::nullopt) {
    return absl::InternalError(
        "DynamicAddressComputationFusion requires a CustomCall hero");
  }

  const auto& custom_call = *static_cast<const HloCustomCallInstruction*>(
      &maybe_custom_call_adaptor->instruction());
  if (IsLegacyCublasMatmul(custom_call)) {
    return EmitDynamicSlicedGemm(ir_emitter_context, adaptor, fusion,
                                 custom_call);
  }

  return absl::UnimplementedError(absl::StrCat(
      "No emission for DynamicAddressComputationFusion of custom call ",
      custom_call.custom_call_target()));
}

}  // namespace gpu
}  // namespace xla
