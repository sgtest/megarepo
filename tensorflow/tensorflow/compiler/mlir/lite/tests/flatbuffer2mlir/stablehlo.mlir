// RUN: flatbuffer_translate -mlir-to-tflite-flatbuffer %s -o - | flatbuffer_translate --tflite-flatbuffer-to-mlir - -o - | FileCheck %s
// test stablehlo roundtrip

// Identity function to make the exporter happy
func.func @main(%arg0: tensor<4xi8>) -> tensor<4xi8> {
  func.return %arg0 : tensor<4xi8>
}

//CHECK:func.func @main(%arg0: tensor<4xi8>) -> tensor<4xi8> attributes {tf.entry_function = {inputs = "arg0", outputs = "arg0"}} {
//CHECK: return %arg0 : tensor<4xi8>
//CHECK:}

func.func @logistic(%arg0: tensor<1x1x1x96xf32>) -> tensor<1x1x1x96xf32> {
  %0 = stablehlo.logistic %arg0 : tensor<1x1x1x96xf32>
  func.return %0 : tensor<1x1x1x96xf32>
}

// CHECK:func.func private @logistic(%arg0: tensor<1x1x1x96xf32>) -> tensor<1x1x1x96xf32> {
// CHECK: %0 = stablehlo.logistic %arg0 : tensor<1x1x1x96xf32>
// CHECK: return %0 : tensor<1x1x1x96xf32>
// CHECK:}

func.func @add(%arg0: tensor<1xf32>, %arg1: tensor<1xf32>) -> tensor<1xf32> {
  %0 = stablehlo.add %arg0, %arg1 : tensor<1xf32>
  func.return %0 : tensor<1xf32>
}

// CHECK:func.func private @add(%arg0: tensor<1xf32>, %arg1: tensor<1xf32>) -> tensor<1xf32> {
// CHECK: %0 = stablehlo.add %arg0, %arg1 : tensor<1xf32>
// CHECK: return %0 : tensor<1xf32>
// CHECK:}

func.func @multiply(%arg0: tensor<1xf32>, %arg1: tensor<1xf32>) -> tensor<1xf32> {
  %0 = stablehlo.multiply %arg0, %arg1 : tensor<1xf32>
  func.return %0 : tensor<1xf32>
}

// CHECK:func.func private @multiply(%arg0: tensor<1xf32>, %arg1: tensor<1xf32>) -> tensor<1xf32> {
// CHECK: %0 = stablehlo.multiply %arg0, %arg1 : tensor<1xf32>
// CHECK: return %0 : tensor<1xf32>
// CHECK:}

func.func @divide(%arg0: tensor<1xf32>, %arg1: tensor<1xf32>) -> tensor<1xf32> {
  %0 = stablehlo.divide %arg0, %arg1 : tensor<1xf32>
  func.return %0 : tensor<1xf32>
}

// CHECK:func.func private @divide(%arg0: tensor<1xf32>, %arg1: tensor<1xf32>) -> tensor<1xf32> {
// CHECK: %0 = stablehlo.divide %arg0, %arg1 : tensor<1xf32>
// CHECK: return %0 : tensor<1xf32>
// CHECK:}

func.func @maximum(%arg0: tensor<1xf32>, %arg1: tensor<1xf32>) -> tensor<1xf32> {
  %0 = stablehlo.maximum %arg0, %arg1 : tensor<1xf32>
  func.return %0 : tensor<1xf32>
}

// CHECK:func.func private @maximum(%arg0: tensor<1xf32>, %arg1: tensor<1xf32>) -> tensor<1xf32> {
// CHECK: %0 = stablehlo.maximum %arg0, %arg1 : tensor<1xf32>
// CHECK: return %0 : tensor<1xf32>
// CHECK:}

func.func @reshape(%arg0 : tensor<1x128xi32>) -> tensor<4x32x1xi32>{
  %0 = stablehlo.reshape %arg0 : (tensor<1x128xi32>) -> tensor<4x32x1xi32>
  func.return %0 : tensor<4x32x1xi32>
}

//CHECK:func.func private @reshape(%arg0: tensor<1x128xi32>) -> tensor<4x32x1xi32> {
//CHECK-NEXT: %0 = stablehlo.reshape %arg0 : (tensor<1x128xi32>) -> tensor<4x32x1xi32>
//CHECK-NEXT: return %0 : tensor<4x32x1xi32>
//CHECK-NEXT:}

func.func @clamp(%arg0: tensor<f32>, %arg1: tensor<1x256x256x24xf32>, %arg2: tensor<f32>) -> tensor<1x256x256x24xf32>{
  %0 = stablehlo.clamp %arg0, %arg1, %arg2 : (tensor<f32>, tensor<1x256x256x24xf32>, tensor<f32>) -> tensor<1x256x256x24xf32>
  return %0 : tensor<1x256x256x24xf32>
}

//CHECK:func.func private @clamp(%arg0: tensor<f32>, %arg1: tensor<1x256x256x24xf32>, %arg2: tensor<f32>) -> tensor<1x256x256x24xf32> {
//CHECK-NEXT: %0 = stablehlo.clamp %arg0, %arg1, %arg2 : (tensor<f32>, tensor<1x256x256x24xf32>, tensor<f32>) -> tensor<1x256x256x24xf32>
//CHECK-NEXT: return %0 : tensor<1x256x256x24xf32>
//CHECK-NEXT:}

func.func @concat(%arg0: tensor<1x30x1xi32>, %arg1: tensor<1x30x1xi32>) -> tensor<1x30x2xi32> {
  %0 = stablehlo.concatenate %arg0, %arg1, dim = 2 : (tensor<1x30x1xi32>, tensor<1x30x1xi32>) -> tensor<1x30x2xi32>
  func.return %0 : tensor<1x30x2xi32>
}

//CHECK:func.func private @concat(%arg0: tensor<1x30x1xi32>, %arg1: tensor<1x30x1xi32>) -> tensor<1x30x2xi32> {
//CHECK-NEXT: %0 = stablehlo.concatenate %arg0, %arg1, dim = 2 : (tensor<1x30x1xi32>, tensor<1x30x1xi32>) -> tensor<1x30x2xi32>
//CHECK-NEXT: return %0 : tensor<1x30x2xi32>
//CHECK-NEXT:}

func.func @broadcast_in_dim(%arg0: tensor<1x32x256xf32>) -> tensor<4x32x256xf32>{
  %0 = stablehlo.broadcast_in_dim %arg0, dims = [0, 1, 2] : (tensor<1x32x256xf32>) -> tensor<4x32x256xf32>
  return %0 : tensor<4x32x256xf32>
}

//CHECK:func.func private @broadcast_in_dim(%arg0: tensor<1x32x256xf32>) -> tensor<4x32x256xf32> {
//CHECK-NEXT: %0 = stablehlo.broadcast_in_dim %arg0, dims = [0, 1, 2] : (tensor<1x32x256xf32>) -> tensor<4x32x256xf32>
//CHECK-NEXT: return %0 : tensor<4x32x256xf32>
//CHECK-NEXT:}

func.func @slice(%arg0: tensor<160x20x1xf32>) -> tensor<1x1x1xf32> {
  %0 = stablehlo.slice %arg0 [0:1:1, 0:1:1, 0:1:1] : (tensor<160x20x1xf32>) -> tensor<1x1x1xf32>
  return %0 : tensor<1x1x1xf32>
}

//CHECK:func.func private @slice(%arg0: tensor<160x20x1xf32>) -> tensor<1x1x1xf32> {
//CHECK-NEXT: %0 = stablehlo.slice %arg0 [0:1, 0:1, 0:1] : (tensor<160x20x1xf32>) -> tensor<1x1x1xf32>
//CHECK-NEXT: return %0 : tensor<1x1x1xf32>
//CHECK-NEXT:}

func.func @convolution(%arg0: tensor<1x1x1600x32xf32>, %arg1: tensor<1x13x1x32xf32>) -> tensor<1x1x1600x32xf32> {
  %0 = stablehlo.convolution(%arg0, %arg1) dim_numbers = [b, 0, 1, f]x[0, 1, i, o]->[b, 0, 1, f], window = {stride = [1, 1], pad = [[0, 0], [6, 6]], rhs_dilate = [1, 1]} {batch_group_count = 1 : i64, feature_group_count = 32 : i64, precision_config = [#stablehlo<precision DEFAULT>, #stablehlo<precision DEFAULT>]} : (tensor<1x1x1600x32xf32>, tensor<1x13x1x32xf32>) -> tensor<1x1x1600x32xf32>
  return %0 : tensor<1x1x1600x32xf32>
}

//CHECK:func.func private @convolution(%arg0: tensor<1x1x1600x32xf32>, %arg1: tensor<1x13x1x32xf32>) -> tensor<1x1x1600x32xf32> {
//CHECK-NEXT{LITERAL}: %0 = stablehlo.convolution(%arg0, %arg1) dim_numbers = [b, 0, 1, f]x[0, 1, i, o]->[b, 0, 1, f], window = {stride = [1, 1], pad = [[0, 0], [6, 6]], rhs_dilate = [1, 1]} {batch_group_count = 1 : i64, feature_group_count = 32 : i64, precision_config = [#stablehlo<precision DEFAULT>, #stablehlo<precision DEFAULT>]} : (tensor<1x1x1600x32xf32>, tensor<1x13x1x32xf32>) -> tensor<1x1x1600x32xf32>
//CHECK-NEXT: return %0 : tensor<1x1x1600x32xf32>
//CHECK-NEXT:}