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

#ifndef TENSORFLOW_COMPILER_XLA_SERVICE_COLLECTIVE_PERMUTE_DECOMPOSER_H_
#define TENSORFLOW_COMPILER_XLA_SERVICE_COLLECTIVE_PERMUTE_DECOMPOSER_H_

#include "tensorflow/compiler/xla/hlo/ir/hlo_module.h"
#include "tensorflow/compiler/xla/service/hlo_pass_interface.h"

namespace xla {

// CollectivePermuteDecomposer is a pass that converts asynchronous
// CollectivePermute operations without any cycle in the (source, target)
// relationship to Send/Recv. We currently restrict this transformation to
// CollectivePermuteStart with one input and without any context data.
//
// before transformation:
//     start = (<rt>, <rt>) collective-permute-start(data),
//       source_target_pairs={...}
//     done = <rt> collective-permute-done(start)
//
// after transformation:
//    after-all = token[] after-all()
//    recv = (<rt>, token[]) recv(after-all), channel_id=0,
//     frontend_attributes={_xla_send_recv_source_target_pairs="{...}"}
//    send = (<rt>, token[]) send(data, after-all), channel_id=0,
//      control-predecessors={recv}, frontend_attributes={
//      _xla_send_recv_source_target_pairs="{...}"}
//    recv-done = (<rt>, token[]) recv-done(recv), channel_id=0
//    send-done = token[] send-done(send), channel_id=0,
//      control-predecessors={recv-done}
//    done = <rt> get-tuple-element(recv-done), index=0
//
class CollectivePermuteDecomposer : public HloModulePass {
 public:
  absl::string_view name() const override {
    return "collective-permute-decomposer";
  }

  using HloPassInterface::Run;
  // Runs CollectivePermuteDecomposer pass on computations in 'module'.
  // Returns whether the 'module' was changed.
  StatusOr<bool> Run(
      HloModule* module,
      const absl::flat_hash_set<absl::string_view>& execution_threads) override;
};

}  // namespace xla

#endif  // TENSORFLOW_COMPILER_XLA_SERVICE_COLLECTIVE_PERMUTE_DECOMPOSER_H_
