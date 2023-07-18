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
#include "tensorflow/tools/proto_splitter/cc/repeated_field_splitter.h"

#include <memory>
#include <vector>

#include "absl/status/status.h"
#include "absl/status/statusor.h"
#include "tensorflow/core/framework/function.pb.h"
#include "tensorflow/core/framework/graph.pb.h"
#include "tensorflow/core/framework/node_def.pb.h"
#include "tensorflow/tools/proto_splitter/cc/max_size.h"
#include "tensorflow/tools/proto_splitter/cc/util.h"
#include "tensorflow/tsl/platform/errors.h"
#include "tensorflow/tsl/platform/protobuf.h"
#include "tensorflow/tsl/platform/statusor.h"

namespace tensorflow {
namespace tools::proto_splitter {

template <typename ParentMessage, typename RepeatedMessage>
absl::StatusOr<RepeatedFieldSplitters<ParentMessage, RepeatedMessage>>
RepeatedFieldSplitters<ParentMessage, RepeatedMessage>::Create(
    tsl::protobuf::Message* message, ComposableSplitter* parent_splitter,
    std::vector<FieldType>* fields_in_parent, const FieldType& repeated_field,
    std::vector<SizeSplitterFactory*>* splitter_factories) {
  // std::vector<FieldType> all_fields = *fields_in_parent;
  // all_fields.push_back(repeated_field);
  // std::vector<FieldType>

  TF_ASSIGN_OR_RETURN(auto field_ret, GetField(*message, {repeated_field}));
  if (!field_ret.field->is_repeated()) {
    return absl::FailedPreconditionError("Unable to split non-repeated field.");
  }

  auto ret = RepeatedFieldSplitters<ParentMessage, RepeatedMessage>(
      message, parent_splitter, fields_in_parent, repeated_field,
      splitter_factories);
  return ret;
}

template <typename ParentMessage, typename RepeatedMessage>
absl::StatusOr<int> RepeatedFieldSplitters<
    ParentMessage, RepeatedMessage>::BuildChunksReturnSize() {
  // std::vector<FieldType> all_fields = *fields_in_parent();
  // all_fields.push_back(repeated_field_);

  TF_ASSIGN_OR_RETURN(auto ret, GetMutableField(message(), {repeated_field_}));

  uint64_t max_size = GetMaxSize();
  size_t initial_size = GetInitialSize();

  // List of indices at which to split the repeated field. For example, [3, 5]
  // means that the field list is split into: [:3], [3:5], [5:]
  std::vector<int> repeated_msg_split = {};
  // Should be the same length as the list above. Contains new protos to hold
  // the elements that are split from the original proto.
  // From the [3, 5] example above, the messages in this list contain nodes
  // [3:5] and [5:]
  std::vector<std::shared_ptr<ParentMessage>> repeated_new_msg;
  // Track the total size of the current node split.
  uint64_t total_size = 0;

  // Linearly iterate through all nodes. It may be possible to optimize this
  // further by making best guesses as to where to split the nodes, since
  // most nodes (aside from constants) are relatively small.
  int repeated_field_size =
      ret.parent->GetReflection()->FieldSize(*ret.parent, ret.field);
  for (int i = 0; i < repeated_field_size; ++i) {
    tsl::protobuf::Message* node =
        ret.parent->GetReflection()->MutableRepeatedMessage(ret.parent,
                                                            ret.field, i);
    auto node_size = node->ByteSizeLong();

    std::vector<FieldType> new_fields = {repeated_field_, i};

    for (auto factory : *splitter_factories_) {
      TF_ASSIGN_OR_RETURN(
          std::unique_ptr<SizeSplitter> new_splitter,
          factory->CreateSplitter(node, this, &new_fields, node_size));
      if (new_splitter != nullptr) {
        TF_ASSIGN_OR_RETURN(auto size_diff,
                            new_splitter->BuildChunksReturnSize());
        node_size -= size_diff;
      }
    }
    if (total_size + node_size > max_size) {
      repeated_msg_split.push_back(i);
      auto new_chunk = std::make_shared<ParentMessage>();
      repeated_new_msg.push_back(new_chunk);
      std::vector<FieldType> empty_fields = {};
      auto x = std::make_unique<MessageBytes>(new_chunk);
      TF_RETURN_IF_ERROR(AddChunk(std::move(x), &empty_fields));
      total_size = 0;
    }
    total_size += node_size;
  }

  if (!repeated_msg_split.empty()) {
    auto repeated_nodes_ptrs =
        ret.parent->GetReflection()
            ->template MutableRepeatedPtrField<RepeatedMessage>(ret.parent,
                                                                ret.field);

    int start = repeated_msg_split[0];

    std::vector<RepeatedMessage*> extracted_nodes;
    extracted_nodes.resize(repeated_field_size - start);
    repeated_nodes_ptrs->ExtractSubrange(start, repeated_field_size - start,
                                         &extracted_nodes.at(0));
    repeated_msg_split.push_back(repeated_field_size);
    auto extracted_node = extracted_nodes.begin();

    for (int i = 1; i < repeated_msg_split.size(); ++i) {
      start = repeated_msg_split[i - 1];
      int end = repeated_msg_split[i];
      std::shared_ptr<ParentMessage> new_msg = repeated_new_msg[i - 1];

      // Move nodes into new_msg.
      TF_ASSIGN_OR_RETURN(auto new_ret,
                          GetMutableField(new_msg.get(), repeated_field_));

      for (int j = 0; j < end - start; ++j) {
        new_msg->GetReflection()->AddAllocatedMessage(
            new_msg.get(), new_ret.field, *extracted_node++);
      }
    }
  }

  // Estimate the size diff by subtracting the first computed chunk size from
  // the initial size of the repeated field.
  return initial_size - message()->ByteSizeLong();
}

// Declare template classes to fix linking error.
template class RepeatedFieldSplitters<GraphDef, NodeDef>;
template class RepeatedFieldSplitters<FunctionDefLibrary, FunctionDef>;
template class RepeatedFieldSplitters<FunctionDef, NodeDef>;

}  // namespace tools::proto_splitter
}  // namespace tensorflow
