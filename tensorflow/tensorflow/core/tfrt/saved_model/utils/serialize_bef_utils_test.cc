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

#include "tensorflow/core/tfrt/saved_model/utils/serialize_bef_utils.h"

#include <memory>
#include <string>

#include <gmock/gmock.h>
#include <gtest/gtest.h>
#include "mlir/Parser/Parser.h"  // from @llvm-project
#include "tensorflow/compiler/mlir/tensorflow/dialect_registration.h"
#include "tensorflow/compiler/mlir/tfrt/translate/import_model.h"
#include "tensorflow/core/platform/path.h"
#include "tensorflow/core/platform/resource_loader.h"
#include "tensorflow/core/tfrt/saved_model/saved_model_testutil.h"
#include "tensorflow/core/tfrt/utils/utils.h"
#include "tensorflow/tsl/lib/core/status_test_util.h"
#include "tfrt/bef/bef_buffer.h"  // from @tf_runtime

namespace tensorflow {
namespace tfrt_stub {
namespace {

TEST(SerializeBEFTest, HandlesCompleteProcess) {
  // Create Empty BEF Buffer
  tfrt::BefBuffer old_bef;

  // Load BEF Buffer Data

  const std::string saved_model_mlir_path =
      "third_party/tensorflow/compiler/mlir/tfrt/tests/saved_model/testdata/"
      "test.mlir";

  mlir::DialectRegistry registry;
  mlir::RegisterAllTensorFlowDialects(registry);
  mlir::MLIRContext context(registry);
  auto module =
      mlir::parseSourceFile<mlir::ModuleOp>(saved_model_mlir_path, &context);
  ASSERT_TRUE(module);

  std::unique_ptr<Runtime> runtime =
      tensorflow::tfrt_stub::Runtime::Create(/*num_inter_op_threads=*/1);
  tfrt_stub::GraphExecutionOptions options(runtime.get());
  tfrt::ResourceContext resource_context;
  tfrt_stub::ModelRuntimeContext model_context(
      &options, options.compile_options.saved_model_dir, &resource_context);
  TF_ASSERT_OK(ConvertTfMlirToBef(options.compile_options, module.get(),
                                  &old_bef, model_context));

  // Create Filepath for .mlir.bef
  const std::string filepath =
      io::JoinPath(getenv("TEST_UNDECLARED_OUTPUTS_DIR"),
                   std::string("serialized_bef.mlir.bef"));

  // Serialize BEF Buffer
  TF_ASSERT_OK(tensorflow::tfrt_stub::SerializeBEF(old_bef, filepath));
  // Check that Bef Buffer is not empty
  ASSERT_NE(old_bef.size(), 0);

  // Create new empty BEF buffer and deserialize to verify data

  TF_ASSERT_OK_AND_ASSIGN(const tfrt::BefBuffer bef,
                          DeserializeBEFBuffer(filepath));

  // Check for any data loss during deserialization process
  ASSERT_TRUE(old_bef.size() == bef.size());

  // Check file creation
  std::unique_ptr<Runtime> default_runtime =
      DefaultTfrtRuntime(/*num_threads=*/1);
  SavedModel::Options default_options =
      DefaultSavedModelOptions(default_runtime.get());
  TF_EXPECT_OK(tfrt::CreateBefFileFromBefBuffer(
                   *default_options.graph_execution_options.runtime, bef)
                   .status());
}
}  // namespace
}  // namespace tfrt_stub
}  // namespace tensorflow
