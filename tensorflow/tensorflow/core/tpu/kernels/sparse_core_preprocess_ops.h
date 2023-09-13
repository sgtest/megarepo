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
#ifndef TENSORFLOW_CORE_TPU_KERNELS_SPARSE_CORE_PREPROCESS_OPS_H_
#define TENSORFLOW_CORE_TPU_KERNELS_SPARSE_CORE_PREPROCESS_OPS_H_

#include <string>

#include "tensorflow/core/framework/op_kernel.h"
#include "tensorflow/core/framework/tensor.h"

namespace tensorflow {

// Struct to describe an embedding lookup input data.
struct EmbeddingLookupInput {
  // Which replica it belongs.
  int32 replica_id;
  // Token id.
  int32 token_id;
  // Sample id.
  int32 sample_id;
  // Gain.
  float gain;

  EmbeddingLookupInput(int32 replica_id, int32 token_id, int32 sample_id,
                       float gain)
      : replica_id(replica_id),
        token_id(token_id),
        sample_id(sample_id),
        gain(gain) {}
};

class GetMinibatchesInCsrWithPhysicalReplicaOp : public OpKernel {
 public:
  explicit GetMinibatchesInCsrWithPhysicalReplicaOp(OpKernelConstruction* ctx);
  ~GetMinibatchesInCsrWithPhysicalReplicaOp() override = default;
  GetMinibatchesInCsrWithPhysicalReplicaOp(
      const GetMinibatchesInCsrWithPhysicalReplicaOp&) = delete;
  GetMinibatchesInCsrWithPhysicalReplicaOp& operator=(
      const GetMinibatchesInCsrWithPhysicalReplicaOp&) = delete;

  void Compute(OpKernelContext* ctx) override;

 protected:
  virtual void GetMaxIdsAndUniques(OpKernelContext* ctx,
                                   const std::string& program_key,
                                   const std::string& table_name,
                                   int64_t num_samples_per_sparse_core,
                                   int64_t feature_width,
                                   int64_t* max_ids_per_partition,
                                   int64_t* max_unique_ids_per_partition) {}

  int sample_count_ = 1;
  int feature_width_ = 1;
  int64_t num_sc_per_chip_;
  std::string table_name_;

 private:
  int num_replica_ = 1;
  int max_minibatches_per_sc_ = 1;
  int max_ids_per_chip_per_sample_ = 1;
  int table_vocab_size_ = 1;
  std::string device_name_;
};

}  // namespace tensorflow

#endif  // TENSORFLOW_CORE_TPU_KERNELS_SPARSE_CORE_PREPROCESS_OPS_H_
