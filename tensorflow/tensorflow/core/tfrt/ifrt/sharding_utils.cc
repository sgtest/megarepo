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
#define EIGEN_USE_THREADS

#include "tensorflow/core/tfrt/ifrt/sharding_utils.h"

#include <algorithm>
#include <cstdint>
#include <cstdlib>
#include <memory>
#include <optional>
#include <utility>
#include <vector>

#include "absl/container/btree_map.h"
#include "absl/log/log.h"
#include "absl/status/status.h"
#include "absl/status/statusor.h"
#include "absl/strings/str_cat.h"
#include "absl/types/span.h"
#include "unsupported/Eigen/CXX11/Tensor"  // from @eigen_archive
#include "tensorflow/compiler/tf2xla/type_util.h"
#include "xla/python/ifrt/array.h"
#include "xla/python/ifrt/client.h"
#include "xla/python/ifrt/device.h"
#include "xla/python/ifrt/dtype.h"
#include "xla/python/ifrt/index_domain.h"
#include "xla/python/ifrt/memory.h"
#include "xla/python/ifrt/shape.h"
#include "xla/python/ifrt/sharding.h"
#include "xla/python/pjrt_ifrt/pjrt_array.h"
#include "tensorflow/core/framework/register_types.h"
#include "tensorflow/core/framework/tensor.h"
#include "tensorflow/core/framework/tensor_shape.h"
#include "tensorflow/core/framework/types.h"
#include "tensorflow/core/framework/types.pb.h"
#include "tensorflow/core/platform/status.h"
#include "tensorflow/core/platform/statusor.h"
#include "tensorflow/core/tpu/kernels/sharding_utils.h"
#include "tsl/concurrency/ref_count.h"
#include "tsl/platform/errors.h"
#include "tsl/platform/statusor.h"

namespace tensorflow {
namespace ifrt_serving {
namespace {
absl::StatusOr<xla::ifrt::DType> ToIfrtDType(
    tensorflow::DataType tensor_dtype) {
  xla::PrimitiveType primitive_type;
  TF_RETURN_IF_ERROR(
      tensorflow::DataTypeToPrimitiveType(tensor_dtype, &primitive_type));
  return xla::ifrt::ToDType(primitive_type);
}

// Shard the given `input_tensor` into equal shapes of slices.
//
// `num_paritions_per_axis` specifies the number of partitions along
// each axis (dimension).
//
// `num_replicas` specifies the number of replication for each partitioned
// sliced buffer.
//
// `devices` contains a list of devices flattend into the following
// order: [slice0][replicate0], [slice0][replicate1], ..., [slice1][replicate0],
// [slice1][replicate1], ...
absl::StatusOr<std::vector<tsl::RCReference<xla::ifrt::Array>>>
SplitAndCreateArraysFromHostBuffer(
    xla::ifrt::Client& ifrt_client, const tensorflow::Tensor& input_tensor,
    const std::vector<int32_t>& num_partitions_per_axis, int num_replicas,
    const std::vector<xla::ifrt::Device*>& devices,
    const Eigen::ThreadPoolDevice& thread_pool_device) {
  int64_t num_slices = 1;
  for (auto k : num_partitions_per_axis) {
    num_slices *= k;
  }

  tensorflow::DataType tensor_data_type = input_tensor.dtype();
  std::vector<int32_t> paddings(num_partitions_per_axis.size(), 0);
  std::vector<tensorflow::Tensor> split_tensors;
  split_tensors.resize(num_slices);

  auto allocate_output_fn =
      [&](int i, const tensorflow::TensorShape& output_slice_shape,
          tensorflow::Tensor** tensor) {
        if (i < 0 || i >= split_tensors.size()) {
          return absl::InvalidArgumentError(absl::StrCat(
              "Index ", i, " out of range [0, ", split_tensors.size(), "]"));
        }
        split_tensors[i] =
            tensorflow::Tensor(tensor_data_type, output_slice_shape);
        *tensor = &split_tensors[i];
        return absl::OkStatus();
      };

  // Fast path for output in the simple no split case.
  auto assign_or_copy_value_fn =
      [&](const tensorflow::Tensor& input) -> Status {
    split_tensors[0] = input;
    return absl::OkStatus();
  };

  // XlaNDSplitter only support rank (0, 8] as there is no concept of split for
  // rank 0 tensor.
  if (input_tensor.shape().dims() == 0) {
    if (split_tensors.size() != 1) {
      return absl::InvalidArgumentError(absl::StrCat(
          "Rank 0 tensor only expects 1 slice but got ", split_tensors.size()));
    }
    split_tensors[0] = input_tensor;
  } else {
    switch (input_tensor.dtype()) {
#define CASE(type)                                                             \
  case tensorflow::DataTypeToEnum<type>::value: {                              \
    TF_ASSIGN_OR_RETURN(auto splitter,                                         \
                        (XlaNDSplitter<Eigen::ThreadPoolDevice, type>::Create( \
                            num_partitions_per_axis, num_slices, paddings,     \
                            /*has_paddings=*/false)));                         \
    TF_RETURN_IF_ERROR(                                                        \
        splitter.Split(&input_tensor, "input tensor", assign_or_copy_value_fn, \
                       allocate_output_fn, thread_pool_device));               \
  } break;
    TF_CALL_ALL_TYPES(CASE);
    TF_CALL_quint8(CASE);
#undef CASE
    default:
      return absl::InvalidArgumentError("Unsupported data type");
    }
  }

  if (split_tensors.size() * num_replicas != devices.size()) {
    return absl::InvalidArgumentError(
        absl::StrCat("Expect ", devices.size(), " but got ",
                     split_tensors.size(), " x ", num_replicas));
  }

  std::vector<tsl::RCReference<xla::ifrt::Array>> arrays;
  arrays.reserve(devices.size());
  TF_ASSIGN_OR_RETURN(xla::ifrt::DType dtype, ToIfrtDType(tensor_data_type));
  auto device_iter = devices.begin();
  for (int slice_idx = 0; slice_idx < split_tensors.size(); ++slice_idx) {
    auto& tensor = split_tensors[slice_idx];

    for (int i = 0; i < num_replicas; ++i) {
      VLOG(2) << "Make array for buffer slice " << slice_idx << " at "
              << tensor.data();
      if (device_iter == devices.end()) {
        return absl::InternalError(
            absl::StrCat("Missing Device ", i, " for slice ", slice_idx));
      }
      auto single_device_sharding = xla::ifrt::SingleDeviceSharding::Create(
          *device_iter, xla::ifrt::MemoryKind());

      TF_ASSIGN_OR_RETURN(
          auto array,
          ifrt_client.MakeArrayFromHostBuffer(
              tensor.data(), dtype,
              xla::ifrt::Shape(tensor.shape().dim_sizes()),
              /*byte_strides=*/{}, std::move(single_device_sharding),
              xla::ifrt::Client::HostBufferSemantics::
                  kImmutableUntilTransferCompletes,
              [tensor, slice_idx]() {
                // Keep tensor alive
                LOG(INFO) << "Done with host buffer for slice " << slice_idx
                          << " at " << tensor.data();
              }));
      arrays.push_back(std::move(array));
      device_iter++;
    }
  }
  return arrays;
}

absl::StatusOr<int> VerifyIndexDomainsAndGetReplicas(
    absl::Span<xla::ifrt::IndexDomain> index_domains,
    const tensorflow::TensorShape& tensor_shape) {
  if (index_domains.size() <= 1) {
    return absl::InvalidArgumentError(absl::StrCat(
        "Expect multiple index domains but got ", index_domains.size()));
  }

  for (auto index_domain = index_domains.begin();
       index_domain < index_domains.end(); ++index_domain) {
    if (index_domain->shape().dims().size() != tensor_shape.dims()) {
      return absl::InvalidArgumentError(
          absl::StrCat("Expect equal rank of ", tensor_shape.dims(),
                       " but got ", index_domain->shape().dims().size()));
    }
  }

  // Only support equal shape for all index domains
  auto first_index_domain = index_domains.begin();
  for (auto index_domain = index_domains.begin() + 1;
       index_domain < index_domains.end(); ++index_domain) {
    if (first_index_domain->shape() != index_domain->shape()) {
      return absl::UnimplementedError(absl::StrCat(
          "Expect equal shape of ", first_index_domain->shape().DebugString(),
          " but  got ", index_domain->shape().DebugString()));
    }
  }

  // Verify that each `IndexDomain` appear the same `num_replica` times. Since
  // shapes are the same for all `IndexDomain`, this also implies each `origin`
  // appear `num_replica` times.
  auto index_domain_lexicographical_comparator =
      [](const xla::ifrt::IndexDomain& a, const xla::ifrt::IndexDomain& b) {
        return std::lexicographical_compare(
            a.origin().elements().begin(), a.origin().elements().end(),
            b.origin().elements().begin(), b.origin().elements().end());
      };
  absl::btree_map<xla::ifrt::IndexDomain, int,
                  decltype(index_domain_lexicographical_comparator)>
      index_domain_counts;
  for (const auto& index_domain : index_domains) {
    index_domain_counts[index_domain]++;
  }

  std::vector<xla::ifrt::IndexDomain> unique_index_domains;
  unique_index_domains.reserve(index_domain_counts.size());
  int num_replicas = index_domain_counts.begin()->second;
  for (const auto& [index_domain, count] : index_domain_counts) {
    if (count != num_replicas) {
      return absl::FailedPreconditionError(absl::StrCat(
          "Expected ", num_replicas, " replicas for ",
          index_domain.DebugString(), " but got ", count, " replicas"));
    }
    unique_index_domains.push_back(index_domain);
  }

  // Verify that distances of between origins of neighbouring `IndexDomain`
  // bounded by shape. Note that unique_indexx_domains are already in sorted
  // order.
  auto prev_iter = unique_index_domains.begin();
  auto next_iter = unique_index_domains.begin() + 1;
  const auto& bounded_box = first_index_domain->shape();
  while (prev_iter != unique_index_domains.end() &&
         next_iter != unique_index_domains.end()) {
    xla::ifrt::Index offset = next_iter->origin() - prev_iter->origin();
    for (int dim = 0; dim < bounded_box.dims().size(); ++dim) {
      if (std::abs(offset.elements()[dim]) != bounded_box.dims()[dim] &&
          offset.elements()[dim] != 0) {
        return absl::FailedPreconditionError(absl::StrCat(
            "IndexDomains should not have gap or overlap, but got ",
            prev_iter->DebugString(), " and ", next_iter->DebugString(),
            " that have offset of ", offset.DebugString()));
      }
    }
    prev_iter = next_iter;
    next_iter++;
  }

  // Verify the last `IndexDomain`'s upper end of the bound matches with the
  // tensor shape. Together with the above check, this provides an approximation
  // to the following two assumptions:
  // 1. the union of all IndexDomain covers the entire global shape array with
  // no gaps.
  // 2. no two index_domain have any overlap.
  std::vector<int64_t> bounded_shape;
  const auto& last_index_domain = unique_index_domains.back();
  bounded_shape.reserve(last_index_domain.shape().dims().size());
  for (int d = 0; d < last_index_domain.shape().dims().size(); ++d) {
    bounded_shape.push_back(last_index_domain.origin().elements()[d] +
                            last_index_domain.shape().dims()[d]);
  }

  if (xla::ifrt::Shape(bounded_shape) !=
      xla::ifrt::Shape(tensor_shape.dim_sizes())) {
    return absl::FailedPreconditionError(absl::StrCat(
        "IndexDomain ", last_index_domain.DebugString(),
        " does not overlap with tensor shape ", tensor_shape.DebugString()));
  }

  return num_replicas;
}

}  // namespace

StatusOr<tsl::RCReference<xla::ifrt::Array>> MakeAssembledArrayFromHostBuffer(
    xla::ifrt::Client& ifrt_client, const tensorflow::Tensor& input_tensor,
    std::shared_ptr<xla::ifrt::Sharding> sharding,
    const Eigen::ThreadPoolDevice& thread_pool_device) {
  VLOG(2) << "Assembling arrays by sharding " << sharding->DebugString();

  TF_ASSIGN_OR_RETURN(auto index_domains,
                      sharding->IndexDomains(
                          xla::ifrt::Shape(input_tensor.shape().dim_sizes())));

  TF_ASSIGN_OR_RETURN(int index_domain_replicas,
                      VerifyIndexDomainsAndGetReplicas(
                          absl::MakeSpan(index_domains), input_tensor.shape()));

  const auto& first_index_domain = index_domains.begin();
  std::vector<int32_t> num_partitions_per_axis;
  int total_num_partitions = 1;
  num_partitions_per_axis.reserve(input_tensor.shape().dims());
  for (int dim = 0; dim < input_tensor.shape().dims(); ++dim) {
    int target_size = first_index_domain->shape().dims()[dim];
    if (input_tensor.shape().dim_size(dim) % target_size != 0) {
      return absl::FailedPreconditionError(absl::StrCat(
          "Only support even sharding, but input tensor shape ",
          input_tensor.shape().DebugString(), " not even splittable to ",
          first_index_domain->shape().DebugString()));
    }
    int num_partitions = input_tensor.shape().dim_size(dim) / target_size;
    total_num_partitions *= num_partitions;
    num_partitions_per_axis.push_back(num_partitions);
  }

  if (total_num_partitions > sharding->devices().size() ||
      sharding->devices().size() % total_num_partitions != 0) {
    return absl::UnimplementedError(absl::StrCat(
        "Number of devices ", sharding->devices().size(),
        " not a multiple of number of partitions", total_num_partitions));
  }

  // Assume index domains are non-overlapping and each index domain appears
  // exactly num_replicates times. This allows us to rely on
  // lexicographical sorting to replicate slices in the correct order.
  int num_replicas = sharding->devices().size() / total_num_partitions;
  if (index_domain_replicas != num_replicas) {
    return absl::FailedPreconditionError(
        absl::StrCat("IndexDomain indicates ", index_domain_replicas,
                     " replicas, but got ", num_replicas, " replicas"));
  }

  // Sorted the IndexDomain and devices from major to minor dimenson. For
  // example, a two dimension IndexDomain will be ordered by [0, 0], [0, 1], [1,
  // 0], [1, 1].
  // This is O(n*log(n)) vs looking for devices individually which is O(n^2).
  struct IndexDomainDevice {
    xla::ifrt::IndexDomain index_domain;
    xla::ifrt::Device* device;
    // The index of this `device`/`index_domain` in the
    // sharding.devices/index_domains.
    int original_shard_index;
  };
  std::vector<IndexDomainDevice> index_domain_devices;
  index_domain_devices.reserve(index_domains.size());
  for (int i = 0; i < index_domains.size(); ++i) {
    index_domain_devices.push_back(
        {index_domains[i], sharding->devices()[i], i});
  }
  std::sort(index_domain_devices.begin(), index_domain_devices.end(),
            [](const IndexDomainDevice& a, const IndexDomainDevice& b) {
              return std::lexicographical_compare(
                  a.index_domain.origin().elements().begin(),
                  a.index_domain.origin().elements().end(),
                  b.index_domain.origin().elements().begin(),
                  b.index_domain.origin().elements().end());
            });
  // Now the devices is in order.
  std::vector<xla::ifrt::Device*> devices;
  devices.reserve(index_domain_devices.size());
  std::vector<int> original_device_indices;
  original_device_indices.reserve(index_domain_devices.size());
  for (auto& [index_domain, device, original_device_index] :
       index_domain_devices) {
    devices.push_back(device);
    original_device_indices.push_back(original_device_index);
    VLOG(3) << "Device " << device->ToString();
  }

  TF_ASSIGN_OR_RETURN(auto arrays,
                      SplitAndCreateArraysFromHostBuffer(
                          ifrt_client, input_tensor, num_partitions_per_axis,
                          num_replicas, devices, thread_pool_device));

  // Re-arranged arrays back to original device order
  std::vector<tsl::RCReference<xla::ifrt::Array>> rearranged_arrays;
  rearranged_arrays.resize(arrays.size());
  for (int i = 0; i < arrays.size(); ++i) {
    rearranged_arrays[original_device_indices[i]] = std::move(arrays[i]);
  }

  return ifrt_client.AssembleArrayFromSingleDeviceArrays(
      xla::ifrt::Shape(input_tensor.shape().dim_sizes()), sharding,
      absl::MakeSpan(rearranged_arrays),
      xla::ifrt::ArrayCopySemantics::kDonateInput);
}

}  // namespace ifrt_serving
}  // namespace tensorflow
