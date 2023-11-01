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

#ifndef XLA_SERVICE_GPU_IR_EMISSION_UTILS_H_
#define XLA_SERVICE_GPU_IR_EMISSION_UTILS_H_

#include <optional>
#include <string>
#include <vector>

#include "llvm/IR/IRBuilder.h"
#include "llvm/IR/Value.h"
#include "xla/hlo/ir/hlo_instruction.h"
#include "xla/mlir_hlo/lhlo/IR/lhlo_ops.h"
#include "xla/service/buffer_assignment.h"

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
    mlir::Value v, absl::Span<const BufferAllocation* const> allocations,
    std::string* constant_name = nullptr);

bool IsSingleInstructionFusion(mlir::lmhlo::FusionOp fusion);

bool CanEmitFusedDynamicUpdateSliceInPlaceForGpu(
    mlir::lmhlo::FusionOp fusion,
    absl::Span<const BufferAllocation* const> allocations);

// Returns the dynamic-update-slice instructions defining the results of a
// fusion node. A dynamic slice update is said to be "defining" of a result if
// that result is the output of a dynamic slice update, or if that result is the
// output of a bitcast of a dynamic slice update---since such bitcast may be
// handled as a no-op.
std::vector<const HloInstruction*> GetOutputDefiningDynamicUpdateSlices(
    const std::vector<const HloInstruction*>& roots);

// Returns the DynamicUpdateSliceOp(s) defining the results of a fusion node.
// A dynamic slice update is said to be "defining" of a result if that result is
// the output of a dynamic slice update, or if that result is the output of a
// bitcast of a dynamic slice update---since such bitcast may be handled as a
// no-op.
std::vector<mlir::mhlo::DynamicUpdateSliceOp>
GetOutputDefiningDynamicUpdateSliceOps(mlir::lmhlo::FusionOp fusion);

Shape GetShape(mlir::Value value);

// `is_boundary` returns `true` for edges that are on the boundary of the
// fusion, i.e., they go from an instruction inside the fusion to one outside,
// or vice versa.
// Note: when this is called with a fusion instruction, it will traverse into
// the fusion (unless the boundary function stops it).
const HloInstruction& FindNonTrivialHero(
    const HloInstruction& instr,
    const std::function<bool(const HloInstruction& producer,
                             const HloInstruction& consumer)>& is_boundary);
// Like above, with the default boundary function. Additionally, this will not
// traverse into `instr`'s computation if it is a fusion.
const HloInstruction& FindNonTrivialHero(const HloInstruction& instr);

/// Description of how to emit a given transposition.
struct TransposeDescription {
  // Transpose instruction.
  const HloInstruction* instr;

  // Normalized transpose dimensions.
  Vector3 dimensions;

  // Permutations of normalized transpose dimensions.
  Vector3 permutation;

  TransposeDescription(Vector3 dimensions, Vector3 permutation)
      : TransposeDescription(/*instr=*/nullptr, dimensions, permutation) {}

  TransposeDescription(const HloInstruction* instr, Vector3 dimensions,
                       Vector3 permutation)
      : instr(instr), dimensions(dimensions), permutation(permutation) {}

  std::string ToString() const {
    return absl::StrCat("dimensions=", VectorString(dimensions),
                        ", permutation=", VectorString(permutation));
  }

  // Transpose instruction input shape.
  const Shape& input_shape() const { return instr->operand(0)->shape(); }

  // Returns true, if both descriptions have the same dimensions and
  // permutation, even if they're produced by different instructions.
  bool IsEquivalent(const TransposeDescription& other) const {
    return dimensions == other.dimensions && permutation == other.permutation;
  }
};

std::optional<TransposeDescription> FindTiledTranspose(
    const HloInstruction& instr);

std::optional<TransposeDescription> FindTiledLogicalTranspose(
    const HloInstruction& instr);

std::optional<TransposeDescription> GetDescriptionForTiledTransposeEmitter(
    const HloInstruction& root, const HloInstruction& hero);

bool IsIntermediate(const HloInstruction* instr, int allowed_operand_count = 1);

// Log the given module if the VLOG level is >= level.
void VLogModule(int level, const llvm::Module& module);

// Verify the given module, and crash if it failed.
void VerifyModule(const llvm::Module& module);

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

#endif  // XLA_SERVICE_GPU_IR_EMISSION_UTILS_H_
