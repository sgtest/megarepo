// RUN: export MSAN_OPTIONS=intercept_strpbrk=0
// RUN: xla-openxla-opt %s --xla-gpu-to-openxla --split-input-file             \
// RUN:   | FileCheck %s

func.func @main(%arg0: memref<4xi8>, %arg1: memref<4xi8>, %arg2: memref<1xi8>) {
  %c0 = arith.constant 0 : index

  %buffer = memref.view %arg0[%c0][] : memref<4xi8> to memref<1xf32>
  %cst = memref.view %arg1[%c0][] : memref<4xi8> to memref<1xf32>
  %pred = memref.view %arg2[%c0][] : memref<1xi8> to memref<i1>

  "lmhlo.while"(%pred) ({
    "lmhlo.fusion"()({
      %0 = bufferization.to_tensor %pred : memref<i1>
      memref.tensor_store %0, %pred : memref<i1>
      "lmhlo.terminator"() : () ->()
    }) : () -> ()
    "lmhlo.terminator"() : () -> ()
  }, {
    "lmhlo.fusion"() ({
      %1 = bufferization.to_tensor %buffer : memref<1xf32>
      %2 = bufferization.to_tensor %cst : memref<1xf32>
      %3 = mhlo.add %1, %2 : tensor<1xf32>
      memref.tensor_store %3, %buffer : memref<1xf32>
      "lmhlo.terminator"() : () -> ()
    }) : () -> ()
    "lmhlo.terminator"() : () -> ()
  }) : (memref<i1>) -> ()

  "lmhlo.terminator"() : () -> ()
}

// Capture %buffer and %pred as loop-carried SSA values, as they are updated
// in the condition and body block. While loop returns the latest version
// of both tensors as a result.

// CHECK-LABEL: func @main(
// CHECK:   %[[CTX:.*]]: !xla_gpu.execution_context,
// CHECK:   %[[ARG0:.*]]: tensor<4xi8>, %[[ARG1:.*]]: tensor<4xi8>,
// CHECK:   %[[ARG2:.*]]: tensor<1xi8>
// CHECK: ) {

// CHECK:   %[[BUFFER:.*]] = iree_input.tensor.import {{.*}} -> tensor<1xf32>
// CHECK:   %[[CST:.*]] = iree_input.tensor.import {{.*}} -> tensor<1xf32>
// CHECK:   %[[PRED:.*]] = iree_input.tensor.import {{.*}} -> tensor<1xi1>

// CHECK:   %[[LOOP:.*]]:2 = scf.while (%[[COND_PRED:.*]] = %[[PRED]],
// CHECK:                               %[[COND_BUF:.*]] = %[[BUFFER]])
// CHECK:     : (tensor<1xi1>, tensor<1xf32>) -> (tensor<1xi1>, tensor<1xf32>) {
// CHECK:      %[[NEXT_PRED:.*]] = iree_input.dispatch @xla.module.ptx
// CHECK:        (%[[COND_PRED]], %[[COND_PRED]]) {{.*}} -> %[[COND_PRED]]
// CHECK:      %[[NEXT:.*]] = iree_input.tensor.load %[[NEXT_PRED]]
// CHECK:      scf.condition(%[[NEXT]]) %[[NEXT_PRED]], %[[COND_BUF]]
// CHECK:   }

// CHECK:   do {
// CHECK:     ^[[BB:.*]](%[[BODY_PRED:.*]]: tensor<1xi1>,
// CHECK:                %[[BODY_BUF:.*]]: tensor<1xf32>):
// CHECK:     %[[NEXT_BUFFER:.*]] = iree_input.dispatch @xla.module.ptx
// CHECK:       (%[[BODY_BUF]], %[[CST]], %[[BODY_BUF]]) {{.*}} -> %[[BODY_BUF]]
// CHECK:     scf.yield %[[BODY_PRED]], %[[NEXT_BUFFER]]
// CHECK:   }

// CHECK: }
