// RUN: export MSAN_OPTIONS=intercept_strpbrk=0
// RUN: xla-openxla-opt %s --xla-gpu-to-openxla=backend=streamexecutor         \
// RUN:                    --split-input-file                                  \
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

// CHECK-LABEL: func @main(
// CHECK:   %[[CTX:.*]]: !xla_gpu.execution_context,
// CHECK:   %[[ARG0:.*]]: tensor<4xi8>, %[[ARG1:.*]]: tensor<4xi8>,
// CHECK:   %[[ARG2:.*]]: tensor<1xi8>
// CHECK: ) {

// CHECK:   %[[BUFFER:.*]] = iree_input.tensor.import {{.*}} -> tensor<1xf32>
// CHECK:   %[[CST:.*]] = iree_input.tensor.import {{.*}} -> tensor<1xf32>
// CHECK:   %[[PRED:.*]] = iree_input.tensor.import {{.*}} -> tensor<1xi1>

// CHECK:   scf.while : () -> () {
// CHECK:      func.call @xla_gpu.kernel.dispatch
// CHECK:      %[[PRED:.*]] = func.call @xla_gpu.memcpy.load.i1
// CHECK:      scf.condition(%[[PRED]])
// CHECK:   }

// CHECK:   do {
// CHECK:     func.call @xla_gpu.kernel.dispatch
// CHECK:     scf.yield
// CHECK:   }

// CHECK: }
