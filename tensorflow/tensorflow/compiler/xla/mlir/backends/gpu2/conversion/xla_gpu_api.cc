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

#include "tensorflow/compiler/xla/mlir/backends/gpu2/conversion/xla_gpu_api.h"

#include <string_view>

#include "third_party/iree/llvm-external-projects/iree-dialects/include/iree-dialects/Dialect/Input/InputDialect.h"
#include "third_party/iree/llvm-external-projects/iree-dialects/include/iree-dialects/Dialect/Input/InputOps.h"
#include "mlir/Dialect/Arith/IR/Arith.h"  // from @llvm-project
#include "mlir/Dialect/Func/IR/FuncOps.h"  // from @llvm-project
#include "mlir/IR/Builders.h"  // from @llvm-project
#include "mlir/IR/BuiltinOps.h"  // from @llvm-project
#include "mlir/IR/BuiltinTypes.h"  // from @llvm-project
#include "mlir/IR/SymbolTable.h"  // from @llvm-project
#include "tensorflow/compiler/xla/mlir/backends/gpu2/ir/xla_gpu_dialect.h"

namespace xla::gpu {

using namespace mlir;                 // NOLINT
using namespace mlir::iree_compiler;  // NOLINT

using arith::ConstantIndexOp;
using arith::ConstantIntOp;

SymbolTable &XlaGpuApi::symTable(ModuleOp module) {
  return sym_table_.getSymbolTable(module);
}

func::FuncOp XlaGpuApi::addDecl(OpBuilder &b, ModuleOp module,
                                std::string_view name,
                                FunctionType function_type) {
  if (auto fn = sym_table_.lookupNearestSymbolFrom<func::FuncOp>(
          module, b.getStringAttr(name)))
    return fn;

  Location loc = UnknownLoc::get(module->getContext());

  OpBuilder::InsertionGuard guard(b);
  b.setInsertionPointToEnd(module.getBody());

  auto fn = b.create<func::FuncOp>(loc, name, function_type);
  fn.setPrivate();
  symTable(module).insert(fn);
  return fn;
}

//===----------------------------------------------------------------------===//
// Helper functions to build XLA:GPU API arguments.
//===----------------------------------------------------------------------===//

/*static*/ Type XlaGpuApi::getI32ListType(OpBuilder &b) {
  return b.getType<IREE::Input::ListType>(b.getI32Type());
}

/*static*/ Type XlaGpuApi::getBufferViewListType(OpBuilder &b) {
  return b.getType<IREE::Input::ListType>(
      b.getType<IREE::Input::BufferViewType>());
}

/*static*/ TypedValue<IREE::Input::ListType> XlaGpuApi::getI32List(
    ImplicitLocOpBuilder &b, ArrayRef<int64_t> values) {
  Value size = b.create<ConstantIndexOp>(values.size());
  Value list = b.create<IREE::Input::ListCreateOp>(getI32ListType(b), size);

  if (!values.empty()) b.create<IREE::Input::ListResizeOp>(list, size);
  for (auto indexed : llvm::enumerate(values)) {
    Value index = b.create<ConstantIndexOp>(indexed.index());
    Value value = b.create<ConstantIntOp>(indexed.value(), 32);
    b.create<IREE::Input::ListSetOp>(list, index, value);
  }

  return list.cast<TypedValue<IREE::Input::ListType>>();
}

/*static*/ TypedValue<IREE::Input::BufferViewType> XlaGpuApi::getBufferView(
    ImplicitLocOpBuilder &b, TypedValue<TensorType> tensor) {
  // Skip exporting tensor that was just imported from a buffer view.
  if (auto tensor_import = dyn_cast_or_null<IREE::Input::TensorImportOp>(
          tensor.getDefiningOp())) {
    return cast<TypedValue<IREE::Input::BufferViewType>>(
        tensor_import.getSource());
  }

  Value view = b.create<IREE::Input::TensorExportOp>(
      b.getType<IREE::Input::BufferViewType>(), tensor,
      /*source_dims=*/ValueRange());
  return cast<TypedValue<IREE::Input::BufferViewType>>(view);
}

/*static*/ TypedValue<IREE::Input::ListType> XlaGpuApi::getBufferViewList(
    ImplicitLocOpBuilder &b, ArrayRef<TypedValue<TensorType>> tensors) {
  Type type = XlaGpuApi::getBufferViewListType(b);
  Value size = b.create<ConstantIndexOp>(tensors.size());
  Value list = b.create<IREE::Input::ListCreateOp>(type, size);

  if (!tensors.empty()) b.create<IREE::Input::ListResizeOp>(list, size);
  for (auto indexed : llvm::enumerate(tensors)) {
    Value index = b.create<ConstantIndexOp>(indexed.index());
    Value view = getBufferView(b, indexed.value());
    b.create<IREE::Input::ListSetOp>(list, index, view);
  }

  return list.cast<TypedValue<IREE::Input::ListType>>();
}

//===----------------------------------------------------------------------===//
// XLA:GPU kernel APIs
//===----------------------------------------------------------------------===//

func::FuncOp XlaGpuApi::getCreateKernel(OpBuilder &b, ModuleOp module) {
  SmallVector<Type> args = {
      b.getType<IREE::Input::ByteBufferType>(),  // kernel_name
      b.getI32Type(),                            // shared_memory_bytes
  };
  SmallVector<Type> rets = {b.getType<KernelType>()};
  return addDecl(b, module, "xla_gpu.kernel.create",
                 FunctionType::get(b.getContext(), args, rets));
}

func::FuncOp XlaGpuApi::getDispatchKernel(OpBuilder &b, ModuleOp module) {
  SmallVector<Type> args = {b.getType<ExecutionContextType>(),
                            b.getType<KernelType>(), getBufferViewListType(b)};
  args.append(6, b.getI32Type());  // workgroup_size / workload_size
  return addDecl(b, module, "xla_gpu.kernel.dispatch",
                 FunctionType::get(b.getContext(), args, /*rets=*/{}));
}

//===----------------------------------------------------------------------===//
// XLA:GPU gemm (dot) APIs
//===----------------------------------------------------------------------===//

func::FuncOp XlaGpuApi::getCreateDotDimensionsNumbers(OpBuilder &b,
                                                      ModuleOp module) {
  auto i32_list = getI32ListType(b);
  SmallVector<Type> args = {/*lhs_batch_dimensions=*/i32_list,
                            /*rhs_batch_dimensions=*/i32_list,
                            /*lhs_contracting_dimensions=*/i32_list,
                            /*rhs_contracting_dimensions=*/i32_list};
  SmallVector<Type> rets = {b.getType<DotDimensionNumbersType>()};
  return addDecl(b, module, "xla_gpu.dot_dimension_numbers.create",
                 FunctionType::get(b.getContext(), args, rets));
}

func::FuncOp XlaGpuApi::getCreateDotPrecision(OpBuilder &b, ModuleOp module) {
  SmallVector<Type> args = {getI32ListType(b)};
  SmallVector<Type> rets = {b.getType<DotPrecisionType>()};
  return addDecl(b, module, "xla_gpu.dot_precision.create",
                 FunctionType::get(b.getContext(), args, rets));
}

func::FuncOp XlaGpuApi::getCreateDotConfig(OpBuilder &b, ModuleOp module) {
  SmallVector<Type> args = {b.getI32Type(),  // algorithm
                            b.getF64Type(),  // alpha_real
                            b.getF64Type(),  // alpha_imag
                            b.getF64Type(),  // beta
                            b.getType<DotDimensionNumbersType>(),
                            b.getType<DotPrecisionType>()};
  SmallVector<Type> rets = {b.getType<DotConfigType>()};
  return addDecl(b, module, "xla_gpu.dot_config.create",
                 FunctionType::get(b.getContext(), args, rets));
}

func::FuncOp XlaGpuApi::getDispatchGemm(OpBuilder &b, ModuleOp module) {
  auto execution_context = b.getType<ExecutionContextType>();
  auto buffer_view = b.getType<IREE::Input::BufferViewType>();
  SmallVector<Type> args = {execution_context,
                            buffer_view,  // lhs
                            buffer_view,  // rhs
                            buffer_view,  // out
                            b.getType<DotConfigType>(),
                            b.getType<TraceType>()};
  return addDecl(b, module, "xla_gpu.gemm.dispatch",
                 FunctionType::get(b.getContext(), args, /*rets=*/TypeRange()));
}

//===--------------------------------------------------------------------===//
// XLA:GPU memcpy APIs
//===--------------------------------------------------------------------===//

mlir::func::FuncOp XlaGpuApi::getD2DMemcpy(mlir::OpBuilder &b,
                                           mlir::ModuleOp module) {
  auto execution_context = b.getType<ExecutionContextType>();
  auto buffer_view = b.getType<IREE::Input::BufferViewType>();
  SmallVector<Type> args = {execution_context, buffer_view, buffer_view};
  return addDecl(b, module, "xla_gpu.memcpy.d2d",
                 FunctionType::get(b.getContext(), args, /*rets=*/TypeRange()));
}

mlir::func::FuncOp XlaGpuApi::getLoadI1Memcpy(mlir::OpBuilder &b,
                                              mlir::ModuleOp module) {
  SmallVector<Type> args = {b.getType<ExecutionContextType>(),
                            b.getType<IREE::Input::BufferViewType>(),
                            b.getI32Type()};
  SmallVector<Type> rets = {b.getIntegerType(1)};
  return addDecl(b, module, "xla_gpu.memcpy.load.i1",
                 FunctionType::get(b.getContext(), args, rets));
}

//===----------------------------------------------------------------------===//
// XLA:GPU tracing APIs
//===----------------------------------------------------------------------===//

func::FuncOp XlaGpuApi::getCreateTrace(OpBuilder &b, ModuleOp module) {
  SmallVector<Type> args = {b.getType<IREE::Input::ByteBufferType>()};
  SmallVector<Type> rets = {b.getType<TraceType>()};
  return addDecl(b, module, "xla_gpu.trace.create",
                 FunctionType::get(b.getContext(), args, rets));
}

}  // namespace xla::gpu
