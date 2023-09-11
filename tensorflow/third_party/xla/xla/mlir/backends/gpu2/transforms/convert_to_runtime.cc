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

#include <memory>
#include <optional>
#include <string>
#include <string_view>
#include <utility>

#include "iree-dialects/Dialect/Input/InputDialect.h"
#include "iree-dialects/Dialect/Input/InputOps.h"
#include "mlir/Dialect/Arith/IR/Arith.h"  // from @llvm-project
#include "mlir/Dialect/Func/IR/FuncOps.h"  // from @llvm-project
#include "mlir/Dialect/MemRef/IR/MemRef.h"  // from @llvm-project
#include "mlir/Dialect/SCF/IR/SCF.h"  // from @llvm-project
#include "mlir/Dialect/Tensor/IR/Tensor.h"  // from @llvm-project
#include "mlir/IR/Builders.h"  // from @llvm-project
#include "mlir/IR/BuiltinOps.h"  // from @llvm-project
#include "mlir/IR/BuiltinTypes.h"  // from @llvm-project
#include "mlir/IR/ImplicitLocOpBuilder.h"  // from @llvm-project
#include "mlir/IR/MLIRContext.h"  // from @llvm-project
#include "mlir/IR/PatternMatch.h"  // from @llvm-project
#include "mlir/IR/Value.h"  // from @llvm-project
#include "mlir/Pass/Pass.h"  // from @llvm-project
#include "mlir/Support/LogicalResult.h"  // from @llvm-project
#include "mlir/Transforms/DialectConversion.h"  // from @llvm-project
#include "xla/mlir/backends/gpu2/conversion/convert_case_op.h"
#include "xla/mlir/backends/gpu2/conversion/convert_compiled_ops.h"
#include "xla/mlir/backends/gpu2/conversion/convert_library_ops.h"
#include "xla/mlir/backends/gpu2/conversion/convert_memref_ops.h"
#include "xla/mlir/backends/gpu2/conversion/convert_while_op.h"
#include "xla/mlir/backends/gpu2/conversion/de_bufferization.h"
#include "xla/mlir/backends/gpu2/conversion/xla_gpu_api.h"
#include "xla/mlir/backends/gpu2/ir/xla_gpu_dialect.h"
#include "xla/mlir/backends/gpu2/ir/xla_gpu_ops.h"
#include "xla/mlir/backends/gpu2/transforms/passes.h"
#include "xla/mlir_hlo/lhlo/IR/lhlo_ops.h"

#define GEN_PASS_DECL_CONVERTTOXLAGPURUNTIME
#include "xla/mlir/backends/gpu2/transforms/passes.h.inc"

#define GEN_PASS_DEF_CONVERTTOXLAGPURUNTIME
#include "xla/mlir/backends/gpu2/transforms/passes.h.inc"

namespace xla::gpu {

class ThunkSequence;  // forward declare

using namespace mlir;                 // NOLINT
using namespace mlir::iree_compiler;  // NOLINT

//===----------------------------------------------------------------------===//

// Creates an IREE Input ExecutableSource from the PTX source compiled by the
// XLA compilation pipeline (it has functions for all compiled XLA fusions).
IREE::Input::ExecutableSourceOp createXlaExecutableSource(ModuleOp module) {
  Location loc = module.getLoc();
  MLIRContext *ctx = module.getContext();

  ImplicitLocOpBuilder b(loc, OpBuilder::atBlockEnd(module.getBody()));

  // Create executable source with empty objects, we'll fill it with XLA device
  // kernels later when we'll be compiling MLIR input to IREE VM flatbuffer.
  auto objects = IREE::Input::ExecutableObjectsAttr::get(
      ctx, b.getArrayAttr({}), b.getArrayAttr({}));
  auto executable_source = b.create<IREE::Input::ExecutableSourceOp>(
      b.getStringAttr("private"), b.getStringAttr("xla.module.ptx"), objects);

  b.setInsertionPointToEnd(&executable_source.getBody().emplaceBlock());
  b.create<IREE::Input::ExecutableSourceEndOp>();

  return executable_source;
}

//===----------------------------------------------------------------------===//

// Adds `xla_gpu.execution_context` argument to all functions in the module.
static void addExecutionContextArgument(ModuleOp module) {
  MLIRContext *ctx = module.getContext();

  Type arg = ExecutionContextType::get(ctx);
  DictionaryAttr attrs = DictionaryAttr::get(ctx);

  for (func::FuncOp func : module.getOps<func::FuncOp>()) {
    func.insertArguments({0}, {arg}, {attrs}, {func.getLoc()});
  }
}

class ConvertToXlaGpuRuntimePass
    : public ::impl::ConvertToXlaGpuRuntimeBase<ConvertToXlaGpuRuntimePass> {
 public:
  explicit ConvertToXlaGpuRuntimePass(ThunkSequence *thunk_sequence)
      : thunk_sequence_(thunk_sequence) {}

  void runOnOperation() override {
    auto *ctx = &getContext();

    // Add execution context argument to all functions in the module.
    addExecutionContextArgument(getOperation());

    TypeConverter converter;
    converter.addConversion([](Type type) { return type; });

    // Convert all memrefs back to tensors, as the IREE compilation pipeline
    // accepts input IR with value semantics. We rely on tied operands to pass
    // "output tensors" to be used as a storage for results.
    converter.addConversion([](MemRefType memref) {
      // Update scalars to vectors, so that we can insert cast to a dynamically
      // shaped tensor to prevent folding at Flow level. See use of optimization
      // barriers in the `convert_compiled_ops` conversion patterns.
      if (memref.getRank() == 0) {
        return RankedTensorType::get({1}, memref.getElementType());
      }

      return RankedTensorType::get(memref.getShape(), memref.getElementType());
    });

    // De-bufferization state shared between lowering patterns required for
    // threading tied operands starting from arguments to terminator.
    auto state = DeBufferization::Create(converter, getOperation());
    if (failed(state)) return signalPassFailure();

    // XLA:GPU API declarations for the custom module.
    XlaGpuApi api;

    auto executable_source = createXlaExecutableSource(getOperation());

    RewritePatternSet patterns(&getContext());
    populateAnyFunctionOpInterfaceTypeConversionPattern(patterns, converter);

    // Lower LMHLO control flow operations to structured control flow.
    populateWhileOpConversionPatterns(patterns, converter, *state);
    populateCaseOpConversionPatterns(patterns, converter, *state);

    // Lower LMHLO operation to corresponding XLA runtime operations and API
    // calls (e.g. cuBLAS lowered to XLA:GPU custom module calls).
    populateCompiledOpsConversionPatterns(
        patterns, converter, executable_source, thunk_sequence_, *state);
    populateLibraryOpsConversionPatterns(patterns, converter, *state, api);
    populateMemrefConversionPatterns(patterns, converter, *state);

    // Ensure all HLO and memref operations get lowered to IREEInput and XLA:GPU
    // runtime. For this we have to de-bufferize the IR and correctly tie
    // operands with results write into the destination buffers.
    ConversionTarget target(*ctx);
    target.addIllegalDialect<lmhlo::LmhloDialect, memref::MemRefDialect>();
    target.addLegalDialect<IREE::Input::IREEInputDialect, arith::ArithDialect,
                           func::FuncDialect, tensor::TensorDialect,
                           scf::SCFDialect>();

    target.addDynamicallyLegalOp<func::FuncOp>([&](func::FuncOp op) {
      return converter.isSignatureLegal(op.getFunctionType()) &&
             converter.isLegal(&op.getBody());
    });

    if (failed(applyPartialConversion(getOperation(), target,
                                      std::move(patterns)))) {
      getOperation().emitError()
          << "conversion from HLO to XLA:GPU runtime failed";
      return signalPassFailure();
    }
  }

 private:
  ThunkSequence *thunk_sequence_;
};

std::unique_ptr<OperationPass<ModuleOp>> createConvertToGpu2RuntimePass(
    ThunkSequence *thunk_sequence) {
  return std::make_unique<ConvertToXlaGpuRuntimePass>(thunk_sequence);
}

}  // namespace xla::gpu
