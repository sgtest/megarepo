// RUN: mlir-hlo-opt %s \
// RUN:   --verify-diagnostics \
// RUN:   --canonicalize | FileCheck %s

#DCSR = #sparse_tensor.encoding<{
  map = (d0, d1) -> (d0 : compressed, d1 : compressed)
}>

//
// Tests that ensure trivial transposes are folded,
// but the simplified code still accounts for sparsity.
//

// CHECK-LABEL: func @transpose1(
//  CHECK-SAME: %[[A:.*]]: tensor<100x100xf64>)
//       CHECK: return %[[A]] : tensor<100x100xf64>
func.func @transpose1(%arg0: tensor<100x100xf64>)
                          -> tensor<100x100xf64> {
  %0 = "mhlo.transpose"(%arg0)
       {permutation = dense<[0, 1]> : tensor<2xi64>}
     : (tensor<100x100xf64>) -> tensor<100x100xf64>
  return %0 : tensor<100x100xf64>
}

// CHECK-LABEL: func @transpose2(
//  CHECK-SAME: %[[A:.*]]: tensor<100x100xf64, #sparse{{[0-9]*}}>)
//       CHECK: return %[[A]] : tensor<100x100xf64, #sparse{{[0-9]*}}>
func.func @transpose2(%arg0: tensor<100x100xf64, #DCSR>)
                          -> tensor<100x100xf64, #DCSR> {
  %0 = "mhlo.transpose"(%arg0)
       {permutation = dense<[0, 1]> : tensor<2xi64>}
     : (tensor<100x100xf64, #DCSR>) -> tensor<100x100xf64, #DCSR>
  return %0 : tensor<100x100xf64, #DCSR>
}

// CHECK-LABEL: func @transpose3(
//  CHECK-SAME: %[[A:.*]]: tensor<100x100xf64, #sparse{{[0-9]*}}>)
//       CHECK: %[[R:.*]] = mhlo.reshape %[[A]] : (tensor<100x100xf64, #sparse{{[0-9]*}}>) -> tensor<100x100xf64>
//       CHECK: return %[[R]] : tensor<100x100xf64>
func.func @transpose3(%arg0: tensor<100x100xf64, #DCSR>)
                          -> tensor<100x100xf64> {
  %0 = "mhlo.transpose"(%arg0)
       {permutation = dense<[0, 1]> : tensor<2xi64>}
     : (tensor<100x100xf64, #DCSR>) -> tensor<100x100xf64>
  return %0 : tensor<100x100xf64>
}

// CHECK-LABEL: func @transpose4(
//  CHECK-SAME: %[[A:.*]]: tensor<100x100xf64>)
//       CHECK: %[[R:.*]] = mhlo.reshape %[[A]] : (tensor<100x100xf64>) -> tensor<100x100xf64, #sparse{{[0-9]*}}>
//       CHECK: return %[[R]] : tensor<100x100xf64, #sparse{{[0-9]*}}>
func.func @transpose4(%arg0: tensor<100x100xf64>)
                          -> tensor<100x100xf64, #DCSR> {
  %0 = "mhlo.transpose"(%arg0)
       {permutation = dense<[0, 1]> : tensor<2xi64>}
     : (tensor<100x100xf64>) -> tensor<100x100xf64, #DCSR>
  return %0 : tensor<100x100xf64, #DCSR>
}
