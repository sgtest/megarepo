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

#include "tensorflow/compiler/mlir/tfrt/transforms/ifrt/rewrite_cluster_to_ifrt_call.h"

#include <cstdint>
#include <memory>
#include <vector>

#include "absl/base/casts.h"
#include "absl/strings/str_cat.h"
#include "llvm/ADT/APInt.h"
#include "llvm/ADT/DenseMap.h"
#include "llvm/ADT/SmallVector.h"
#include "mlir/Dialect/Func/IR/FuncOps.h"  // from @llvm-project
#include "mlir/IR/Attributes.h"  // from @llvm-project
#include "mlir/IR/Builders.h"  // from @llvm-project
#include "mlir/IR/BuiltinAttributes.h"  // from @llvm-project
#include "mlir/IR/BuiltinOps.h"  // from @llvm-project
#include "mlir/IR/DialectRegistry.h"  // from @llvm-project
#include "mlir/IR/IRMapping.h"  // from @llvm-project
#include "mlir/IR/Operation.h"  // from @llvm-project
#include "mlir/IR/SymbolTable.h"  // from @llvm-project
#include "mlir/IR/Value.h"  // from @llvm-project
#include "mlir/Pass/Pass.h"  // from @llvm-project
#include "mlir/Support/TypeID.h"  // from @llvm-project
#include "tensorflow/compiler/mlir/tensorflow/ir/tf_device.h"
#include "tensorflow/compiler/mlir/tensorflow/ir/tf_ops.h"
#include "tensorflow/compiler/mlir/tensorflow/ir/tfrt_ops.h"
#include "tensorflow/core/platform/random.h"

namespace tensorflow {
namespace ifrt_serving {
namespace {

// A pass that inserts tf.ifrt_call and create its callee as a Ifrt
// Program.
class RewriteClusterToIfrtCallPass
    : public mlir::PassWrapper<RewriteClusterToIfrtCallPass,
                               mlir::OperationPass<mlir::ModuleOp>> {
 public:
  RewriteClusterToIfrtCallPass() = default;
  RewriteClusterToIfrtCallPass &operator=(
      const RewriteClusterToIfrtCallPass &) = delete;

  MLIR_DEFINE_EXPLICIT_INTERNAL_INLINE_TYPE_ID(RewriteClusterToIfrtCallPass)

 private:
  // Returns a new unique program id.
  static int64_t NewProgramId() {
    const uint64_t id = static_cast<int64_t>(tensorflow::random::New64());
    // We use a signed int for program ids since TensorFlow doesn't
    // support uint64_t attributes.
    return absl::bit_cast<int64_t>(id);
  }

  void getDependentDialects(mlir::DialectRegistry &registry) const override {}

  llvm::StringRef getArgument() const final {
    return "rewrite-cluster-to-ifrt-call";
  }

  llvm::StringRef getDescription() const final {
    return "Convert tf_device.cluster_func to tf.ifrt_proram_call";
  }

  void runOnOperation() override {
    mlir::ModuleOp module = getOperation();
    mlir::SymbolTable symbol_table(module);

    // key: original callee function in tf_device.cluster_func. value: ifrt
    // program.
    llvm::DenseMap<mlir::func::FuncOp, mlir::func::FuncOp>
        cluster_to_ifrt_program;

    std::vector<mlir::tf_device::ClusterFuncOp> cluster_func_ops;
    module.walk([&](mlir::tf_device::ClusterFuncOp cluster_func) {
      cluster_func_ops.push_back(cluster_func);
    });
    for (auto cluster_func : cluster_func_ops) {
      Rewrite(symbol_table, cluster_to_ifrt_program, cluster_func);
    }

    // TODO(b/304839793): Move this to a separate pass. The old remove
    // compilation result pass rely on TPUPartitionedCall
    llvm::SmallVector<mlir::TF::TPUCompilationResultOp> compilation_result_ops;
    module.walk([&](mlir::TF::TPUCompilationResultOp op) {
      compilation_result_ops.push_back(op);
    });
    for (auto op : compilation_result_ops) {
      if (!op.use_empty()) {
        module->emitError("TPUCompilationResultOp is under use");
        return signalPassFailure();
      }
      op.erase();
    }
  }

  void Rewrite(mlir::SymbolTable &symbol_table,
               llvm::DenseMap<mlir::func::FuncOp, mlir::func::FuncOp>
                   &cluster_to_ifrt_program,
               mlir::tf_device::ClusterFuncOp cluster_func) {
    mlir::OpBuilder builder(cluster_func);
    mlir::FlatSymbolRefAttr callee_symbol = cluster_func.getFuncAttr();
    mlir::func::FuncOp callee_func =
        symbol_table.lookup<mlir::func::FuncOp>(callee_symbol.getValue());

    auto ifrt_program_name =
        absl::StrCat("_ifrt_program_", callee_func.getSymName().str());
    if (mlir::func::FuncOp ifrt_program =
            cluster_to_ifrt_program[callee_func]) {
      // ifrt program already exists
      builder.setInsertionPoint(cluster_func);

      mlir::TF::IfrtCallOp ifrt_call_op = builder.create<mlir::TF::IfrtCallOp>(
          cluster_func->getLoc(), cluster_func.getResultTypes(),
          cluster_func->getOperands());

      int64_t program_id;
      if (auto attr = ifrt_program->getAttrOfType<mlir::IntegerAttr>(
              "tfrt_ifrt_serving.program_id")) {
        program_id = attr.getInt();
      } else {
        return signalPassFailure();
      }

      // TODO(b/304839793): populate variable names after adding a variable
      // hoisting pass.
      ifrt_call_op.setVariableNamesAttr(builder.getArrayAttr({}));
      ifrt_call_op.setProgramId(program_id);

      cluster_func->replaceAllUsesWith(ifrt_call_op.getResults());
      cluster_func->erase();

      return;
    }

    mlir::OpBuilder::InsertionGuard insertion_guard(builder);
    builder.setInsertionPoint(callee_func);

    mlir::func::FuncOp cloned_ifrt_program = builder.create<mlir::func::FuncOp>(
        callee_func->getLoc(), ifrt_program_name,
        callee_func.getFunctionType());
    mlir::IRMapping mapper;
    callee_func.cloneInto(cloned_ifrt_program, mapper);

    cloned_ifrt_program.setName(ifrt_program_name);

    int64_t program_id = NewProgramId();
    cloned_ifrt_program->setAttr("tfrt_ifrt_serving.program_id",
                                 builder.getI64IntegerAttr(program_id));

    builder.setInsertionPoint(cluster_func);

    mlir::TF::IfrtCallOp ifrt_call_op = builder.create<mlir::TF::IfrtCallOp>(
        cluster_func->getLoc(), cluster_func.getResultTypes(),
        cluster_func->getOperands());

    // TODO(b/304839793): populate variable names after adding a variable
    // hoisting pass.
    ifrt_call_op.setVariableNamesAttr(builder.getArrayAttr({}));
    ifrt_call_op.setProgramId(program_id);

    cluster_func->replaceAllUsesWith(ifrt_call_op.getResults());
    cluster_func->erase();

    symbol_table.insert(cloned_ifrt_program);
    cluster_to_ifrt_program[callee_func] = cloned_ifrt_program;
  }
};
}  // namespace

std::unique_ptr<mlir::OperationPass<mlir::ModuleOp>>
CreateRewriteClusterToIfrtCallPass() {
  return std::make_unique<RewriteClusterToIfrtCallPass>();
}

}  // namespace ifrt_serving
}  // namespace tensorflow
