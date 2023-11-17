// RUN: mlir-hlo-opt %s \
// RUN: --verify-diagnostics \
// RUN: --mhlo-test-lower-general-dot --canonicalize | FileCheck %s

#SV  = #sparse_tensor.encoding<{ map = (d0) -> (d0 : compressed) }>
#DCSR = #sparse_tensor.encoding<{ map = (d0, d1) -> (d0 : compressed, d1 : compressed) }>
#COO = #sparse_tensor.encoding<{ map = (d0, d1, d2) -> (d0 : compressed(nonunique), d1 : singleton(nonunique), d2 : singleton) }>

// CHECK: #[[$SV:.*]] = #sparse_tensor.encoding<{ map = (d0) -> (d0 : compressed) }>
// CHECK: #[[$DCSR:.*]] = #sparse_tensor.encoding<{ map = (d0, d1) -> (d0 : compressed, d1 : compressed) }>
// CHECK: #[[$COO:.*]] = #sparse_tensor.encoding<{ map = (d0, d1, d2) -> (d0 : compressed(nonunique), d1 : singleton(nonunique), d2 : singleton) }>

//
// Vector-vector gendot.
//
// CHECK-LABEL: func.func @sparse_vecvec(
// CHECK-SAME:    %[[ARG0:.*]]: tensor<10xf64, #[[$SV]]>,
// CHECK-SAME:    %[[ARG1:.*]]: tensor<10xf64, #[[$SV]]>) -> tensor<f64> {
// CHECK:         %[[DOT:.*]] = "mhlo.dot"(%[[ARG0]], %[[ARG1]]) {precision_config = [#mhlo<precision DEFAULT>, #mhlo<precision DEFAULT>]} : (tensor<10xf64, #[[$SV]]>, tensor<10xf64, #[[$SV]]>) -> tensor<f64>
// CHECK:         return %[[DOT]] : tensor<f64>
// CHECK:       }
//
func.func @sparse_vecvec(%arg0: tensor<10xf64, #SV>,
                         %arg1: tensor<10xf64, #SV>) -> tensor<f64> {
  %0 = "mhlo.dot_general"(%arg0, %arg1) {
    dot_dimension_numbers = #mhlo.dot<lhs_contracting_dimensions = [0],
                                      rhs_contracting_dimensions = [0]>,
     precision_config = [#mhlo<precision DEFAULT>,
                         #mhlo<precision DEFAULT>]}
    : (tensor<10xf64, #SV>,
       tensor<10xf64, #SV>) -> tensor<f64>
  return %0 : tensor<f64>
}

//
// Matrix-vector gendot.
//
// CHECK-LABEL: func.func @sparse_matvec(
// CHECK-SAME:    %[[ARG0:.*]]: tensor<3x5xf64, #[[$DCSR]]>,
// CHECK-SAME:    %[[ARG1:.*]]: tensor<5xf64, #[[$SV]]>) -> tensor<3xf64> {
// CHECK:         %[[DOT:.*]] = "mhlo.dot"(%[[ARG0]], %[[ARG1]]) {precision_config = [#mhlo<precision DEFAULT>, #mhlo<precision DEFAULT>]} : (tensor<3x5xf64, #[[$DCSR]]>, tensor<5xf64, #[[$SV]]>) -> tensor<3xf64>
// CHECK:        return %[[DOT]] : tensor<3xf64>
// CHECK:       }
//
func.func @sparse_matvec(%arg0: tensor<3x5xf64, #DCSR>,
                         %arg1: tensor<5xf64, #SV>) -> tensor<3xf64> {
  %0 = "mhlo.dot_general"(%arg0, %arg1) {
    dot_dimension_numbers = #mhlo.dot<lhs_contracting_dimensions = [1],
                                      rhs_contracting_dimensions = [0]>,
     precision_config = [#mhlo<precision DEFAULT>,
                         #mhlo<precision DEFAULT>]}
    : (tensor<3x5xf64, #DCSR>,
       tensor<5xf64, #SV>) -> tensor<3xf64>
  return %0 : tensor<3xf64>
}

//
// Matrix-matrix gendot, one sparse operand.
//
// CHECK-LABEL: func.func @sparse_matmat_1s(
// CHECK-SAME:    %[[ARG0:.*]]: tensor<16x32xf64, #[[$DCSR]]>,
// CHECK-SAME:    %[[ARG1:.*]]: tensor<32x64xf64>) -> tensor<16x64xf64> {
// CHECK:         %[[DOT:.*]] = "mhlo.dot"(%[[ARG0]], %[[ARG1]]) {precision_config = [#mhlo<precision DEFAULT>, #mhlo<precision DEFAULT>]} : (tensor<16x32xf64, #[[$DCSR]]>, tensor<32x64xf64>) -> tensor<16x64xf64>
// CHECK:         return %[[DOT]] : tensor<16x64xf64>
// CHECK:       }
//
func.func @sparse_matmat_1s(%arg0: tensor<16x32xf64, #DCSR>,
                            %arg1: tensor<32x64xf64>) -> tensor<16x64xf64> {
  %0 = "mhlo.dot_general"(%arg0, %arg1) {
    dot_dimension_numbers = #mhlo.dot<lhs_contracting_dimensions = [1],
                                      rhs_contracting_dimensions = [0]>,
    precision_config = [#mhlo<precision DEFAULT>,
                        #mhlo<precision DEFAULT>]}
    : (tensor<16x32xf64, #DCSR>,
       tensor<32x64xf64>) -> tensor<16x64xf64>
  return %0 : tensor<16x64xf64>
}

//
// Matrix-matrix gendot, everything sparse.
//
// CHECK-LABEL: func.func @sparse_matmat_as(
// CHECK-SAME:    %[[ARG0:.*]]: tensor<16x32xf64, #[[$DCSR]]>,
// CHECK-SAME:    %[[ARG1:.*]]: tensor<32x64xf64, #[[$DCSR]]>) -> tensor<16x64xf64, #[[$DCSR]]> {
// CHECK:         %[[DOT:.*]] = "mhlo.dot"(%[[ARG0]], %[[ARG1]]) {precision_config = [#mhlo<precision DEFAULT>, #mhlo<precision DEFAULT>]} : (tensor<16x32xf64, #[[$DCSR]]>, tensor<32x64xf64, #[[$DCSR]]>) -> tensor<16x64xf64, #[[$DCSR]]>
// CHECK:         return %[[DOT]] : tensor<16x64xf64, #[[$DCSR]]>
// CHECK:       }
//
func.func @sparse_matmat_as(%arg0: tensor<16x32xf64, #DCSR>,
                            %arg1: tensor<32x64xf64, #DCSR>) -> tensor<16x64xf64, #DCSR> {
  %0 = "mhlo.dot_general"(%arg0, %arg1) {
    dot_dimension_numbers = #mhlo.dot<lhs_contracting_dimensions = [1],
                                      rhs_contracting_dimensions = [0]>,
    precision_config = [#mhlo<precision DEFAULT>,
                        #mhlo<precision DEFAULT>]}
    : (tensor<16x32xf64, #DCSR>,
       tensor<32x64xf64, #DCSR>) -> tensor<16x64xf64, #DCSR>
  return %0 : tensor<16x64xf64, #DCSR>
}

//
// Higher-order gendot.
//
// A situation that would introduce sparse reshape operations is not rewritten.
//
// CHECK-LABEL: func.func @sparse_tensor(
// CHECK-SAME:    %[[ARG0:.*]]: tensor<197x12x64xf32>,
// CHECK-SAME:    %[[ARG1:.*]]: tensor<12x64x768xf32, #[[$COO]]>) -> tensor<197x768xf32> {
// CHECK:         %[[R:.*]] = "mhlo.dot_general"(%[[ARG0]], %[[ARG1]])
// CHECK:         return %[[R]] : tensor<197x768xf32>
func.func @sparse_tensor(%arg0: tensor<197x12x64xf32>,
                         %arg1: tensor<12x64x768xf32, #COO>) -> tensor<197x768xf32> {
   %0 = "mhlo.dot_general"(%arg0, %arg1)
       {dot_dimension_numbers = #mhlo.dot<lhs_contracting_dimensions = [1, 2],
                                          rhs_contracting_dimensions = [0, 1]>,
	precision_config = [#mhlo<precision DEFAULT>,
	                    #mhlo<precision DEFAULT>]}
    : (tensor<197x12x64xf32>,
       tensor<12x64x768xf32, #COO>) -> tensor<197x768xf32>
  return %0 : tensor<197x768xf32>
}
