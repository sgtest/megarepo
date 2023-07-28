// RUN: export MSAN_OPTIONS=intercept_strpbrk=0
// RUN: xla-openxla-opt %s --xla-gpu-to-openxla --split-input-file             \
// RUN:   | FileCheck %s

func.func @main(%arg0: memref<16xi8>, %arg1: memref<16xi8>,
                %arg2: memref<16xi8>) {
  %c0 = arith.constant 0 : index
  %in = memref.view %arg0[%c0][] : memref<16xi8> to memref<4xf32>
  %out = memref.view %arg1[%c0][] : memref<16xi8> to memref<4xf32>
  %tmp = memref.view %arg2[%c0][] : memref<16xi8> to memref<4xi32>

  "lmhlo.sort"(%in, %tmp, %out, %tmp) ({
    ^bb0(%arg3: tensor<f32>, %arg4: tensor<f32>,
         %arg5: tensor<i32>, %arg6: tensor<i32>):
      %0 = "mhlo.fusion"(%arg5, %arg6, %arg4, %arg3) ({
        ^bb0(%arg7: tensor<i32>, %arg8: tensor<i32>,
             %arg9: tensor<f32>, %arg10: tensor<f32>):
          %0 = mhlo.constant dense<0> : tensor<i1>
          mhlo.return %0 : tensor<i1>
      }) : (tensor<i32>, tensor<i32>, tensor<f32>, tensor<f32>) -> tensor<i1>
      mhlo.return %0 : tensor<i1>
  }) {dimension = 0 : i64, is_stable = true}
     : (memref<4xf32>, memref<4xi32>, memref<4xf32>, memref<4xi32>) -> ()

  "lmhlo.terminator"() : () -> ()
}

// CHECK-LABEL: func @main(
// CHECK:   %[[CTX:.*]]: !xla_gpu.execution_context,
// CHECK:   %[[ARG0:.*]]: tensor<16xi8>, %[[ARG1:.*]]: tensor<16xi8>,
// CHECK:   %[[ARG2:.*]]: tensor<16xi8>
// CHECK: ) {
// CHECK:   %[[I:.*]] = iree_input.tensor.import {{.*}} -> tensor<4xf32>
// CHECK:   %[[O:.*]] = iree_input.tensor.import {{.*}} -> tensor<4xf32>
// CHECK:   %[[TMP:.*]] = iree_input.tensor.import {{.*}} -> tensor<4xi32>
// CHECK:   %[[SORT:.*]]:2 = iree_input.dispatch @xla.module.ptx
// CHECK:     (%[[I]], %[[TMP]], %[[O]], %[[TMP]]) {{.*}} -> (%[[O]], %[[TMP]])
// CHECK: }
