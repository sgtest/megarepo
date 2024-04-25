/* Copyright 2022 The OpenXLA Authors.

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

#include <iterator>
#include <memory>
#include <optional>
#include <utility>
#include <vector>

#include "llvm/ADT/SmallVector.h"
#include "llvm/ADT/StringRef.h"
#include "mlir/Dialect/Func/IR/FuncOps.h"  // from @llvm-project
#include "mlir/Dialect/MemRef/IR/MemRef.h"  // from @llvm-project
#include "mlir/IR/Attributes.h"  // from @llvm-project
#include "mlir/IR/BuiltinTypes.h"  // from @llvm-project
#include "mlir/IR/ImplicitLocOpBuilder.h"  // from @llvm-project
#include "mlir/IR/MLIRContext.h"  // from @llvm-project
#include "mlir/IR/PatternMatch.h"  // from @llvm-project
#include "mlir/IR/SymbolTable.h"  // from @llvm-project
#include "mlir/Pass/Pass.h"  // from @llvm-project
#include "mlir/Support/LLVM.h"  // from @llvm-project
#include "mlir/Transforms/GreedyPatternRewriteDriver.h"  // from @llvm-project
#include "xla/mlir/backends/cpu/transforms/passes.h"
#include "xla/mlir/runtime/transforms/type_converter.h"
#include "xla/mlir/runtime/utils/custom_calls.h"
#include "xla/mlir/xla_cpu/ir/xla_cpu.h"
#include "xla/mlir_hlo/mhlo/IR/hlo_ops.h"
#include "xla/service/hlo_parser.h"

namespace xla {
namespace cpu {
namespace {

#define GEN_PASS_DEF_CONVERTXLACPUTOCPURUNTIMEPASS
#include "xla/mlir/backends/cpu/transforms/passes.h.inc"

using namespace mlir;  // NOLINT

using xla_cpu::PartitionIdOp;
using xla_cpu::ReplicaIdOp;

using xla::runtime::AppendCustomCallAttrs;
using xla::runtime::CustomCallDeclarations;

class ConvertXlaCpuToCpuRuntimePass
    : public impl::ConvertXlaCpuToCpuRuntimePassBase<
          ConvertXlaCpuToCpuRuntimePass> {
  void runOnOperation() override;

  void getDependentDialects(DialectRegistry& registry) const override {
    registry.insert<func::FuncDialect, memref::MemRefDialect>();
  }
};

// Copies memrefs with non-identity layouts (e.g. results of memref.subviews)
// to newly allocated memrefs, ensuring all outputs have flat layouts.
// TODO(jreiffers): If the memref just as an offset, but its layout is otherwise
// default, the copy is overkill.
SmallVector<Value> EnsureFlatMemrefs(ValueRange values,
                                     ImplicitLocOpBuilder& b) {
  SmallVector<Value> out;
  for (Value value : values) {
    auto ty = mlir::dyn_cast<MemRefType>(value.getType());
    if (!ty || ty.getLayout().isIdentity()) {
      out.push_back(value);
    } else {
      auto default_layout_ty =
          MemRefType::get(ty.getShape(), ty.getElementType());
      auto alloc =
          out.emplace_back(b.create<memref::AllocOp>(default_layout_ty));
      b.create<memref::CopyOp>(value, alloc);
    }
  }
  return out;
}

// Replaces a DPS style collective op with a custom call.
func::CallOp CreateCallForDpsCollectiveOp(Operation* op,
                                          CustomCallDeclarations& custom_calls,
                                          StringRef call_target,
                                          PatternRewriter& rewriter) {
  ImplicitLocOpBuilder b(op->getLoc(), rewriter);
  b.setInsertionPoint(op);

  // Subview ops result in strided Memrefs. The runtime can't deal with them,
  // so we copy everything that doesn't have the default layout.
  SmallVector<Value> new_operands = EnsureFlatMemrefs(op->getOperands(), b);

  func::FuncOp callee = custom_calls.GetOrCreate(
      b, call_target, TypeRange(ValueRange(new_operands)), TypeRange());
  auto call =
      b.create<func::CallOp>(callee.getName(), TypeRange(), new_operands);

  // Copy attributes from original op.
  for (auto& attr : op->getAttrs()) {
    call->setAttr(attr.getName(), attr.getValue());
  }
  rewriter.eraseOp(op);
  return call;
}

//===----------------------------------------------------------------------===//

template <typename IdOp>
class IdOpLowering : public OpRewritePattern<IdOp> {
 public:
  IdOpLowering(MLIRContext* ctx, llvm::StringRef call_target,
               CustomCallDeclarations& custom_calls)
      : OpRewritePattern<IdOp>(ctx),
        call_target_(call_target),
        custom_calls_(custom_calls) {}

  LogicalResult matchAndRewrite(IdOp op,
                                PatternRewriter& rewriter) const override {
    ImplicitLocOpBuilder b(op->getLoc(), rewriter);

    // Create a custom call function declaration.
    func::FuncOp callee = custom_calls_.GetOrCreate(
        b, call_target_, TypeRange(), TypeRange(rewriter.getI32Type()));

    rewriter.replaceOpWithNewOp<func::CallOp>(op, callee.getName(),
                                              TypeRange(rewriter.getI32Type()));
    return success();
  }

 private:
  llvm::StringRef call_target_;
  CustomCallDeclarations& custom_calls_;
};

//===----------------------------------------------------------------------===//

class AllReduceLowering : public OpRewritePattern<xla_cpu::AllReduceOp> {
 public:
  AllReduceLowering(MLIRContext* ctx, CustomCallDeclarations& custom_calls)
      : OpRewritePattern(ctx), custom_calls_(custom_calls) {}

  LogicalResult matchAndRewrite(xla_cpu::AllReduceOp op,
                                PatternRewriter& rewriter) const override {
    if (!mlir::isa<MemRefType>(op.getOperandTypes().front())) {
      return failure();
    }

    auto call = CreateCallForDpsCollectiveOp(op.getOperation(), custom_calls_,
                                             kCallTarget, rewriter);

    // Set default attributes.
    if (!call->hasAttr("use_global_device_ids")) {
      call->setAttr("use_global_device_ids", rewriter.getI32IntegerAttr(0));
    }
    if (!call->hasAttr("op_id")) {
      call->setAttr("op_id", rewriter.getI64IntegerAttr(0));
    }

    return success();
  }

 private:
  static constexpr const char kCallTarget[] = "xla.cpu.all_reduce";

  CustomCallDeclarations& custom_calls_;
};

//===----------------------------------------------------------------------===//

class AllToAllLowering : public OpRewritePattern<xla_cpu::AllToAllOp> {
 public:
  AllToAllLowering(MLIRContext* ctx, CustomCallDeclarations& custom_calls)
      : OpRewritePattern(ctx), custom_calls_(custom_calls) {}

  LogicalResult matchAndRewrite(xla_cpu::AllToAllOp op,
                                PatternRewriter& rewriter) const override {
    if (op.getSplitDimensionAttr()) {
      op.emitOpError("ArrayAllToAll is not supported");
      return failure();
    }
    CreateCallForDpsCollectiveOp(op.getOperation(), custom_calls_, kCallTarget,
                                 rewriter);
    return success();
  }

 private:
  static constexpr const char kCallTarget[] = "xla.cpu.tuple_all_to_all";

  CustomCallDeclarations& custom_calls_;
};

//===----------------------------------------------------------------------===//

class CollectivePermuteLowering
    : public OpRewritePattern<xla_cpu::CollectivePermuteOp> {
 public:
  CollectivePermuteLowering(MLIRContext* ctx,
                            CustomCallDeclarations& custom_calls)
      : OpRewritePattern(ctx), custom_calls_(custom_calls) {}

  LogicalResult matchAndRewrite(xla_cpu::CollectivePermuteOp op,
                                PatternRewriter& rewriter) const override {
    if (!mlir::isa<MemRefType>(op.getOperandTypes().front())) {
      return failure();
    }

    CreateCallForDpsCollectiveOp(op.getOperation(), custom_calls_, kCallTarget,
                                 rewriter);
    return success();
  }

 private:
  static constexpr const char kCallTarget[] = "xla.cpu.collective_permute";

  CustomCallDeclarations& custom_calls_;
};

//===----------------------------------------------------------------------===//

class ConvolutionLowering : public OpRewritePattern<xla_cpu::ConvolutionOp> {
 public:
  ConvolutionLowering(MLIRContext* ctx, CustomCallDeclarations& custom_calls)
      : OpRewritePattern(ctx), custom_calls_(custom_calls) {}

  LogicalResult matchAndRewrite(xla_cpu::ConvolutionOp op,
                                PatternRewriter& rewriter) const override {
    ImplicitLocOpBuilder b(op->getLoc(), rewriter);
    b.setInsertionPoint(op);

    // Subview ops result in strided Memrefs. The runtime can't deal with them,
    // so we copy everything that doesn't have the default layout.
    SmallVector<Value> new_operands = EnsureFlatMemrefs(op->getOperands(), b);

    func::FuncOp callee = custom_calls_.GetOrCreate(
        b, kCallTarget, TypeRange(ValueRange(new_operands)), TypeRange());
    auto call =
        b.create<func::CallOp>(callee.getName(), TypeRange(), new_operands);

    // Copy attributes from original op.
    for (auto name :
         {"inputBatchDimension", "inputSpatialDimensions",
          "inputFeatureDimension", "kernelSpatialDimensions",
          "kernelInputFeatureDimension", "kernelOutputFeatureDimension",
          "outputSpatialDimensions", "window_strides", "padding",
          "lhs_dilation", "rhs_dilation", "feature_group_count"}) {
      call->setAttr(name, op->getAttr(name));
    }
    rewriter.eraseOp(op);
    return success();
  }

 private:
  static constexpr const char kCallTarget[] = "xla_cpu_convolution";

  CustomCallDeclarations& custom_calls_;
};

//===----------------------------------------------------------------------===//

class RngBitGeneratorLowering
    : public OpRewritePattern<xla_cpu::RngBitGeneratorOp> {
 public:
  RngBitGeneratorLowering(MLIRContext* ctx,
                          CustomCallDeclarations& custom_calls)
      : OpRewritePattern(ctx), custom_calls_(custom_calls) {}

  LogicalResult matchAndRewrite(xla_cpu::RngBitGeneratorOp op,
                                PatternRewriter& rewriter) const override {
    auto algorithm =
        mlir::cast<mhlo::RngAlgorithmAttr>(op.getRngAlgorithmAttr()).getValue();
    op->removeAttr("rng_algorithm");

    CreateCallForDpsCollectiveOp(op.getOperation(), custom_calls_,
                                 algorithm == mhlo::RngAlgorithm::THREE_FRY
                                     ? kThreeFryTarget
                                     : kPhiloxTarget,
                                 rewriter);
    return success();
  }

 private:
  static constexpr const char kThreeFryTarget[] = "xla_cpu_rng_three_fry";
  static constexpr const char kPhiloxTarget[] = "xla_cpu_rng_philox";

  CustomCallDeclarations& custom_calls_;
};

//===----------------------------------------------------------------------===//

class InfeedLowering : public OpRewritePattern<xla_cpu::InfeedOp> {
 public:
  InfeedLowering(MLIRContext* ctx, CustomCallDeclarations& custom_calls)
      : OpRewritePattern(ctx), custom_calls_(custom_calls) {}

  LogicalResult matchAndRewrite(xla_cpu::InfeedOp op,
                                PatternRewriter& rewriter) const override {
    ImplicitLocOpBuilder b(op->getLoc(), rewriter);

    // By default all operands are passed to the custom call handler.
    llvm::SmallVector<Value> operands = EnsureFlatMemrefs(op->getOperands(), b);

    // For infeed with empty tuples, bufferizer does not run, thus the token is
    // left as the only operand. Remove it.
    if (mlir::isa<mlir::mhlo::TokenType>(operands.back().getType())) {
      assert(operands.size() == 1 && "Expect token only with empty tuples");
      operands.pop_back();
    }

    // Create a custom call function declaration.
    func::FuncOp callee =
        custom_calls_.GetOrCreate(b, StringRef(kCallTarget),
                                  TypeRange(ValueRange(operands)), TypeRange());

    // Call the runtime intrinsic with the original operands.
    b.create<func::CallOp>(op->getLoc(), callee.getName(), TypeRange(),
                           operands);
    rewriter.eraseOp(op);

    return success();
  }

 private:
  static constexpr const char kCallTarget[] = "xla.cpu.infeed";

  CustomCallDeclarations& custom_calls_;
};

//===----------------------------------------------------------------------===//

class OutfeedLowering : public OpRewritePattern<xla_cpu::OutfeedOp> {
 public:
  OutfeedLowering(MLIRContext* ctx, CustomCallDeclarations& custom_calls)
      : OpRewritePattern(ctx), custom_calls_(custom_calls) {}

  LogicalResult matchAndRewrite(xla_cpu::OutfeedOp op,
                                PatternRewriter& rewriter) const override {
    ImplicitLocOpBuilder b(op->getLoc(), rewriter);

    // By default all operands are passed to the custom call handler.
    llvm::SmallVector<Value> operands = EnsureFlatMemrefs(op->getOperands(), b);

    // Create a custom call function declaration.
    func::FuncOp callee =
        custom_calls_.GetOrCreate(b, StringRef(kCallTarget),
                                  TypeRange(ValueRange(operands)), TypeRange());

    llvm::SmallVector<NamedAttribute> custom_call_attrs;
    SmallVector<int32_t> types;
    for (int i = 0; i < op.getResultType().size(); ++i) {
      auto type_attr = cast<TypeAttr>(op.getResultType()[i]);
      auto status_or_primitive_type =
          xla::runtime::TypeConverter::ConvertElementType(type_attr.getValue());
      if (!status_or_primitive_type.ok()) {
        return rewriter.notifyMatchFailure(
            op,
            "is not provided with a supported primitive type in the result "
            "type attribute.");
      }
      types.push_back(status_or_primitive_type.value());
    }

    // Call the runtime intrinsic with the original operands.
    auto call = rewriter.replaceOpWithNewOp<func::CallOp>(
        op, callee.getName(), TypeRange(), operands);
    call->setAttr("result_type", b.getI32ArrayAttr(types));

    return success();
  }

 private:
  static constexpr const char kCallTarget[] = "xla.cpu.outfeed";

  CustomCallDeclarations& custom_calls_;
};

//===----------------------------------------------------------------------===//

class FftLowering : public OpRewritePattern<xla_cpu::FftOp> {
 public:
  FftLowering(MLIRContext* ctx, CustomCallDeclarations& custom_calls)
      : OpRewritePattern(ctx), custom_calls_(custom_calls) {}

  LogicalResult matchAndRewrite(xla_cpu::FftOp op,
                                PatternRewriter& rewriter) const override {
    CreateCallForDpsCollectiveOp(op.getOperation(), custom_calls_, kCallTarget,
                                 rewriter);
    return success();
  }

 private:
  static constexpr const char kCallTarget[] = "xla.cpu.fft";

  CustomCallDeclarations& custom_calls_;
};

//===----------------------------------------------------------------------===//

void ConvertXlaCpuToCpuRuntimePass::runOnOperation() {
  ModuleOp module = getOperation();
  MLIRContext* ctx = module.getContext();

  // Keep track of the custom calls created from the lowered operations.
  SymbolTable sym_table(module);
  CustomCallDeclarations custom_calls(std::move(sym_table));

  // Convert xla_cpu operations to XLA cpu runtime custom calls.
  RewritePatternSet patterns(ctx);
  patterns.insert<AllReduceLowering, AllToAllLowering,
                  CollectivePermuteLowering, ConvolutionLowering, FftLowering,
                  InfeedLowering, OutfeedLowering, RngBitGeneratorLowering>(
      ctx, custom_calls);
  patterns.insert<IdOpLowering<PartitionIdOp>>(ctx, "xla.cpu.partition_id",
                                               custom_calls);
  patterns.insert<IdOpLowering<ReplicaIdOp>>(ctx, "xla.cpu.replica_id",
                                             custom_calls);

  if (failed(applyPatternsAndFoldGreedily(module, std::move(patterns))))
    return signalPassFailure();
}

}  // namespace

std::unique_ptr<mlir::OperationPass<mlir::ModuleOp>>
createConvertXlaCpuToCpuRuntimePass() {
  return std::make_unique<ConvertXlaCpuToCpuRuntimePass>();
}

}  // namespace cpu
}  // namespace xla
