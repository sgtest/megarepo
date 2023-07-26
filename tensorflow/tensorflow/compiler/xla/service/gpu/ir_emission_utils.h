/* Copyright 2017 The TensorFlow Authors. All Rights Reserved.

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

#ifndef TENSORFLOW_COMPILER_XLA_SERVICE_GPU_IR_EMISSION_UTILS_H_
#define TENSORFLOW_COMPILER_XLA_SERVICE_GPU_IR_EMISSION_UTILS_H_

#include <optional>
#include <string>
#include <vector>

#include "llvm/IR/IRBuilder.h"
#include "llvm/IR/Value.h"
#include "tensorflow/compiler/xla/hlo/ir/hlo_instruction.h"
#include "tensorflow/compiler/xla/mlir_hlo/lhlo/IR/lhlo_ops.h"
#include "tensorflow/compiler/xla/service/buffer_assignment.h"

namespace xla {
namespace gpu {

// If a dimensions is smaller than this, untiled transposition may be more
// efficient.
inline constexpr int64_t kMinDimensionToTransposeTiled = 16;
// But if both swap dimensions are larger than 'kMinDimensionToTransposeTiled2',
// and the product of the dimensions to be swapped is larger than
// 'kMinTotalDimensionsToTransposeTiled', tiled transposition may be more
// efficient.
inline constexpr int64_t kMinDimensionToTransposeTiled2 = 8;
inline constexpr int64_t kMinTotalDimensionsToTransposeTiled = 64 * 128;

// Matrix multiplication before the rewrite.
//
// This function should never return "true" on instructions after
// GemmRewriter pass has finished.
bool IsMatrixMultiplication(const HloInstruction& dot);

inline constexpr int64_t WarpSize() { return 32; }

// Fusions that use Triton have FusionBackendConfig.kind equal to this string.
inline constexpr absl::string_view kTritonGemmFusionKind = "__triton_gemm";

// SoftmaxRewriterTriton sets backend_config of Triton Softmax custom fusions to
// this string.
inline constexpr absl::string_view kTritonSoftmaxFusionKind =
    "__triton_softmax";

inline constexpr absl::string_view kUncompilableFusion =
    "__uncompilable_fusion";

// Returns true if `hlo` will be implemented as a call to a cuSolver routine.
//
// This returns true if `hlo` is a CustomCall HLO with a call target equal to
// one of the kCusolver... constants, but returns *false* for HLOs with
// say, a kCholesky opcode.
bool IsCustomCallToCusolver(const HloInstruction& hlo);

// Cholesky decomposition. Takes a (batched) matrix as input, and returns a
// tuple of (result, workspace, info), where result is the result of the
// Cholesky decomposition, workspace is scratch space for cuSolver, and info
// is a success/failure code per batch element.
extern const char* const kCusolverCholeskyCallTarget;

// Returns whether unnested_hlo is an input fusion whose root is either a slice
// or a tuple of slices. If verify_no_strides is true, returns false unless all
// ROOT slices have no strides.
bool IsInputFusibleSlices(mlir::Operation* unnested_hlo,
                          bool verify_no_strides);

// Emits call to "vprintf" with given format and arguments.
llvm::Value* EmitPrintf(absl::string_view fmt,
                        absl::Span<llvm::Value* const> arguments,
                        llvm::IRBuilder<>* builder);

// Emits code to shuffle data between threads of a warp. This has the same
// semantics as the PTX "shfl.sync.down" instruction but works for values that
// aren't 32 bits in size. The last operand of the emitted "shfl" is
// `WarpSize() - 1`.
//
// This function emits a "full-warp" shuffle, which all threads of a warp
// participate in.  *Do not use this function from a divergent context:* You
// can't correctly do so on both Volta and earlier GPUs.
//
// https://docs.nvidia.com/cuda/parallel-thread-execution/#data-movement-and-conversion-instructions-shfl-sync
llvm::Value* EmitFullWarpShuffleDown(llvm::Value* value, llvm::Value* offset,
                                     llvm::IRBuilder<>* builder);

// Emits code that determines whether the current thread is thread 0 within
// block 0 of the kernel.
llvm::Value* IsBlock0Thread0(llvm::IRBuilder<>* b);

int PartitionLmhloOperandsAndOutputs(mlir::Operation* op);
llvm::SmallVector<mlir::Value> GetHloOperands(mlir::Operation* op);
llvm::SmallVector<mlir::Value> GetHloOutputs(mlir::Operation* op);

bool WritesMlirBuffer(mlir::Operation* op, mlir::Value operand);

template <typename T>
std::vector<T> ToStdVector(const llvm::SmallVectorImpl<T>& v) {
  return std::vector<T>(v.begin(), v.end());
}

StatusOr<BufferAllocation::Slice> GetAllocationSlice(
    mlir::Value v, absl::Span<const BufferAllocation> allocations,
    std::string* constant_name = nullptr);

bool CanEmitFusedDynamicUpdateSliceInPlaceForGpu(
    mlir::lmhlo::FusionOp fusion,
    absl::Span<const BufferAllocation> allocations);

// Returns the dynamic-update-slice instructions defining the results of a
// fusion node. A dynamic slice update is said to be "defining" of a result if
// that result is the output of a dynamic slice update, or if that result is the
// output of a bitcast of a dynamic slice update---since such bitcast may be
// handled as a no-op.
std::vector<HloInstruction*> GetOutputDefiningDynamicUpdateSlices(
    const HloComputation* fusion);

// Returns the DynamicUpdateSliceOp(s) defining the results of a fusion node.
// A dynamic slice update is said to be "defining" of a result if that result is
// the output of a dynamic slice update, or if that result is the output of a
// bitcast of a dynamic slice update---since such bitcast may be handled as a
// no-op.
std::vector<mlir::mhlo::DynamicUpdateSliceOp>
GetOutputDefiningDynamicUpdateSliceOps(mlir::lmhlo::FusionOp fusion);

Shape GetShape(mlir::Value value);

/// Description of how to emit a given transposition.
//
// On a group of input parameters that are 0-2-1 transpose of the outputs
// of a fusion kernel, stores the input parameters that are safe for the
// shared memory transpose implementation and the dimension permutation.
//
// When a tile based shared memory transpose is used to implement an input
// with 0-2-1 transpose, we preload a tile of the input elements [z, y..y+31,
// x..x+31] to compute the output tile elements of the same indices.
// Preloading the input tile this way is only safe when the computation of the
// output tile elements do not need any input element outside the preloaded
// tile. We inspect all the transitive users of the input parameter up to the
// fusion root instruction to see if we can find any instruction that can make
// preloading the input tile unsafe.
struct TransposeDimsAndParams {
  // Permutation of the dimensions relative to output.
  Vector3 dims;

  // Indices of parameters which are permuted.
  std::vector<int64_t> params;

  std::string ToString() const {
    return absl::StrFormat("{dims={%s}, params={%s}}",
                           absl::StrJoin(dims, ", "),
                           absl::StrJoin(params, ", "));
  }
};

const HloInstruction& FindNonTrivialHero(const HloInstruction& instr);

struct TransposeDescription {
  Vector3 dimensions;
  Vector3 permutation;

  TransposeDescription(Vector3 dimensions, Vector3 permutation)
      : dimensions(dimensions), permutation(permutation) {}

  std::string ToString() const {
    return absl::StrCat("dimensions=", VectorString(dimensions),
                        ", permutation=", VectorString(permutation));
  }

  bool operator==(const TransposeDescription& other) const {
    return dimensions == other.dimensions && permutation == other.permutation;
  }

  bool operator!=(const TransposeDescription& other) const {
    return !(*this == other);
  }
};

std::optional<TransposeDescription> FindTiledTranspose(
    const HloInstruction& instr);

std::optional<TransposeDescription> FindTiledLogicalTranspose(
    const HloInstruction& instr);

std::optional<TransposeDescription> FindAnyTiledTranspose(
    const HloInstruction& instr);

bool IsIntermediate(const HloInstruction* instr, int allowed_operand_count = 1);

// Log and verify an LLVM module.
void LogAndVerify(const llvm::Module* m);

// Returns the llvm type for the indices used in the kernel that contains the
// hlo instruction. Such indices include the index for the parallel loop and
// the indices for the tensors accessed by the kernel. The return type is i32
// iff the following conditions are met:
//  . The launch_size of the kernel is within the range of i32.
//  . The sizes of all the tensors accessed within the kernel are within the
//    range of i32.
// Otherwise, the return type is i64.
llvm::Type* GetIndexTypeForKernel(const HloInstruction* hlo,
                                  int64_t launch_size, llvm::IRBuilder<>* b);

// The same as GetIndexTypeForKernel, but works with MLIR ops.
llvm::Type* GetIndexTypeForKernel(mlir::Operation* op, int64_t launch_size,
                                  llvm::IRBuilder<>* b);

// Returns a sanitized (doesn't need quoting) identifier name from a location.
std::string GetIrNameFromLoc(mlir::Location loc);

// Whether the module's target is an AMD GPU.
bool IsAMDGPU(const llvm::Module* module);

}  // namespace gpu
}  // namespace xla

#endif  // TENSORFLOW_COMPILER_XLA_SERVICE_GPU_IR_EMISSION_UTILS_H_
