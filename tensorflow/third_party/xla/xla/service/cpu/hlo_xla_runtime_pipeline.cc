/* Copyright 2022 The TensorFlow Authors. All Rights Reserved.

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

#include "xla/service/cpu/hlo_xla_runtime_pipeline.h"

#include <utility>

#include "mlir/Dialect/Linalg/Transforms/TilingInterfaceImpl.h"
#include "mlir/Conversion/BufferizationToMemRef/BufferizationToMemRef.h"  // from @llvm-project
#include "mlir/Conversion/ComplexToStandard/ComplexToStandard.h"  // from @llvm-project
#include "mlir/Conversion/ReconcileUnrealizedCasts/ReconcileUnrealizedCasts.h"  // from @llvm-project
#include "mlir/Conversion/SCFToControlFlow/SCFToControlFlow.h"  // from @llvm-project
#include "mlir/Conversion/ShapeToStandard/ShapeToStandard.h"  // from @llvm-project
#include "mlir/Conversion/TensorToLinalg/TensorToLinalgPass.h"  // from @llvm-project
#include "mlir/Conversion/VectorToLLVM/ConvertVectorToLLVM.h"  // from @llvm-project
#include "mlir/Conversion/VectorToSCF/VectorToSCF.h"  // from @llvm-project
#include "mlir/Dialect/Arith/Transforms/BufferizableOpInterfaceImpl.h"  // from @llvm-project
#include "mlir/Dialect/Bufferization/Transforms/FuncBufferizableOpInterfaceImpl.h"  // from @llvm-project
#include "mlir/Dialect/Bufferization/Transforms/OneShotAnalysis.h"  // from @llvm-project
#include "mlir/Dialect/Bufferization/Transforms/Passes.h"  // from @llvm-project
#include "mlir/Dialect/Func/IR/FuncOps.h"  // from @llvm-project
#include "mlir/Dialect/Func/Transforms/Passes.h"  // from @llvm-project
#include "mlir/Dialect/Linalg/Passes.h"  // from @llvm-project
#include "mlir/Dialect/Linalg/Transforms/BufferizableOpInterfaceImpl.h"  // from @llvm-project
#include "mlir/Dialect/MemRef/Transforms/AllocationOpInterfaceImpl.h"  // from @llvm-project
#include "mlir/Dialect/MemRef/Transforms/Passes.h"  // from @llvm-project
#include "mlir/Dialect/SCF/Transforms/BufferizableOpInterfaceImpl.h"  // from @llvm-project
#include "mlir/Dialect/Shape/Transforms/BufferizableOpInterfaceImpl.h"  // from @llvm-project
#include "mlir/Dialect/Shape/Transforms/Passes.h"  // from @llvm-project
#include "mlir/Dialect/SparseTensor/Transforms/BufferizableOpInterfaceImpl.h"  // from @llvm-project
#include "mlir/Dialect/SparseTensor/Transforms/Passes.h"  // from @llvm-project
#include "mlir/Dialect/Tensor/Transforms/BufferizableOpInterfaceImpl.h"  // from @llvm-project
#include "mlir/Dialect/Vector/Transforms/BufferizableOpInterfaceImpl.h"  // from @llvm-project
#include "mlir/Pass/PassManager.h"  // from @llvm-project
#include "mlir/Transforms/Passes.h"  // from @llvm-project
#include "xla/mlir/backends/cpu/transforms/passes.h"
#include "xla/mlir/runtime/transforms/compiler.h"
#include "xla/mlir_hlo/mhlo/interfaces/bufferizable_op_interface_impl.h"
#include "xla/mlir_hlo/mhlo/transforms/passes.h"
#include "xla/mlir_hlo/transforms/passes.h"
#include "xla/status.h"
#include "tsl/platform/errors.h"
#include "tsl/platform/logging.h"

#ifdef EXPERIMENTAL_MLIR_GPU
#include "mlir/Conversion/GPUToNVVM/GPUToNVVMPass.h"  // from @llvm-project
#include "mlir/Dialect/GPU/IR/GPUDialect.h"  // from @llvm-project
#endif  // EXPERIMENTAL_MLIR_GPU

namespace xla {
namespace cpu {
namespace {

using mlir::func::FuncOp;

mlir::bufferization::OneShotBufferizationOptions GetBufferizationOptions(
    bool new_deallocator) {
  using mlir::bufferization::BufferizationOptions;
  using mlir::bufferization::LayoutMapOption;
  using mlir::bufferization::OneShotBufferizationOptions;

  OneShotBufferizationOptions options;
  options.bufferizeFunctionBoundaries = true;
  options.allowReturnAllocsFromLoops = true;
  options.setFunctionBoundaryTypeConversion(LayoutMapOption::IdentityLayoutMap);
  options.unknownTypeConverterFn = [](mlir::Value value,
                                      mlir::Attribute memorySpace,
                                      const BufferizationOptions& options) {
    return mlir::bufferization::getMemRefTypeWithStaticIdentityLayout(
        value.getType().cast<mlir::TensorType>(), memorySpace);
  };
  return options;
}

void AddSparsificationPasses(mlir::OpPassManager& pm, bool new_deallocator,
                             int32_t xla_cpu_sparse_cuda_threads) {
  // Sparse GPU acceleration for sparsified code.
  // Setting 0 threads means no acceleration (default).
  // Setting 1 thread means cuSPARSE libgen.
  // Otherwise direct CUDA codegen.
  const bool gpu_codegen = xla_cpu_sparse_cuda_threads > 0;
  const bool gpu_libgen = xla_cpu_sparse_cuda_threads == 1;
  mlir::SparsificationOptions sparsification_options;
  sparsification_options.enableRuntimeLibrary = false;
  if (gpu_codegen && !gpu_libgen) {
    sparsification_options.parallelizationStrategy =
        mlir::SparseParallelizationStrategy::kDenseOuterLoop;
  }
  // Sparsification set up.
  pm.addNestedPass<FuncOp>(mlir::createLinalgGeneralizationPass());
  pm.addPass(mlir::bufferization::createEmptyTensorEliminationPass());
  pm.addPass(mlir::createSparsificationAndBufferizationPass(
      GetBufferizationOptions(new_deallocator), sparsification_options,
      /*createSparseDeallocs=*/false,
      /*enableRuntimeLibrary=*/false,
      /*enableBufferInitialization=*/false,
      /*vectorLength=*/0,
      /*enableVLAVectorization=*/false,
      /*enableSIMDIndex32=*/false,
      /*enableGPULibgen=*/gpu_libgen));
  pm.addPass(mlir::createStorageSpecifierToLLVMPass());
  pm.addNestedPass<mlir::func::FuncOp>(mlir::createCanonicalizerPass());
  pm.addNestedPass<mlir::func::FuncOp>(
      mlir::bufferization::createFinalizingBufferizePass());
#ifdef EXPERIMENTAL_MLIR_GPU
  // Sparse GPU acceleration lowers to GPU dialect.
  if (gpu_codegen) {
    pm.addPass(
        mlir::createSparseGPUCodegenPass(xla_cpu_sparse_cuda_threads, false));
    pm.addNestedPass<mlir::gpu::GPUModuleOp>(mlir::createStripDebugInfoPass());
    pm.addNestedPass<mlir::gpu::GPUModuleOp>(mlir::createConvertSCFToCFPass());
    pm.addNestedPass<mlir::gpu::GPUModuleOp>(
        mlir::createConvertGpuOpsToNVVMOps());
  }
#else   // EXPERIMENTAL_MLIR_GPU
  CHECK(!gpu_codegen)
      << "Experimental MLIR GPU code generation was not enabled at build time";
#endif  // EXPERIMENTAL_MLIR_GPU
}

void AddSparsificationPassPipeline(mlir::OpPassManager& pm) {
  AddSparsificationPasses(pm, false, /*xla_cpu_sparse_cuda_threads=*/0);
}

}  // namespace

// -------------------------------------------------------------------------- //
// Assemble a HLO XLA Runtime pipeline to lower from HLO to Linalg on buffers.
// -------------------------------------------------------------------------- //

static Status CreateHloXlaPipeline(
    mlir::OpPassManager& pm, const HloXlaRuntimePipelineOptions& options) {
  // Resolve all shape constraints (e.g. broadcast constraints that can be
  // proved statically and changed to const witness) early to allow more
  // efficient broadcast operations moving.
  // Move up broadcasting operations to allow for more fusion opportunities.
  pm.addPass(mlir::createInlinerPass());
  pm.addPass(mlir::mhlo::createExpandHloTuplesPass("main"));
  // TODO(b/233771980): Remove once custom_call doesn't use tuples.
  pm.addNestedPass<mlir::func::FuncOp>(mlir::mhlo::createFlattenTuplePass());
  pm.addPass(createXlaAbiLegalizationPass());
  pm.addNestedPass<mlir::func::FuncOp>(
      mlir::mhlo::createLegalizeGeneralDotPass());
  pm.addNestedPass<mlir::func::FuncOp>(
      mlir::mhlo::createBroadcastPropagationPass());
  pm.addPass(mlir::createCSEPass());
  pm.addPass(mlir::createCanonicalizerPass());

  // Some early sparse rewriting rules.
  if (options.sparse_bufferization) {
    pm.addNestedPass<FuncOp>(createSparseCustomCallRewritingPass());
    // We wrap some CHLO unary operations with custom calls to preserve the
    // sparsity information for those operations during the roundtrip. We now
    // invoke the needed passes to lower such CHLO operations to HLO after we
    // rewrite the custom calls back to such CHLO unary operations.
    pm.addNestedPass<mlir::func::FuncOp>(
        mlir::mhlo::createLegalizeSparseOperationsPass(
            /*legalizeToCustomCalls=*/false));
    pm.addNestedPass<mlir::func::FuncOp>(
        mlir::mhlo::createChloLegalizeToHloPass());
    pm.addNestedPass<mlir::func::FuncOp>(
        mlir::mhlo::createSparseRewritingPass());
  }

  // Transform HLO operations to Linalg.
  pm.addNestedPass<mlir::func::FuncOp>(
      mlir::mhlo::createLegalizeControlFlowPass());
  pm.addNestedPass<FuncOp>(mlir::mhlo::createLegalizeDotGeneralToDotPass());
  pm.addPass(::mlir::mhlo::createLegalizeToArithmeticPass());
  pm.addNestedPass<mlir::func::FuncOp>(
      xla::cpu::createLegalizeLibraryOpsPass());
  pm.addNestedPass<mlir::func::FuncOp>(
      mlir::mhlo::createMhloExpandOpsSimplifierPass());
  pm.addNestedPass<mlir::func::FuncOp>(
      mlir::mhlo::createHloCanonicalizeScatterPass());
  pm.addNestedPass<FuncOp>(mlir::mhlo::createHloCanonicalizeDotPass());
  pm.addNestedPass<FuncOp>(mlir::mhlo::createGroupReductionDimensionsPass());
  pm.addNestedPass<mlir::func::FuncOp>(
      mlir::mhlo::createLegalizeHloToLinalgPass());

  // Lower index cast on tensors to tensor.generate.
  pm.addNestedPass<mlir::func::FuncOp>(mlir::createLowerIndexCastPass());

  pm.addPass(mlir::mhlo::createConvertToSignlessPass());

  // Lower shape dialect to standard to enable linalg canonicalizations (e.g.
  // use linalg inputs instead of outputs for memref.dim operations).
  pm.addNestedPass<mlir::func::FuncOp>(mlir::mhlo::createShapeSimplification());
  pm.addNestedPass<mlir::func::FuncOp>(mlir::createShapeToShapeLowering());
  pm.addPass(mlir::createConvertShapeToStandardPass());
  pm.addNestedPass<mlir::func::FuncOp>(
      mlir::createConvertShapeConstraintsPass());

  // Fuse Linalg on tensors operations.
  pm.addPass(mlir::createCSEPass());
  pm.addPass(mlir::memref::createResolveShapedTypeResultDimsPass());
  pm.addPass(mlir::createCanonicalizerPass());
  pm.addNestedPass<mlir::func::FuncOp>(
      mlir::createLinalgElementwiseOpFusionPass());
  pm.addPass(mlir::createReconcileUnrealizedCastsPass());
  pm.addPass(mlir::createConvertTensorToLinalgPass());

  // Detensorize SCF iter args.
  pm.addNestedPass<mlir::func::FuncOp>(mlir::createDetensorizeScfOpsPass());
  // mhlo ops on unit tensors generate trivial linalg.generics, which
  // one-shot-bufferize generates unnecessary allocs for. The detensorize pass
  // replaces these linalg.generics with scalar ops.
  auto detensorize = mlir::createLinalgDetensorizePass();
  if (detensorize->initializeOptions("aggressive-mode=true").failed()) {
    return tsl::errors::Internal("Failed to set up detensorize pass.");
  }
  pm.addNestedPass<mlir::func::FuncOp>(std::move(detensorize));
  pm.addPass(mlir::bufferization::createEmptyTensorEliminationPass());
  pm.addNestedPass<mlir::func::FuncOp>(
      mlir::bufferization::createEmptyTensorToAllocTensorPass());

  // Always run canonicalizer (which does dead code removal) before
  // bufferizing anything.
  pm.addPass(mlir::createCanonicalizerPass());

  if (options.sparse_bufferization) {
    // Convert Sparse tensors.
    AddSparsificationPasses(pm, false, options.xla_cpu_sparse_cuda_threads);
  } else {
    pm.addPass(mlir::hlo::createOneShotBufferizePass());
  }
  pm.addNestedPass<mlir::func::FuncOp>(createRewriteReallocToAllocPass());
  pm.addNestedPass<FuncOp>(mlir::createVectorizeCopyPass());
  pm.addNestedPass<FuncOp>(mlir::createNaiveCopyRemovalPass());

  // This should be unified. It exists, because the async runtime tests expect
  // parallel loops.
  if (options.sparse_bufferization) {
    pm.addNestedPass<mlir::func::FuncOp>(
        mlir::createConvertLinalgToLoopsPass());
  } else {
    pm.addNestedPass<mlir::func::FuncOp>(
        mlir::createConvertLinalgToParallelLoopsPass());
  }
  pm.addPass(mlir::createCSEPass());
  pm.addPass(mlir::createCanonicalizerPass());
  mlir::bufferization::BufferResultsToOutParamsOptions out_params_options;
  out_params_options.filterFn = [](mlir::func::FuncOp* func) {
    // Only transform the entry point.
    return func->getSymName() == "main";
  };
  pm.addPass(mlir::bufferization::createBufferResultsToOutParamsPass(
      out_params_options));

  pm.addNestedPass<FuncOp>(
      mlir::bufferization::createPromoteBuffersToStackPass(nullptr));
  pm.addNestedPass<mlir::func::FuncOp>(
      mlir::bufferization::createBufferDeallocationPass());
  pm.addPass(mlir::createBufferizationToMemRefPass());
  if (options.remove_copies_to_outparams) {
    pm.addNestedPass<mlir::func::FuncOp>(
        xla::cpu::createRemoveCopiesToOutParamsPass());
  }

  // Specialize linalg.matmul to linalg.dot, linalg.matvec or linalg.vecmat,
  // and immediately canonicalize to clean up not taken branches.
  // pm.addNestedPass<mlir::func::FuncOp>(CreateLinalgMatmulSpecializationPass());
  pm.addPass(mlir::createCanonicalizerPass());

  // TODO(tpopp): Move hits to mlir::hlo::createGenericHostToLLVMPass?
  pm.addNestedPass<mlir::func::FuncOp>(
      mlir::createConvertComplexToStandardPass());

  pm.addPass(mlir::createCSEPass());
  pm.addPass(mlir::createCanonicalizerPass());
  pm.addNestedPass<FuncOp>(mlir::createConvertVectorToSCFPass());
  pm.addNestedPass<FuncOp>(xla::cpu::createLegalizeI1VectorTransferOpsPass());
  pm.addNestedPass<FuncOp>(
      xla::cpu::createConvertXlaCpuMemRefElementCastToLLVMPass());
  return OkStatus();
}

Status CreateHloXlaRuntimePipeline(
    xla::runtime::PassManager& passes,
    const HloXlaRuntimePipelineOptions& options) {
  return CreateHloXlaPipeline(*passes, options);
}

Status CreateDefaultHloXlaRuntimePipeline(xla::runtime::PassManager& passes) {
  HloXlaRuntimePipelineOptions options;
  return CreateHloXlaPipeline(*passes, options);
}

void RegisterHloXlaRuntimePipelineDialects(mlir::DialectRegistry& dialects) {
  mlir::arith::registerBufferizableOpInterfaceExternalModels(dialects);
  mlir::bufferization::func_ext::registerBufferizableOpInterfaceExternalModels(
      dialects);
  mlir::memref::registerAllocationOpInterfaceExternalModels(dialects);
  mlir::linalg::registerBufferizableOpInterfaceExternalModels(dialects);
  mlir::linalg::registerTilingInterfaceExternalModels(dialects);
  mlir::mhlo::registerBufferizableOpInterfaceExternalModels(dialects);
  mlir::scf::registerBufferizableOpInterfaceExternalModels(dialects);
  mlir::shape::registerBufferizableOpInterfaceExternalModels(dialects);
  mlir::sparse_tensor::registerBufferizableOpInterfaceExternalModels(dialects);
  mlir::tensor::registerBufferizableOpInterfaceExternalModels(dialects);
  mlir::vector::registerBufferizableOpInterfaceExternalModels(dialects);
}

static mlir::PassPipelineRegistration<> hlo_xla_runtime_pipeline(
    "hlo-xla-runtime-pipeline",
    "Convert HLO dialect to XLA Runtime compatible dialects",
    [](mlir::OpPassManager& pm) {
      HloXlaRuntimePipelineOptions options;
      Status status = CreateHloXlaPipeline(pm, options);
      if (!status.ok()) {
        LOG(FATAL) << "HLO-XLA Runtime pipeline failed with: "
                   << status.message();
      }
    });

static mlir::PassPipelineRegistration<> sparsification_pipeline(
    "hlo-xla-runtime-sparsification",
    "Sparsification passes from HLO-XLA Runtime pipeline",
    AddSparsificationPassPipeline);

}  // namespace cpu
}  // namespace xla
