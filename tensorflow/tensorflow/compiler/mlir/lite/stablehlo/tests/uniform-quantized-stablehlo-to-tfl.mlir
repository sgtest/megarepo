// RUN: odml-to-stablehlo-opt --uniform-quantized-stablehlo-to-tfl \
// RUN:     --split-input-file --verify-diagnostics %s | FileCheck %s

// CHECK-LABEL: uniform_quantize_op
func.func @uniform_quantize_op(%arg: tensor<2x2xf32>) -> tensor<2x2x!quant.uniform<i8:f32, 3.000000e+0:127>> {
  %0 = stablehlo.uniform_quantize %arg : (tensor<2x2xf32>) -> tensor<2x2x!quant.uniform<i8:f32, 3.000000e+0:127>>
  return %0 : tensor<2x2x!quant.uniform<i8:f32, 3.000000e+0:127>>
}
// CHECK: %[[QUANT:.*]] = "tfl.quantize"({{.*}}) {qtype = tensor<2x2x!quant.uniform<i8:f32, 3.000000e+00:127>>} : (tensor<2x2xf32>) -> tensor<2x2x!quant.uniform<i8:f32, 3.000000e+00:127>>
// CHECK: return %[[QUANT]]

// -----

// Tests that the pattern doesn't match when the input tensor's type is a
// quantized type.

// CHECK-LABEL: uniform_quantize_op_quantized_input
func.func @uniform_quantize_op_quantized_input(%arg: tensor<2x2x!quant.uniform<i8:f32, 2.000000e+0:16>>) -> tensor<2x2x!quant.uniform<i8:f32, 3.000000e+0:127>> {
  %0 = stablehlo.uniform_quantize %arg : (tensor<2x2x!quant.uniform<i8:f32, 2.000000e+0:16>>) -> tensor<2x2x!quant.uniform<i8:f32, 3.000000e+0:127>>
  return %0 : tensor<2x2x!quant.uniform<i8:f32, 3.000000e+0:127>>
}
// CHECK: stablehlo.uniform_quantize
// CHECK-NOT: tfl.quantize

// -----

// Tests that the pattern doesn't match when the output tensor's storage type
// is ui16. ui16 storage type for quantized type is not compatible with
// `tfl.quantize`.

// CHECK-LABEL: uniform_quantize_op_uint16_output
func.func @uniform_quantize_op_uint16_output(%arg: tensor<2x2xf32>) -> tensor<2x2x!quant.uniform<ui16:f32, 3.000000e+0:127>> {
  %0 = stablehlo.uniform_quantize %arg : (tensor<2x2xf32>) -> tensor<2x2x!quant.uniform<ui16:f32, 3.000000e+0:127>>
  return %0 : tensor<2x2x!quant.uniform<ui16:f32, 3.000000e+0:127>>
}
// CHECK: stablehlo.uniform_quantize
// CHECK-NOT: tfl.quantize

// -----

// Tests that the pattern doesn't match when the output tensor's storage type
// is i32. i32 storage type for quantized type is not compatible with
// `tfl.quantize`.

// CHECK-LABEL: uniform_quantize_op_i32_output
func.func @uniform_quantize_op_i32_output(%arg: tensor<2x2xf32>) -> tensor<2x2x!quant.uniform<i32:f32, 3.000000e+0:127>> {
  %0 = stablehlo.uniform_quantize %arg : (tensor<2x2xf32>) -> tensor<2x2x!quant.uniform<i32:f32, 3.000000e+0:127>>
  return %0 : tensor<2x2x!quant.uniform<i32:f32, 3.000000e+0:127>>
}
// CHECK: stablehlo.uniform_quantize
// CHECK-NOT: tfl.quantize

// -----

// CHECK-LABEL: uniform_dequantize_op
func.func @uniform_dequantize_op(%arg: tensor<2x2x!quant.uniform<i8:f32, 1.000000e+0:8>>) -> tensor<2x2xf32> {
  %0 = stablehlo.uniform_dequantize %arg : (tensor<2x2x!quant.uniform<i8:f32, 1.000000e+0:8>>) -> tensor<2x2xf32>
  return %0 : tensor<2x2xf32>
}
// CHECK: %[[DEQUANT:.*]] = "tfl.dequantize"({{.*}}) : (tensor<2x2x!quant.uniform<i8:f32, 1.000000e+00:8>>) -> tensor<2x2xf32>
// CHECK: return %[[DEQUANT]]

// -----

// Tests that the pattern doesn't match when the input quantized tensor's
// storage type is ui16. ui16 storage type is not compatible with
// `tfl.dequantize`.

// CHECK-LABEL: uniform_dequantize_op_ui16_storage_input
func.func @uniform_dequantize_op_ui16_storage_input(%arg: tensor<2x2x!quant.uniform<ui16:f32, 1.000000e+0:8>>) -> tensor<2x2xf32> {
  %0 = stablehlo.uniform_dequantize %arg : (tensor<2x2x!quant.uniform<ui16:f32, 1.000000e+0:8>>) -> tensor<2x2xf32>
  return %0 : tensor<2x2xf32>
}
// CHECK: stablehlo.uniform_dequantize
// CHECK-NOT: tfl.dequantize

// -----

// Tests that the pattern doesn't match when the input quantized tensor's
// storage type is i32. i32 storage type is not compatible with
// `tfl.dequantize`.

// CHECK-LABEL: uniform_dequantize_op_i32_storage_input
func.func @uniform_dequantize_op_i32_storage_input(%arg: tensor<2x2x!quant.uniform<i32:f32, 1.000000e+0:8>>) -> tensor<2x2xf32> {
  %0 = stablehlo.uniform_dequantize %arg : (tensor<2x2x!quant.uniform<i32:f32, 1.000000e+0:8>>) -> tensor<2x2xf32>
  return %0 : tensor<2x2xf32>
}
// CHECK: stablehlo.uniform_dequantize
// CHECK-NOT: tfl.dequantize

// -----

// Tests that the pattern doesn't match when the input quantized tensor's
// storage type is i32. i32 storage type is not compatible with
// `tfl.dequantize`.

// CHECK-LABEL: uniform_dequantize_op_return_f64
func.func @uniform_dequantize_op_return_f64(%arg: tensor<2x2x!quant.uniform<i8:f64, 1.000000e+0:8>>) -> tensor<2x2xf64> {
  %0 = stablehlo.uniform_dequantize %arg : (tensor<2x2x!quant.uniform<i8:f64, 1.000000e+0:8>>) -> tensor<2x2xf64>
  return %0 : tensor<2x2xf64>
}
// CHECK: stablehlo.uniform_dequantize
// CHECK-NOT: tfl.dequantize

// -----

// CHECK-LABEL: convolution_upstream_full_integer
func.func @convolution_upstream_full_integer(%arg0: tensor<1x3x3x4x!quant.uniform<i8:f32, 3.000000e+0:-100>>) -> tensor<1x3x3x2x!quant.uniform<i8:f32, 4.000000e+0>> {
  %0 = stablehlo.constant() {value = dense<3> : tensor<3x3x4x2xi8>} : () -> tensor<3x3x4x2x!quant.uniform<i8:f32:3, {2.000000e+2, 3.000000e+3}>>
  %1 = stablehlo.convolution(%arg0, %0) dim_numbers = [b, 0, 1, f]x[0, 1, i, o]->[b, 0, 1, f], window = {pad = [[1, 1], [1, 1]]} {batch_group_count = 1 : i64, feature_group_count = 1 : i64} : (tensor<1x3x3x4x!quant.uniform<i8:f32, 3.000000e+0:-100>>, tensor<3x3x4x2x!quant.uniform<i8:f32:3, {2.000000e+2, 3.000000e+3}>>) -> tensor<1x3x3x2x!quant.uniform<i8:f32, 4.000000e+0>>
  return %1 : tensor<1x3x3x2x!quant.uniform<i8:f32, 4.000000e+0>>
}
// CHECK-SAME: %[[ARG:.*]]: tensor<1x3x3x4x!quant.uniform<i8:f32, 3.000000e+00:-100>>
// CHECK-DAG: %[[CONST_0:.*]] = "tfl.pseudo_const"(){{.*}}dense<{{\[\[0, 0\], \[1, 1\], \[1, 1\], \[0, 0\]\]}}> : tensor<4x2xi32>
// Note that the quantized dimension is 0, and the shape has been transposed
// to (2, 3, 3, 4).
// CHECK-DAG: %[[QCONST_0:.*]] = "tfl.pseudo_qconst"() {qtype = tensor<2x3x3x4x!quant.uniform<i8<-127:127>:f32:0, {2.000000e+02,3.000000e+03}>>, value = dense<3> : tensor<2x3x3x4xi8>} : () -> tensor<2x3x3x4x!quant.uniform<i8<-127:127>:f32:0, {2.000000e+02,3.000000e+03}>>
// CHECK-DAG: %[[QCONST_1:.*]] = "tfl.pseudo_qconst"() {qtype = tensor<2x!quant.uniform<i32:f32:0, {6.000000e+02,9.000000e+03}>>, value = dense<0> : tensor<2xi32>} : () -> tensor<2x!quant.uniform<i32:f32:0, {6.000000e+02,9.000000e+03}>>
// Explicit tfl.pad op to reflect explicit padding attribute.
// CHECK: %[[PAD:.*]] = "tfl.pad"(%[[ARG]], %[[CONST_0]]) : (tensor<1x3x3x4x!quant.uniform<i8:f32, 3.000000e+00:-100>>, tensor<4x2xi32>) -> tensor<1x5x5x4x!quant.uniform<i8:f32, 3.000000e+00:-100>>
// CHECK: %[[CONV2D:.*]] = "tfl.conv_2d"(%[[PAD]], %[[QCONST_0]], %[[QCONST_1]]) {dilation_h_factor = 1 : i32, dilation_w_factor = 1 : i32, fused_activation_function = "NONE", padding = "VALID", stride_h = 1 : i32, stride_w = 1 : i32} : (tensor<1x5x5x4x!quant.uniform<i8:f32, 3.000000e+00:-100>>, tensor<2x3x3x4x!quant.uniform<i8<-127:127>:f32:0, {2.000000e+02,3.000000e+03}>>, tensor<2x!quant.uniform<i32:f32:0, {6.000000e+02,9.000000e+03}>>) -> tensor<1x3x3x2x!quant.uniform<i8:f32, 4.000000e+00>>
// CHECK: return %[[CONV2D]] : tensor<1x3x3x2x!quant.uniform<i8:f32, 4.000000e+00>>

// -----

// CHECK-LABEL: convolution_upstream_full_integer_non_const_filter
func.func @convolution_upstream_full_integer_non_const_filter(%arg0: tensor<1x3x3x4x!quant.uniform<i8:f32, 1.000000e+0:-100>>, %arg1: tensor<3x3x4x2x!quant.uniform<i8:f32:3, {2.000000e+2, 3.000000e+3}>>) -> tensor<1x3x3x2x!quant.uniform<i8:f32, 4.000000e+0>> {
  %0 = stablehlo.convolution(%arg0, %arg1) dim_numbers = [b, 0, 1, f]x[0, 1, i, o]->[b, 0, 1, f], window = {pad = [[1, 1], [1, 1]]} {batch_group_count = 1 : i64, feature_group_count = 1 : i64} : (tensor<1x3x3x4x!quant.uniform<i8:f32, 1.000000e+0:-100>>, tensor<3x3x4x2x!quant.uniform<i8:f32:3, {2.000000e+2, 3.000000e+3}>>) -> tensor<1x3x3x2x!quant.uniform<i8:f32, 4.000000e+0>>
  return %0 : tensor<1x3x3x2x!quant.uniform<i8:f32, 4.000000e+0>>
}
// CHECK-SAME: %[[ARG:.*]]: tensor<1x3x3x4x!quant.uniform<i8:f32, 1.000000e+00:-100>>

// Confirm that the `stablehlo.convolution` is not converted to `tfl.conv_2d`.
// CHECK: stablehlo.convolution
// CHECK-NOT: tfl.conv_2d

// -----

// Test that if the window padding contains values of 0, tfl.pad op is not
// created and the `padding` attribute is set as "VALID".

// CHECK-LABEL: convolution_upstream_full_integer_valid_padding
func.func @convolution_upstream_full_integer_valid_padding(%arg0: tensor<1x3x3x4x!quant.uniform<i8:f32, 1.000000e+0:-100>>) -> tensor<1x1x1x2x!quant.uniform<i8:f32, 4.000000e+0>> {
  %0 = stablehlo.constant() {value = dense<3> : tensor<3x3x4x2xi8>} : () -> tensor<3x3x4x2x!quant.uniform<i8:f32:3, {2.000000e+2, 3.000000e+3}>>
  %1 = stablehlo.convolution(%arg0, %0) dim_numbers = [b, 0, 1, f]x[0, 1, i, o]->[b, 0, 1, f], window = {pad = [[0, 0], [0, 0]]} {batch_group_count = 1 : i64, feature_group_count = 1 : i64} : (tensor<1x3x3x4x!quant.uniform<i8:f32, 1.000000e+0:-100>>, tensor<3x3x4x2x!quant.uniform<i8:f32:3, {2.000000e+2, 3.000000e+3}>>) -> tensor<1x1x1x2x!quant.uniform<i8:f32, 4.000000e+0>>
  return %1 : tensor<1x1x1x2x!quant.uniform<i8:f32, 4.000000e+0>>
}
// CHECK-SAME: %[[ARG:.*]]: tensor<1x3x3x4x!quant.uniform<i8:f32, 1.000000e+00:-100>>
// CHECK: %[[QCONST_0:.*]] = "tfl.pseudo_qconst"() {qtype = tensor<2x3x3x4x!quant.uniform<i8<-127:127>:f32:0, {2.000000e+02,3.000000e+03}>>, value = dense<3> : tensor<2x3x3x4xi8>} : () -> tensor<2x3x3x4x!quant.uniform<i8<-127:127>:f32:0, {2.000000e+02,3.000000e+03}>>
// CHECK: %[[QCONST_1:.*]] = "tfl.pseudo_qconst"() {qtype = tensor<2x!quant.uniform<i32:f32:0, {2.000000e+02,3.000000e+03}>>, value = dense<0> : tensor<2xi32>} : () -> tensor<2x!quant.uniform<i32:f32:0, {2.000000e+02,3.000000e+03}>>
// CHECK-NOT: tfl.pad
// CHECK: %[[CONV2D:.*]] = "tfl.conv_2d"(%[[ARG]], %[[QCONST_0]], %[[QCONST_1]]) {dilation_h_factor = 1 : i32, dilation_w_factor = 1 : i32, fused_activation_function = "NONE", padding = "VALID", stride_h = 1 : i32, stride_w = 1 : i32} : (tensor<1x3x3x4x!quant.uniform<i8:f32, 1.000000e+00:-100>>, tensor<2x3x3x4x!quant.uniform<i8<-127:127>:f32:0, {2.000000e+02,3.000000e+03}>>, tensor<2x!quant.uniform<i32:f32:0, {2.000000e+02,3.000000e+03}>>) -> tensor<1x1x1x2x!quant.uniform<i8:f32, 4.000000e+00>>
// CHECK: return %[[CONV2D]] : tensor<1x1x1x2x!quant.uniform<i8:f32, 4.000000e+00>>

// -----

// Test that if the window padding value is missing, tfl.pad op is not
// created and the `padding` attribute is set as "VALID".

// CHECK-LABEL: convolution_upstream_full_integer_valid_padding
func.func @convolution_upstream_full_integer_valid_padding(%arg0: tensor<1x3x3x4x!quant.uniform<i8:f32, 1.000000e+0:-100>>) -> tensor<1x1x1x2x!quant.uniform<i8:f32, 4.000000e+0>> {
  %0 = stablehlo.constant() {value = dense<3> : tensor<3x3x4x2xi8>} : () -> tensor<3x3x4x2x!quant.uniform<i8:f32:3, {2.000000e+2, 3.000000e+3}>>
  // The `window` attribute is empty.
  %1 = stablehlo.convolution(%arg0, %0) dim_numbers = [b, 0, 1, f]x[0, 1, i, o]->[b, 0, 1, f], window = {} {batch_group_count = 1 : i64, feature_group_count = 1 : i64} : (tensor<1x3x3x4x!quant.uniform<i8:f32, 1.000000e+0:-100>>, tensor<3x3x4x2x!quant.uniform<i8:f32:3, {2.000000e+2, 3.000000e+3}>>) -> tensor<1x1x1x2x!quant.uniform<i8:f32, 4.000000e+0>>
  return %1 : tensor<1x1x1x2x!quant.uniform<i8:f32, 4.000000e+0>>
}
// CHECK-SAME: %[[ARG:.*]]: tensor<1x3x3x4x!quant.uniform<i8:f32, 1.000000e+00:-100>>
// CHECK: %[[QCONST_0:.*]] = "tfl.pseudo_qconst"() {qtype = tensor<2x3x3x4x!quant.uniform<i8<-127:127>:f32:0, {2.000000e+02,3.000000e+03}>>, value = dense<3> : tensor<2x3x3x4xi8>} : () -> tensor<2x3x3x4x!quant.uniform<i8<-127:127>:f32:0, {2.000000e+02,3.000000e+03}>>
// CHECK: %[[QCONST_1:.*]] = "tfl.pseudo_qconst"() {qtype = tensor<2x!quant.uniform<i32:f32:0, {2.000000e+02,3.000000e+03}>>, value = dense<0> : tensor<2xi32>} : () -> tensor<2x!quant.uniform<i32:f32:0, {2.000000e+02,3.000000e+03}>>
// CHECK: %[[CONV2D:.*]] = "tfl.conv_2d"(%[[ARG]], %[[QCONST_0]], %[[QCONST_1]]) {dilation_h_factor = 1 : i32, dilation_w_factor = 1 : i32, fused_activation_function = "NONE", padding = "VALID", stride_h = 1 : i32, stride_w = 1 : i32} : (tensor<1x3x3x4x!quant.uniform<i8:f32, 1.000000e+00:-100>>, tensor<2x3x3x4x!quant.uniform<i8<-127:127>:f32:0, {2.000000e+02,3.000000e+03}>>, tensor<2x!quant.uniform<i32:f32:0, {2.000000e+02,3.000000e+03}>>) -> tensor<1x1x1x2x!quant.uniform<i8:f32, 4.000000e+00>>
// CHECK: return %[[CONV2D]] : tensor<1x1x1x2x!quant.uniform<i8:f32, 4.000000e+00>>

// -----

// Test that if the window stride value is explicitly set, the attribute
// value is transferred to tfl.conv_2d's stridw_h and stride_w values.

// CHECK-LABEL: convolution_upstream_full_integer_strides
func.func @convolution_upstream_full_integer_strides(%arg0: tensor<1x3x3x4x!quant.uniform<i8:f32, 1.000000e+0:-100>>) -> tensor<1x3x2x2x!quant.uniform<i8:f32, 4.000000e+0>> {
  %0 = stablehlo.constant() {value = dense<3> : tensor<3x3x4x2xi8>} : () -> tensor<3x3x4x2x!quant.uniform<i8:f32:3, {2.000000e+2, 3.000000e+3}>>
  // The stride value is explicitly set to [1, 2].
  %1 = stablehlo.convolution(%arg0, %0) dim_numbers = [b, 0, 1, f]x[0, 1, i, o]->[b, 0, 1, f], window = {stride = [1, 2], pad = [[1, 1], [1, 1]]} {batch_group_count = 1 : i64, feature_group_count = 1 : i64} : (tensor<1x3x3x4x!quant.uniform<i8:f32, 1.000000e+0:-100>>, tensor<3x3x4x2x!quant.uniform<i8:f32:3, {2.000000e+2, 3.000000e+3}>>) -> tensor<1x3x2x2x!quant.uniform<i8:f32, 4.000000e+0>>
  return %1 : tensor<1x3x2x2x!quant.uniform<i8:f32, 4.000000e+0>>
}
// CHECK-SAME: %[[ARG:.*]]: tensor<1x3x3x4x!quant.uniform<i8:f32, 1.000000e+00:-100>>
// CHECK-DAG: %[[CONST:.*]] = "tfl.pseudo_const"(){{.*}}dense<{{\[\[0, 0\], \[1, 1\], \[1, 1\], \[0, 0\]\]}}> : tensor<4x2xi32>
// CHECK-DAG: %[[QCONST_0:.*]] = "tfl.pseudo_qconst"() {qtype = tensor<2x3x3x4x!quant.uniform<i8<-127:127>:f32:0, {2.000000e+02,3.000000e+03}>>, value = dense<3> : tensor<2x3x3x4xi8>} : () -> tensor<2x3x3x4x!quant.uniform<i8<-127:127>:f32:0, {2.000000e+02,3.000000e+03}>>
// CHECK-DAG: %[[QCONST_1:.*]] = "tfl.pseudo_qconst"() {qtype = tensor<2x!quant.uniform<i32:f32:0, {2.000000e+02,3.000000e+03}>>, value = dense<0> : tensor<2xi32>} : () -> tensor<2x!quant.uniform<i32:f32:0, {2.000000e+02,3.000000e+03}>>
// CHECK: %[[PAD:.*]] = "tfl.pad"(%arg0, %[[CONST]]) : (tensor<1x3x3x4x!quant.uniform<i8:f32, 1.000000e+00:-100>>, tensor<4x2xi32>) -> tensor<1x5x5x4x!quant.uniform<i8:f32, 1.000000e+00:-100>>
// Tests that the stride_w is set to 2.
// CHECK: %[[CONV2D:.*]] = "tfl.conv_2d"(%[[PAD]], %[[QCONST_0]], %[[QCONST_1]]) {dilation_h_factor = 1 : i32, dilation_w_factor = 1 : i32, fused_activation_function = "NONE", padding = "VALID", stride_h = 1 : i32, stride_w = 2 : i32} : (tensor<1x5x5x4x!quant.uniform<i8:f32, 1.000000e+00:-100>>, tensor<2x3x3x4x!quant.uniform<i8<-127:127>:f32:0, {2.000000e+02,3.000000e+03}>>, tensor<2x!quant.uniform<i32:f32:0, {2.000000e+02,3.000000e+03}>>) -> tensor<1x3x2x2x!quant.uniform<i8:f32, 4.000000e+00>>
// CHECK: return %[[CONV2D]] : tensor<1x3x2x2x!quant.uniform<i8:f32, 4.000000e+00>>

// -----

// Test full integer quantized dot_general with asymmetric quantized input.

// CHECK-LABEL: dot_general_upstream_full_integer_asym_input
func.func @dot_general_upstream_full_integer_asym_input(%arg0: tensor<1x2x3x4x!quant.uniform<i8:f32, 1.000000e+0:-100>>) -> tensor<1x2x3x5x!quant.uniform<i8:f32, 4.000000e+0>> {
  %0 = stablehlo.constant() {value = dense<1> : tensor<1x2x4x5xi8>} : () -> tensor<1x2x4x5x!quant.uniform<i8:f32, 1.000000e+0>>
  %1 = "stablehlo.dot_general"(%arg0, %0) {
    dot_dimension_numbers = #stablehlo.dot<
      lhs_batching_dimensions = [0, 1],
      rhs_batching_dimensions = [0, 1],
      lhs_contracting_dimensions = [3],
      rhs_contracting_dimensions = [2]>,
      precision_config = [#stablehlo<precision DEFAULT>, #stablehlo<precision DEFAULT>]
  } : (tensor<1x2x3x4x!quant.uniform<i8:f32, 1.000000e+0:-100>>, tensor<1x2x4x5x!quant.uniform<i8:f32, 1.000000e+0>>) -> tensor<1x2x3x5x!quant.uniform<i8:f32, 4.000000e+0>>
  return %1 : tensor<1x2x3x5x!quant.uniform<i8:f32, 4.000000e+0>>
}
// CHECK-SAME: %[[ARG:.*]]: tensor<1x2x3x4x!quant.uniform<i8:f32, 1.000000e+00:-100>>
// CHECK: %[[QCONST_0:.*]] =  "tfl.pseudo_qconst"() {qtype = tensor<1x2x4x5x!quant.uniform<i8:f32, 1.000000e+00>>, value = dense<1> : tensor<1x2x4x5xi8>} : () -> tensor<1x2x4x5x!quant.uniform<i8:f32, 1.000000e+00>>
// CHECK: %[[BMM:.*]] = "tfl.batch_matmul"(%[[ARG]], %[[QCONST_0]]) {adj_x = false, adj_y = false} : (tensor<1x2x3x4x!quant.uniform<i8:f32, 1.000000e+00:-100>>, tensor<1x2x4x5x!quant.uniform<i8:f32, 1.000000e+00>>) -> tensor<1x2x3x5x!quant.uniform<i8:f32, 4.000000e+00>>

// -----

// Test full integer quantized dot_general with symmetric quantized input.

// CHECK-LABEL: dot_general_upstream_full_integer_sym_input
func.func @dot_general_upstream_full_integer_sym_input(%arg0: tensor<1x2x3x4x!quant.uniform<i8:f32, 1.000000e+0>>) -> tensor<1x2x3x5x!quant.uniform<i8:f32, 4.000000e+0>> {
  %0 = stablehlo.constant() {value = dense<1> : tensor<1x2x4x5xi8>} : () -> tensor<1x2x4x5x!quant.uniform<i8:f32, 1.000000e+0>>
  %1 = "stablehlo.dot_general"(%arg0, %0) {
    dot_dimension_numbers = #stablehlo.dot<
      lhs_batching_dimensions = [0, 1],
      rhs_batching_dimensions = [0, 1],
      lhs_contracting_dimensions = [3],
      rhs_contracting_dimensions = [2]
    >,
    precision_config = [#stablehlo<precision DEFAULT>, #stablehlo<precision DEFAULT>]
  } : (tensor<1x2x3x4x!quant.uniform<i8:f32, 1.000000e+0>>, tensor<1x2x4x5x!quant.uniform<i8:f32, 1.000000e+0>>) -> tensor<1x2x3x5x!quant.uniform<i8:f32, 4.000000e+0>>
  return %1 : tensor<1x2x3x5x!quant.uniform<i8:f32, 4.000000e+0>>
}

// CHECK-SAME: %[[ARG:.*]]: tensor<1x2x3x4x!quant.uniform<i8:f32, 1.000000e+00>>
// CHECK: %[[QCONST_0:.*]] =  "tfl.pseudo_qconst"()
// CHECK: "tfl.batch_matmul"(%[[ARG]], %[[QCONST_0]]) {adj_x = false, adj_y = false}

// -----

// Tests that the pattern does not match when the output tensor's storage
// type is i32. Currently we support qi8, qi8 -> qi8 only for GEMM ops that
// are quantized upstream. Other cases should be handled by regular quantized
// stablehlo.dot_general case.

// CHECK-LABEL: dot_general_upstream_full_integer_i32_output
func.func @dot_general_upstream_full_integer_i32_output(%arg0: tensor<1x2x3x4x!quant.uniform<i8:f32, 1.000000e+0>>) -> tensor<1x2x3x5x!quant.uniform<i32:f32, 4.000000e+0>> {
  %0 = stablehlo.constant() {value = dense<1> : tensor<1x2x4x5xi8>} : () -> tensor<1x2x4x5x!quant.uniform<i8:f32, 1.000000e+0>>
  %1 = "stablehlo.dot_general"(%arg0, %0) {
    dot_dimension_numbers = #stablehlo.dot<
      lhs_batching_dimensions = [0, 1],
      rhs_batching_dimensions = [0, 1],
      lhs_contracting_dimensions = [3],
      rhs_contracting_dimensions = [2]
    >,
    precision_config = [#stablehlo<precision DEFAULT>, #stablehlo<precision DEFAULT>]
  } : (tensor<1x2x3x4x!quant.uniform<i8:f32, 1.000000e+0>>, tensor<1x2x4x5x!quant.uniform<i8:f32, 1.000000e+0>>) -> tensor<1x2x3x5x!quant.uniform<i32:f32, 4.000000e+0>>
  return %1 : tensor<1x2x3x5x!quant.uniform<i32:f32, 4.000000e+0>>
}
// CHECK: stablehlo.dot_general
// CHECK-NOT: tfl.quantize

// -----

// Test full integer quantized dot_general with activation as RHS

// CHECK-LABEL: dot_general_upstream_full_integer_activation_rhs
func.func @dot_general_upstream_full_integer_activation_rhs(%arg0: tensor<1x2x3x4x!quant.uniform<i8:f32, 1.000000e+0>>, %arg1: tensor<1x2x4x5x!quant.uniform<i8:f32, 1.000000e+0>>) -> tensor<1x2x3x5x!quant.uniform<i8:f32, 4.000000e+0>> {
  %0 = "stablehlo.dot_general"(%arg0, %arg1) {
    dot_dimension_numbers = #stablehlo.dot<
      lhs_batching_dimensions = [0, 1],
      rhs_batching_dimensions = [0, 1],
      lhs_contracting_dimensions = [3],
      rhs_contracting_dimensions = [2]
    >,
    precision_config = [#stablehlo<precision DEFAULT>, #stablehlo<precision DEFAULT>]
  } : (tensor<1x2x3x4x!quant.uniform<i8:f32, 1.000000e+0>>, tensor<1x2x4x5x!quant.uniform<i8:f32, 1.000000e+0>>) -> tensor<1x2x3x5x!quant.uniform<i8:f32, 4.000000e+0>>
  return %0 : tensor<1x2x3x5x!quant.uniform<i8:f32, 4.000000e+0>>
}
// CHECK: "tfl.batch_matmul"(%arg0, %arg1) {adj_x = false, adj_y = false} : (tensor<1x2x3x4x!quant.uniform<i8:f32, 1.000000e+00>>, tensor<1x2x4x5x!quant.uniform<i8:f32, 1.000000e+00>>) -> tensor<1x2x3x5x!quant.uniform<i8:f32, 4.000000e+00>>

// -----

// Test full integer quantized dot_general with adj_x

// CHECK-LABEL: dot_general_upstream_full_integer_adj_x
func.func @dot_general_upstream_full_integer_adj_x(%arg0: tensor<1x2x4x3x!quant.uniform<i8:f32, 1.000000e+0>>) -> tensor<1x2x3x5x!quant.uniform<i8:f32, 4.000000e+0>> {
  %0 = stablehlo.constant() {value = dense<1> : tensor<1x2x4x5xi8>} : () -> tensor<1x2x4x5x!quant.uniform<i8:f32, 1.000000e+0>>
  %1 = "stablehlo.dot_general"(%arg0, %0) {
    dot_dimension_numbers = #stablehlo.dot<
      lhs_batching_dimensions = [0, 1],
      rhs_batching_dimensions = [0, 1],
      // implicit transpose of lhs
      lhs_contracting_dimensions = [2],
      rhs_contracting_dimensions = [2]
    >,
    precision_config = [#stablehlo<precision DEFAULT>, #stablehlo<precision DEFAULT>]
  } : (tensor<1x2x4x3x!quant.uniform<i8:f32, 1.000000e+0>>, tensor<1x2x4x5x!quant.uniform<i8:f32, 1.000000e+0>>) -> tensor<1x2x3x5x!quant.uniform<i8:f32, 4.000000e+0>>
  return %1 : tensor<1x2x3x5x!quant.uniform<i8:f32, 4.000000e+0>>
}

// CHECK-SAME: %[[ARG:.*]]: tensor<1x2x4x3x!quant.uniform<i8:f32, 1.000000e+00>>
// CHECK: %[[QCONST_0:.*]] =  "tfl.pseudo_qconst"() {qtype = tensor<1x2x4x5x!quant.uniform<i8:f32, 1.000000e+00>>, value = dense<1> : tensor<1x2x4x5xi8>} : () -> tensor<1x2x4x5x!quant.uniform<i8:f32, 1.000000e+00>>
// CHECK: "tfl.batch_matmul"(%[[ARG]], %[[QCONST_0]]) {adj_x = true, adj_y = false}

// -----

// Test full integer quantized dot_general with adj_y

// CHECK-LABEL: dot_general_upstream_full_integer_adj_y
func.func @dot_general_upstream_full_integer_adj_y(%arg0: tensor<1x2x3x4x!quant.uniform<i8:f32, 1.000000e+0>>) -> tensor<1x2x3x5x!quant.uniform<i8:f32, 4.000000e+0>> {
  %0 = stablehlo.constant() {value = dense<1> : tensor<1x2x5x4xi8>} : () -> tensor<1x2x5x4x!quant.uniform<i8:f32, 1.000000e+0>>
  %1 = "stablehlo.dot_general"(%arg0, %0) {
    dot_dimension_numbers = #stablehlo.dot<
      lhs_batching_dimensions = [0, 1],
      rhs_batching_dimensions = [0, 1],
      lhs_contracting_dimensions = [3],
      // implicit transpose of rhs
      rhs_contracting_dimensions = [3]
    >,
    precision_config = [#stablehlo<precision DEFAULT>, #stablehlo<precision DEFAULT>]
  } : (tensor<1x2x3x4x!quant.uniform<i8:f32, 1.000000e+0>>, tensor<1x2x5x4x!quant.uniform<i8:f32, 1.000000e+0>>) -> tensor<1x2x3x5x!quant.uniform<i8:f32, 4.000000e+0>>
  return %1 : tensor<1x2x3x5x!quant.uniform<i8:f32, 4.000000e+0>>
}

// CHECK-SAME: %[[ARG:.*]]: tensor<1x2x3x4x!quant.uniform<i8:f32, 1.000000e+00>>
// CHECK: %[[QCONST_0:.*]] =  "tfl.pseudo_qconst"() {qtype = tensor<1x2x5x4x!quant.uniform<i8:f32, 1.000000e+00>>, value = dense<1> : tensor<1x2x5x4xi8>} : () -> tensor<1x2x5x4x!quant.uniform<i8:f32, 1.000000e+00>>
// CHECK: "tfl.batch_matmul"(%[[ARG]], %[[QCONST_0]]) {adj_x = false, adj_y = true}

// -----

// Test full integer quantized dot_general with wrong batch dims

// CHECK-LABEL: dot_general_upstream_full_integer_too_many_batches
func.func @dot_general_upstream_full_integer_too_many_batches(%arg0: tensor<1x1x1x2x3x4x!quant.uniform<i8:f32, 1.000000e+0>>) -> tensor<1x1x1x2x3x5x!quant.uniform<i8:f32, 4.000000e+0>> {
  %0 = stablehlo.constant() {value = dense<1> : tensor<1x1x1x2x4x5xi8>} : () -> tensor<1x1x1x2x4x5x!quant.uniform<i8:f32, 1.000000e+0>>
  %1 = "stablehlo.dot_general"(%arg0, %0) {
    dot_dimension_numbers = #stablehlo.dot<
      lhs_batching_dimensions = [0, 1, 2, 3],
      rhs_batching_dimensions = [0, 1, 2, 3],
      lhs_contracting_dimensions = [5],
      rhs_contracting_dimensions = [4]
    >,
    precision_config = [#stablehlo<precision DEFAULT>, #stablehlo<precision DEFAULT>]
  } : (tensor<1x1x1x2x3x4x!quant.uniform<i8:f32, 1.000000e+0>>, tensor<1x1x1x2x4x5x!quant.uniform<i8:f32, 1.000000e+0>>) -> tensor<1x1x1x2x3x5x!quant.uniform<i8:f32, 4.000000e+0>>
  return %1 : tensor<1x1x1x2x3x5x!quant.uniform<i8:f32, 4.000000e+0>>
}
// Only support size(batching_dimensions) <= 3
// CHECK: stablehlo.dot_general
// CHECK-NOT: tfl.batch_matmul

// -----

// Test full integer quantized dot_general with too many contracting dimension

// CHECK-LABEL: dot_general_upstream_full_integer_too_many_contractions
func.func @dot_general_upstream_full_integer_too_many_contractions(%arg0: tensor<1x2x3x4x4x!quant.uniform<i8:f32, 1.000000e+0>>) -> tensor<1x2x3x5x!quant.uniform<i8:f32, 4.000000e+0>> {
  %0 = stablehlo.constant() {value = dense<1> : tensor<1x2x4x4x5xi8>} : () -> tensor<1x2x4x4x5x!quant.uniform<i8:f32, 1.000000e+0>>
  %1 = "stablehlo.dot_general"(%arg0, %0) {
    dot_dimension_numbers = #stablehlo.dot<
      lhs_batching_dimensions = [0, 1],
      rhs_batching_dimensions = [0, 1],
      lhs_contracting_dimensions = [3, 4],
      rhs_contracting_dimensions = [2, 3]
    >,
    precision_config = [#stablehlo<precision DEFAULT>, #stablehlo<precision DEFAULT>]
  } : (tensor<1x2x3x4x4x!quant.uniform<i8:f32, 1.000000e+0>>, tensor<1x2x4x4x5x!quant.uniform<i8:f32, 1.000000e+0>>) -> tensor<1x2x3x5x!quant.uniform<i8:f32, 4.000000e+0>>
  return %1 : tensor<1x2x3x5x!quant.uniform<i8:f32, 4.000000e+0>>
}
// Only support size(contracting_dimensions) == 1
// CHECK: stablehlo.dot_general
// CHECK-NOT: tfl.batch_matmul

// -----

// Test full integer quantized dot_general with unsupported contracting dim

// CHECK-LABEL: dot_general_upstream_full_integer_wrong_contracting
func.func @dot_general_upstream_full_integer_wrong_contracting(%arg0: tensor<1x2x3x4x!quant.uniform<i8:f32, 1.000000e+0>>) -> tensor<1x4x3x5x!quant.uniform<i8:f32, 4.000000e+0>> {
  %0 = stablehlo.constant() {value = dense<1> : tensor<1x2x4x5xi8>} : () -> tensor<1x2x4x5x!quant.uniform<i8:f32, 1.000000e+0>>
  %1 = "stablehlo.dot_general"(%arg0, %0) {
    dot_dimension_numbers = #stablehlo.dot<
      lhs_batching_dimensions = [0, 3],
      rhs_batching_dimensions = [0, 2],
      lhs_contracting_dimensions = [1],
      rhs_contracting_dimensions = [1]
    >,
    precision_config = [#stablehlo<precision DEFAULT>, #stablehlo<precision DEFAULT>]
  } : (tensor<1x2x3x4x!quant.uniform<i8:f32, 1.000000e+0>>, tensor<1x2x4x5x!quant.uniform<i8:f32, 1.000000e+0>>) -> tensor<1x4x3x5x!quant.uniform<i8:f32, 4.000000e+0>>
  return %1 : tensor<1x4x3x5x!quant.uniform<i8:f32, 4.000000e+0>>
}

// Contracting dimension must be the last two dimension
// CHECK: stablehlo.dot_general
// CHECK-NOT: tfl.batch_matmul

// -----

// Test full integer quantized dot_general with float operands

// CHECK-LABEL: dot_general_upstream_full_integer_float_operands
func.func @dot_general_upstream_full_integer_float_operands(%arg0: tensor<1x2x3x4xf32>, %arg1: tensor<1x2x4x5xf32>) -> tensor<1x2x3x5xf32> {
  %0 = "stablehlo.dot_general"(%arg0, %arg1) {
    dot_dimension_numbers = #stablehlo.dot<
      lhs_batching_dimensions = [0, 1],
      rhs_batching_dimensions = [0, 1],
      lhs_contracting_dimensions = [3],
      rhs_contracting_dimensions = [2]
    >,
    precision_config = [#stablehlo<precision DEFAULT>, #stablehlo<precision DEFAULT>]
  } : (tensor<1x2x3x4xf32>, tensor<1x2x4x5xf32>) -> tensor<1x2x3x5xf32>
  return %0 : tensor<1x2x3x5xf32>
}
// Do nothing for float operands
// CHECK: stablehlo.dot_general
// CHECK-NOT: tfl.batch_matmul

// -----

// Test full integer quantized dot_general with asymmetric weight (rhs).

// CHECK-LABEL: dot_general_upstream_full_integer_asym_weight
func.func @dot_general_upstream_full_integer_asym_weight(%arg0: tensor<1x2x3x4x!quant.uniform<i8:f32, 1.000000e+0:-100>>) -> tensor<1x2x3x5x!quant.uniform<i8:f32, 4.000000e+0>> {
  %0 = stablehlo.constant() {value = dense<1> : tensor<1x2x4x5xi8>} : () -> tensor<1x2x4x5x!quant.uniform<i8:f32, 1.000000e+0:5>>
  %1 = "stablehlo.dot_general"(%arg0, %0) {dot_dimension_numbers = #stablehlo.dot<lhs_batching_dimensions = [0, 1], rhs_batching_dimensions = [0, 1], lhs_contracting_dimensions = [3], rhs_contracting_dimensions = [2]>, precision_config = [#stablehlo<precision DEFAULT>, #stablehlo<precision DEFAULT>]} : (tensor<1x2x3x4x!quant.uniform<i8:f32, 1.000000e+0:-100>>, tensor<1x2x4x5x!quant.uniform<i8:f32, 1.000000e+0:5>>) -> tensor<1x2x3x5x!quant.uniform<i8:f32, 4.000000e+0>>
  return %1 : tensor<1x2x3x5x!quant.uniform<i8:f32, 4.000000e+0>>
}
// CHECK-SAME: %[[ARG:.*]]: tensor<1x2x3x4x!quant.uniform<i8:f32, 1.000000e+00:-100>>
// CHECK: %[[QCONST_0:.*]] =  "tfl.pseudo_qconst"() {qtype = tensor<1x2x4x5x!quant.uniform<i8:f32, 1.000000e+00:5>>, value = dense<1> : tensor<1x2x4x5xi8>} : () -> tensor<1x2x4x5x!quant.uniform<i8:f32, 1.000000e+00:5>>
// CHECK: %[[BMM:.*]] = "tfl.batch_matmul"(%[[ARG]], %[[QCONST_0]]) {adj_x = false, adj_y = false} : (tensor<1x2x3x4x!quant.uniform<i8:f32, 1.000000e+00:-100>>, tensor<1x2x4x5x!quant.uniform<i8:f32, 1.000000e+00:5>>) -> tensor<1x2x3x5x!quant.uniform<i8:f32, 4.000000e+00>>

// -----

// Test that when the weight tensor for `stablehlo.dot_general` is per-axis
// quantized, it is converted to `tfl.fully_connected` op.

// CHECK-LABEL: dot_general_upstream_full_integer_per_axis_quantized_filter
func.func @dot_general_upstream_full_integer_per_axis_quantized_filter(%arg0: tensor<1x3x!quant.uniform<i8:f32, 5.000000e+05:-100>>) -> tensor<1x2x!quant.uniform<i8:f32, 4.000000e+04:127>> {
  %0 = stablehlo.constant() {value = dense<1> : tensor<3x2xi8>} : () -> tensor<3x2x!quant.uniform<i8:f32:1,{2.000000e+02, 3.000000e+03}>>
  %1 = stablehlo.dot_general %arg0, %0, contracting_dims = [1] x [0] : (tensor<1x3x!quant.uniform<i8:f32, 5.000000e+05:-100>>, tensor<3x2x!quant.uniform<i8:f32:1,{2.000000e+02, 3.000000e+03}>>) -> tensor<1x2x!quant.uniform<i8:f32, 4.000000e+04:127>>
  return %1 : tensor<1x2x!quant.uniform<i8:f32, 4.000000e+04:127>>
}
// CHECK-SAME: %[[ARG_0:.*]]: tensor<1x3x!quant.uniform<i8:f32, 5.000000e+05:-100>>
// Weight tensor is transposed, as tfl.fully_connected accepts a [o, i] matrix.
// CHECK-DAG: %[[QCONST_0:.*]] = "tfl.pseudo_qconst"() {qtype = tensor<2x3x!quant.uniform<i8<-127:127>:f32:0, {2.000000e+02,3.000000e+03}>>, value = dense<1> : tensor<2x3xi8>} : () -> tensor<2x3x!quant.uniform<i8<-127:127>:f32:0, {2.000000e+02,3.000000e+03}>>
// CHECK-DAG: %[[QCONST_1:.*]] = "tfl.pseudo_qconst"() {qtype = tensor<2x!quant.uniform<i32:f32:0, {1.000000e+08,1.500000e+09}>>, value = dense<0> : tensor<2xi32>} : () -> tensor<2x!quant.uniform<i32:f32:0, {1.000000e+08,1.500000e+09}>>
// Bias tensor's scale is input scale * filter scale.
// CHECK: %[[FC:.*]] = "tfl.fully_connected"(%[[ARG_0]], %[[QCONST_0]], %[[QCONST_1]]) {fused_activation_function = "NONE", keep_num_dims = false, weights_format = "DEFAULT"} : (tensor<1x3x!quant.uniform<i8:f32, 5.000000e+05:-100>>, tensor<2x3x!quant.uniform<i8<-127:127>:f32:0, {2.000000e+02,3.000000e+03}>>, tensor<2x!quant.uniform<i32:f32:0, {1.000000e+08,1.500000e+09}>>) -> tensor<1x2x!quant.uniform<i8:f32, 4.000000e+04:127>>
// CHECK-NEXT: return %[[FC]] : tensor<1x2x!quant.uniform<i8:f32, 4.000000e+04:127>>

// -----

// Test that when the weight tensor for `stablehlo.dot_general` is per-axis
// quantized but has a batch dimension, it is not converted.

// CHECK-LABEL: dot_general_upstream_full_integer_per_axis_quantized_filter_with_batch_dim
func.func @dot_general_upstream_full_integer_per_axis_quantized_filter_with_batch_dim(%arg0: tensor<1x1x3x!quant.uniform<i8:f32, 5.000000e+05:-100>>) -> tensor<1x1x2x!quant.uniform<i8:f32, 4.000000e+04:127>> {
  %0 = stablehlo.constant() {value = dense<1> : tensor<1x3x2xi8>} : () -> tensor<1x3x2x!quant.uniform<i8:f32:1,{2.000000e+02, 3.000000e+03}>>
  %1 = stablehlo.dot_general %arg0, %0, batching_dims = [0] x [0], contracting_dims = [2] x [1] : (tensor<1x1x3x!quant.uniform<i8:f32, 5.000000e+05:-100>>, tensor<1x3x2x!quant.uniform<i8:f32:1,{2.000000e+02, 3.000000e+03}>>) -> tensor<1x1x2x!quant.uniform<i8:f32, 4.000000e+04:127>>
  return %1 : tensor<1x1x2x!quant.uniform<i8:f32, 4.000000e+04:127>>
}
// Nothing changes.
// CHECK: stablehlo.dot_general
// CHECK-NOT: tfl.fully_connected
// CHECK-NOT: tfl.batch_matmul

// -----

// Test that when the weight tensor for `stablehlo.dot_general` is per-axis
// quantized but has a batch dim > 1, it is not converted.

// CHECK-LABEL: dot_general_upstream_full_integer_per_axis_quantized_filter_multibatch
func.func @dot_general_upstream_full_integer_per_axis_quantized_filter_multibatch(%arg0: tensor<3x1x3x!quant.uniform<i8:f32, 5.000000e+05:-100>>) -> tensor<3x1x2x!quant.uniform<i8:f32, 4.000000e+04:127>> {
  %0 = stablehlo.constant() {value = dense<1> : tensor<3x3x2xi8>} : () -> tensor<3x3x2x!quant.uniform<i8:f32:1,{2.000000e+02, 3.000000e+03}>>
  %1 = stablehlo.dot_general %arg0, %0, batching_dims = [0] x [0], contracting_dims = [2] x [1] : (tensor<3x1x3x!quant.uniform<i8:f32, 5.000000e+05:-100>>, tensor<3x3x2x!quant.uniform<i8:f32:1,{2.000000e+02, 3.000000e+03}>>) -> tensor<3x1x2x!quant.uniform<i8:f32, 4.000000e+04:127>>
  return %1 : tensor<3x1x2x!quant.uniform<i8:f32, 4.000000e+04:127>>
}
// Nothing changes.
// CHECK: stablehlo.dot_general
// CHECK-NOT: tfl.fully_connected
// CHECK-NOT: tfl.batch_matmul

// -----

// Test that when the weight tensor for `stablehlo.dot_general` is per-axis
// quantized but has more than one contracting dimension, it is not converted.

// CHECK-LABEL: dot_general_upstream_full_integer_per_axis_quantized_filter_with_multiple_contracting_dims
func.func @dot_general_upstream_full_integer_per_axis_quantized_filter_with_multiple_contracting_dims(%arg0: tensor<1x2x3x!quant.uniform<i8:f32, 5.000000e+05:-100>>) -> tensor<1x1x!quant.uniform<i8:f32, 4.000000e+04:127>> {
  %0 = stablehlo.constant() {value = dense<1> : tensor<1x3x2xi8>} : () -> tensor<1x3x2x!quant.uniform<i8:f32:1,{2.000000e+02, 3.000000e+03}>>
  %1 = stablehlo.dot_general %arg0, %0, contracting_dims = [1, 2] x [2, 1] : (tensor<1x2x3x!quant.uniform<i8:f32, 5.000000e+05:-100>>, tensor<1x3x2x!quant.uniform<i8:f32:1,{2.000000e+02, 3.000000e+03}>>) -> tensor<1x1x!quant.uniform<i8:f32, 4.000000e+04:127>>
  return %1 : tensor<1x1x!quant.uniform<i8:f32, 4.000000e+04:127>>
}
// Nothing changes.
// CHECK: stablehlo.dot_general
// CHECK-NOT: tfl.fully_connected
// CHECK-NOT: tfl.batch_matmul

// -----

// Test that a simple per-tensor quantized stablehlo.dot_general is properly
// fused with a subsequent requantize (qi32->qi8) op then legalized.
// Supports the following format: (lhs: qi8, rhs: qi8) -> result: qi32

// CHECK-LABEL: dot_general_full_integer
// CHECK-SAME: (%[[ARG_1:.*]]: tensor<1x1024x!quant.uniform<i8:f32, {{.*}}>
 func.func @dot_general_full_integer(%arg0: tensor<1x1024x!quant.uniform<i8:f32, 1.000000e+0:0>> {tf_saved_model.index_path = ["input_tensor"]}) -> (tensor<1x3xf32> {tf_saved_model.index_path = ["output"]}) {
    %0 = stablehlo.constant() {value = dense<1> : tensor<1024x3xi8>} : () -> tensor<1024x3x!quant.uniform<i8<-127:127>:f32, 2.000000e+0:0>>
    %1 = stablehlo.dot_general %arg0, %0, contracting_dims = [1] x [0] : (tensor<1x1024x!quant.uniform<i8:f32, 1.000000e+0:0>>, tensor<1024x3x!quant.uniform<i8<-127:127>:f32, 2.000000e+0:0>>) -> tensor<1x3x!quant.uniform<i32:f32, 4.000000e+0:-127>>
    %2 = stablehlo.uniform_quantize %1 : (tensor<1x3x!quant.uniform<i32:f32, 4.000000e+0:-127>>) -> tensor<1x3x!quant.uniform<i8:f32, 2.000000e+00:-127>>
    %3 = stablehlo.uniform_dequantize %2 : (tensor<1x3x!quant.uniform<i8:f32, 2.000000e+00:-127>>) -> tensor<1x3xf32>
    return %3 : tensor<1x3xf32>
  }

// CHECK-NOT: stablehlo.dot_general
// CHECK: %[[QCONST_0:.*]] =  "tfl.pseudo_qconst"() {qtype = tensor<3x1024x!quant.uniform<i8<-127:127>:f32, 2.000000e+00>>, value = dense<1> : tensor<3x1024xi8>} : () -> tensor<3x1024x!quant.uniform<i8<-127:127>:f32, 2.000000e+00>>
// CHECK: %[[QCONST_1:.*]] =  "tfl.pseudo_qconst"() {qtype = tensor<3x!quant.uniform<i32:f32, 2.000000e+00>>, value = dense<0> : tensor<3xi32>} : () -> tensor<3x!quant.uniform<i32:f32, 2.000000e+00>>
// CHECK: "tfl.fully_connected"(%[[ARG_1]], %[[QCONST_0]], %[[QCONST_1]])  {fused_activation_function = "NONE", keep_num_dims = false, weights_format = "DEFAULT"} : (tensor<1x1024x!quant.uniform<i8:f32, 1.000000e+00>>, tensor<3x1024x!quant.uniform<i8<-127:127>:f32, 2.000000e+00>>, tensor<3x!quant.uniform<i32:f32, 2.000000e+00>>) -> tensor<1x3x!quant.uniform<i8:f32, 2.000000e+00:-127>>
// CHECK-NOT: tfl.batch_matmul

// -----

// Test that a `stablehlo.dot_general` with an i32 output remains unchanged when
// it is not followed by a requantization (`stablehlo.quantize`).

// CHECK-LABEL: dot_general_no_requantize
func.func @dot_general_no_requantize(%arg0: tensor<1x4xf32>) -> tensor<1x3xf32> {
  %0 = stablehlo.constant() {value = dense<5> : tensor<4x3xi8>} : () -> tensor<4x3x!quant.uniform<i8<-127:127>:f32, 2.000000e+00>>
  %1 = stablehlo.uniform_quantize %arg0 : (tensor<1x4xf32>) -> tensor<1x4x!quant.uniform<i8:f32, 3.000000e+00>>
  %2 = stablehlo.dot_general %1, %0, contracting_dims = [1] x [0] : (tensor<1x4x!quant.uniform<i8:f32, 3.000000e+00>>, tensor<4x3x!quant.uniform<i8<-127:127>:f32, 2.000000e+00>>) -> tensor<1x3x!quant.uniform<i32:f32, 5.000000e+00>>
  %3 = stablehlo.uniform_dequantize %2 : (tensor<1x3x!quant.uniform<i32:f32, 5.000000e+00>>) -> tensor<1x3xf32>
  return %3 : tensor<1x3xf32>
}
// CHECK: "tfl.quantize"
// CHECK: stablehlo.dot_general
// CHECK-NOT: tfl.fully_connected
// CHECK-NOT: tfl.batch_matmul
// CHECK: stablehlo.uniform_dequantize

// -----

// Test that a quantized stablehlo.transpose is converted to tfl.transpose.

// CHECK-LABEL: transpose
// CHECK-SAME: %[[ARG0:.*]]: tensor<2x3x4x!quant.uniform<i8:f32, 2.000000e+00:-1>>
func.func @transpose(
    %arg0: tensor<2x3x4x!quant.uniform<i8:f32, 2.000000e+00:-1>>
  ) -> tensor<4x3x2x!quant.uniform<i8:f32, 2.000000e+00:-1>> {
  %0 = stablehlo.transpose %arg0, dims = [2, 1, 0] : (tensor<2x3x4x!quant.uniform<i8:f32, 2.000000e+00:-1>>) -> tensor<4x3x2x!quant.uniform<i8:f32, 2.000000e+00:-1>>
  return %0 : tensor<4x3x2x!quant.uniform<i8:f32, 2.000000e+00:-1>>
}

// CHECK-NOT: stablehlo.transpose
// CHECK: %[[CST:.*]] = arith.constant dense<[2, 1, 0]> : tensor<3xi32>
// CHECK: %[[TRANSPOSE:.*]] = "tfl.transpose"(%[[ARG0]], %[[CST]]) : (tensor<2x3x4x!quant.uniform<i8:f32, 2.000000e+00:-1>>, tensor<3xi32>) -> tensor<4x3x2x!quant.uniform<i8:f32, 2.000000e+00:-1>>
// CHECK: return %[[TRANSPOSE]]

// -----

// Test that a float stablehlo.transpose is not converted to tfl.transpose.

// CHECK-LABEL: float_transpose
func.func @float_transpose(%arg0: tensor<2x3x4xf32>) -> tensor<4x3x2xf32> {
  %0 = stablehlo.transpose %arg0, dims = [2, 1, 0] : (tensor<2x3x4xf32>) -> tensor<4x3x2xf32>
  return %0 : tensor<4x3x2xf32>
}

// CHECK-NOT: tfl.transpose
// CHECK: stablehlo.transpose

// -----

// Test that a quantized stablehlo.reshape is converted to tfl.reshape.

// CHECK-LABEL: reshape
// CHECK-SAME: %[[ARG0:.*]]: tensor<2x3x4x!quant.uniform<i8:f32, 2.000000e+00:-1>>
func.func @reshape(
    %arg0: tensor<2x3x4x!quant.uniform<i8:f32, 2.000000e+00:-1>>
  ) -> tensor<6x4x!quant.uniform<i8:f32, 2.000000e+00:-1>> {
  %0 = stablehlo.reshape %arg0 : (tensor<2x3x4x!quant.uniform<i8:f32, 2.000000e+00:-1>>) -> tensor<6x4x!quant.uniform<i8:f32, 2.000000e+00:-1>>
  return %0 : tensor<6x4x!quant.uniform<i8:f32, 2.000000e+00:-1>>
}

// CHECK-NOT: stablehlo.reshape
// CHECK: %[[CST:.*]] = arith.constant dense<[6, 4]> : tensor<2xi32>
// CHECK: %[[RESHAPE:.*]] = "tfl.reshape"(%[[ARG0]], %[[CST]]) : (tensor<2x3x4x!quant.uniform<i8:f32, 2.000000e+00:-1>>, tensor<2xi32>) -> tensor<6x4x!quant.uniform<i8:f32, 2.000000e+00:-1>>
// CHECK: return %[[RESHAPE]]

// -----

// Test that a float stablehlo.reshape is not converted to tfl.reshape.

// CHECK-LABEL: float_reshape
func.func @float_reshape(%arg0: tensor<2x3x4xf32>) -> tensor<6x4xf32> {
  %0 = stablehlo.reshape %arg0 : (tensor<2x3x4xf32>) -> tensor<6x4xf32>
  return %0 : tensor<6x4xf32>
}

// CHECK-NOT: tfl.reshape
// CHECK: stablehlo.reshape

// -----

// Test that a quantized stablehlo.select is converted to tfl.select_v2.

// CHECK-LABEL: select
// CHECK-SAME: %[[ARG0:.*]]: tensor<1x3xi1>, %[[ARG1:.*]]: tensor<1x3x!quant.uniform<i8:f32, 2.000000e+00:-1>>, %[[ARG2:.*]]: tensor<1x3x!quant.uniform<i8:f32, 2.000000e+00:-1>>
func.func @select(
    %arg0: tensor<1x3xi1>,
    %arg1: tensor<1x3x!quant.uniform<i8:f32, 2.000000e+00:-1>>,
    %arg2: tensor<1x3x!quant.uniform<i8:f32, 2.000000e+00:-1>>
  ) -> tensor<1x3x!quant.uniform<i8:f32, 2.000000e+00:-1>> {
  %0 = "stablehlo.select"(%arg0, %arg1, %arg2) : (
    tensor<1x3xi1>,
    tensor<1x3x!quant.uniform<i8:f32, 2.000000e+00:-1>>,
    tensor<1x3x!quant.uniform<i8:f32, 2.000000e+00:-1>>
  ) -> tensor<1x3x!quant.uniform<i8:f32, 2.000000e+00:-1>>
  return %0 : tensor<1x3x!quant.uniform<i8:f32, 2.000000e+00:-1>>
}

// CHECK-NOT: stablehlo.select
// CHECK: %[[SELECT:.*]] = "tfl.select_v2"(%[[ARG0]], %[[ARG1]], %[[ARG2]]) : (tensor<1x3xi1>, tensor<1x3x!quant.uniform<i8:f32, 2.000000e+00:-1>>, tensor<1x3x!quant.uniform<i8:f32, 2.000000e+00:-1>>) -> tensor<1x3x!quant.uniform<i8:f32, 2.000000e+00:-1>>
// CHECK: return %[[SELECT]]

// -----

// Test that a float stablehlo.select is not converted to tfl.select_v2.


// CHECK-LABEL: float_select
func.func @float_select(%arg0: tensor<1x3xi1>, %arg1: tensor<1x3xf32>, %arg2: tensor<1x3xf32>) -> tensor<1x3xf32> {
  %0 = "stablehlo.select"(%arg0, %arg1, %arg2) : (tensor<1x3xi1>, tensor<1x3xf32>, tensor<1x3xf32>) -> tensor<1x3xf32>
  return %0 : tensor<1x3xf32>
}

// CHECK-NOT: tfl.select_v2
// CHECK: stablehlo.select

// -----

// Test that a quantized stablehlo.concatenate is converted to tfl.concatenation.

// CHECK-LABEL: concatenate
// CHECK-SAME: %[[ARG0:.*]]: tensor<3x2x!quant.uniform<i8:f32, 2.000000e+00:-1>>, %[[ARG1:.*]]: tensor<1x2x!quant.uniform<i8:f32, 2.000000e+00:-1>>
func.func @concatenate(
    %arg0: tensor<3x2x!quant.uniform<i8:f32, 2.000000e+00:-1>>,
    %arg1: tensor<1x2x!quant.uniform<i8:f32, 2.000000e+00:-1>>
  ) -> tensor<4x2x!quant.uniform<i8:f32, 2.000000e+00:-1>> {
  %0 = "stablehlo.concatenate"(%arg0, %arg1) {dimension = 0 : i64} : (
    tensor<3x2x!quant.uniform<i8:f32, 2.000000e+00:-1>>,
    tensor<1x2x!quant.uniform<i8:f32, 2.000000e+00:-1>>
  ) -> tensor<4x2x!quant.uniform<i8:f32, 2.000000e+00:-1>>
  return %0 : tensor<4x2x!quant.uniform<i8:f32, 2.000000e+00:-1>>
}

// CHECK-NOT: stablehlo.concatenate
// CHECK: %[[CONCAT:.*]] = "tfl.concatenation"(%arg0, %arg1) {axis = 0 : i32, fused_activation_function = "NONE"} : (tensor<3x2x!quant.uniform<i8:f32, 2.000000e+00:-1>>, tensor<1x2x!quant.uniform<i8:f32, 2.000000e+00:-1>>) -> tensor<4x2x!quant.uniform<i8:f32, 2.000000e+00:-1>>
// CHECK: return %[[CONCAT]]

// -----

// Test that a float stablehlo.concatenate is not converted to tfl.concatenation.

// CHECK-LABEL: float_concatenate
func.func @float_concatenate(%arg0: tensor<3x2xf32>, %arg1: tensor<1x2xf32>) -> tensor<4x2xf32> {
  %0 = "stablehlo.concatenate"(%arg0, %arg1) {dimension = 0 : i64} : (tensor<3x2xf32>, tensor<1x2xf32>) -> tensor<4x2xf32>
  return %0 : tensor<4x2xf32>
}

// CHECK-NOT: tfl.concatenation
// CHECK: stablehlo.concatenate

// -----

// Test that a quantized stablehlo.pad without interior padding is converted to
// tfl.padv2.

// CHECK-LABEL: pad_without_interior_padding
// CHECK-SAME: %[[ARG0:.*]]: tensor<2x3x!quant.uniform<i8:f32, 2.000000e+00:-1>>
// CHECK-SAME: %[[ARG1:.*]]: tensor<!quant.uniform<i8:f32, 2.000000e+00:-1>>
func.func @pad_without_interior_padding(
    %arg0: tensor<2x3x!quant.uniform<i8:f32, 2.000000e+00:-1>>,
    %arg1: tensor<!quant.uniform<i8:f32, 2.000000e+00:-1>>
  ) -> tensor<4x5x!quant.uniform<i8:f32, 2.000000e+00:-1>> {
  %0 = stablehlo.pad %arg0, %arg1, low = [0, 1], high = [2, 1], interior = [0, 0] : (
    tensor<2x3x!quant.uniform<i8:f32, 2.000000e+00:-1>>,
    tensor<!quant.uniform<i8:f32, 2.000000e+00:-1>>
  ) -> tensor<4x5x!quant.uniform<i8:f32, 2.000000e+00:-1>>
  return %0 : tensor<4x5x!quant.uniform<i8:f32, 2.000000e+00:-1>>
}

// CHECK: %[[PADDING:.*]] = arith.constant
// CHECK{LITERAL}: dense<[[0, 2], [1, 1]]> : tensor<2x2xi32>
// CHECK: %[[PAD:.*]] = "tfl.padv2"(%[[ARG0]], %[[PADDING]], %[[ARG1]]) : (tensor<2x3x!quant.uniform<i8:f32, 2.000000e+00:-1>>, tensor<2x2xi32>, tensor<!quant.uniform<i8:f32, 2.000000e+00:-1>>) -> tensor<4x5x!quant.uniform<i8:f32, 2.000000e+00:-1>>
// CHECK: return %[[PAD]]

// -----

// Test that a quantized stablehlo.pad with interior padding is converted to
// tfl.dilate and tfl.padv2.

// CHECK-LABEL: pad_with_interior_padding
// CHECK-SAME: %[[ARG0:.*]]: tensor<2x3x!quant.uniform<i8:f32, 2.000000e+00:-1>>
// CHECK-SAME: %[[ARG1:.*]]: tensor<!quant.uniform<i8:f32, 2.000000e+00:-1>>
func.func @pad_with_interior_padding(
    %arg0: tensor<2x3x!quant.uniform<i8:f32, 2.000000e+00:-1>>,
    %arg1: tensor<!quant.uniform<i8:f32, 2.000000e+00:-1>>
  ) -> tensor<5x9x!quant.uniform<i8:f32, 2.000000e+00:-1>> {
  %0 = stablehlo.pad %arg0, %arg1, low = [0, 1], high = [2, 1], interior = [1, 2] : (
    tensor<2x3x!quant.uniform<i8:f32, 2.000000e+00:-1>>,
    tensor<!quant.uniform<i8:f32, 2.000000e+00:-1>>
  ) -> tensor<5x9x!quant.uniform<i8:f32, 2.000000e+00:-1>>
  return %0 : tensor<5x9x!quant.uniform<i8:f32, 2.000000e+00:-1>>
}

// CHECK: %[[PADDING:.*]] = arith.constant
// CHECK{LITERAL}: dense<[[0, 2], [1, 1]]> : tensor<2x2xi32>
// CHECK: %[[INTERIOR:.*]] = arith.constant
// CHECK{LITERAL}: dense<[1, 2]> : tensor<2xi32>
// CHECK: %[[DILATE:.*]] = "tfl.dilate"(%[[ARG0]], %[[INTERIOR]], %[[ARG1]]) : (tensor<2x3x!quant.uniform<i8:f32, 2.000000e+00:-1>>, tensor<2xi32>, tensor<!quant.uniform<i8:f32, 2.000000e+00:-1>>) -> tensor<3x7x!quant.uniform<i8:f32, 2.000000e+00:-1>>
// CHECK: %[[PAD:.*]] = "tfl.padv2"(%[[DILATE]], %[[PADDING]], %[[ARG1]]) : (tensor<3x7x!quant.uniform<i8:f32, 2.000000e+00:-1>>, tensor<2x2xi32>, tensor<!quant.uniform<i8:f32, 2.000000e+00:-1>>) -> tensor<5x9x!quant.uniform<i8:f32, 2.000000e+00:-1>>
// CHECK: return %[[PAD]]

// -----

// Test that a float stablehlo.pad is not converted to tfl.padv2.

// CHECK-LABEL: float_pad
func.func @float_pad(%arg0: tensor<2x3xf32>, %arg1: tensor<f32>) -> tensor<4x5xf32> {
  %0 = stablehlo.pad %arg0, %arg1, low = [0, 1], high = [2, 1], interior = [0, 0] : (tensor<2x3xf32>, tensor<f32>) -> tensor<4x5xf32>
  return %0 : tensor<4x5xf32>
}

// CHECK-NOT: tfl.padv2
// CHECK: stablehlo.pad

// -----

// Test that a quantized stablehlo.slice is converted to tfl.slice when stride
// is 1.

// CHECK-LABEL: slice
// CHECK-SAME: %[[ARG0:.*]]: tensor<3x4x!quant.uniform<i8:f32, 2.000000e+00:-1>>
func.func @slice(
    %arg0: tensor<3x4x!quant.uniform<i8:f32, 2.000000e+00:-1>>
  ) -> tensor<2x2x!quant.uniform<i8:f32, 2.000000e+00:-1>> {
  %0 = "stablehlo.slice"(%arg0) {
    start_indices = array<i64: 1, 2>,
    limit_indices = array<i64: 3, 4>,
    strides = array<i64: 1, 1>
  } : (
    tensor<3x4x!quant.uniform<i8:f32, 2.000000e+00:-1>>
  ) -> tensor<2x2x!quant.uniform<i8:f32, 2.000000e+00:-1>>
  return %0 : tensor<2x2x!quant.uniform<i8:f32, 2.000000e+00:-1>>
}

// CHECK-DAG: %[[START:.*]] = arith.constant dense<{{\[1, 2\]}}> : tensor<2xi32>
// CHECK-DAG: %[[SIZE:.*]] = arith.constant dense<2> : tensor<2xi32>
// CHECK: %[[SLICE:.*]] = "tfl.slice"(%[[ARG0]], %[[START]], %[[SIZE]]) : (tensor<3x4x!quant.uniform<i8:f32, 2.000000e+00:-1>>, tensor<2xi32>, tensor<2xi32>) -> tensor<2x2x!quant.uniform<i8:f32, 2.000000e+00:-1>>
// CHECK: return %[[SLICE]]

// -----

// Test that a quantized stablehlo.slice is converted to tfl.strided_slice when
// stride is not 1.

// CHECK-LABEL: strided_slice
// CHECK-SAME: %[[ARG0:.*]]: tensor<3x6x!quant.uniform<i8:f32, 2.000000e+00:-1>>
func.func @strided_slice(
    %arg0: tensor<3x6x!quant.uniform<i8:f32, 2.000000e+00:-1>>
  ) -> tensor<2x2x!quant.uniform<i8:f32, 2.000000e+00:-1>> {
  %0 = "stablehlo.slice"(%arg0) {
    start_indices = array<i64: 0, 2>,
    limit_indices = array<i64: 3, 6>,
    strides = array<i64: 2, 3>
  } : (
    tensor<3x6x!quant.uniform<i8:f32, 2.000000e+00:-1>>
  ) -> tensor<2x2x!quant.uniform<i8:f32, 2.000000e+00:-1>>
  return %0 : tensor<2x2x!quant.uniform<i8:f32, 2.000000e+00:-1>>
}

// CHECK: %[[START:.*]] = arith.constant
// CHECK{LITERAL}: dense<[0, 2]> : tensor<2xi32>
// CHECK: %[[SIZE:.*]] = arith.constant
// CHECK{LITERAL}: dense<[3, 4]> : tensor<2xi32>
// CHECK: %[[STRIDE:.*]] = arith.constant
// CHECK{LITERAL}: dense<[2, 3]> : tensor<2xi32>
// CHECK: %[[SLICE:.*]] = "tfl.strided_slice"(%[[ARG0]], %[[START]], %[[SIZE]], %[[STRIDE]]) {begin_mask = 0 : i32, ellipsis_mask = 0 : i32, end_mask = 0 : i32, new_axis_mask = 0 : i32, offset = false, shrink_axis_mask = 0 : i32} : (tensor<3x6x!quant.uniform<i8:f32, 2.000000e+00:-1>>, tensor<2xi32>, tensor<2xi32>, tensor<2xi32>) -> tensor<2x2x!quant.uniform<i8:f32, 2.000000e+00:-1>>
// CHECK: return %[[SLICE]]

// -----

// Test that a float stablehlo.slice is not converted to tfl.slice.

// CHECK-LABEL: float_slice
func.func @float_slice(%arg0: tensor<3x4xf32>) -> tensor<2x2xf32> {
  %0 = "stablehlo.slice"(%arg0) {
    start_indices = array<i64: 1, 2>,
    limit_indices = array<i64: 3, 4>,
    strides = array<i64: 1, 1>
  } : (tensor<3x4xf32>) -> tensor<2x2xf32>
  return %0 : tensor<2x2xf32>
}

// CHECK-NOT: tfl.slice
// CHECK-NOT: tfl.strided_slice
// CHECK: stablehlo.slice

// -----

// Test that a quantized stablehlo.broadcast_in_dim is converted to
// tfl.broadcast_to.

// CHECK-LABEL: broadcast_in_dim
// CHECK-SAME: %[[ARG0:.*]]: tensor<1x2x!quant.uniform<i8:f32, 2.000000e+00:3>>
func.func @broadcast_in_dim(
    %arg0: tensor<1x2x!quant.uniform<i8:f32, 2.000000e+00:3>>
  ) -> tensor<3x2x!quant.uniform<i8:f32, 2.000000e+00:3>> {
  %0 = "stablehlo.broadcast_in_dim"(%arg0) {
    broadcast_dimensions = array<i64: 0, 1>
  } : (tensor<1x2x!quant.uniform<i8:f32, 2.000000e+00:3>>) -> tensor<3x2x!quant.uniform<i8:f32, 2.000000e+00:3>>
  return %0 : tensor<3x2x!quant.uniform<i8:f32, 2.000000e+00:3>>
}

// CHECK: %[[SHAPE:.*]] = arith.constant
// CHECK{LITERAL}: dense<[3, 2]> : tensor<2xi32>
// CHECK: %[[BROADCAST:.*]] = "tfl.broadcast_to"(%[[ARG0]], %[[SHAPE]]) : (tensor<1x2x!quant.uniform<i8:f32, 2.000000e+00:3>>, tensor<2xi32>) -> tensor<3x2x!quant.uniform<i8:f32, 2.000000e+00:3>>
// CHECK: return %[[BROADCAST]]

// -----

// Test that a quantized stablehlo.broadcast_in_dim is converted to
// tfl.transpose and tfl.broadcast_to when broadcast_dimensions is not in
// ascending order.

// CHECK-LABEL: broadcast_in_dim_with_transpose
// CHECK-SAME: %[[ARG0:.*]]: tensor<1x2x!quant.uniform<i8:f32, 2.000000e+00:3>>
func.func @broadcast_in_dim_with_transpose(
    %arg0: tensor<1x2x!quant.uniform<i8:f32, 2.000000e+00:3>>
  ) -> tensor<2x3x!quant.uniform<i8:f32, 2.000000e+00:3>> {
  %0 = "stablehlo.broadcast_in_dim"(%arg0) {
    broadcast_dimensions = array<i64: 1, 0>
  } : (tensor<1x2x!quant.uniform<i8:f32, 2.000000e+00:3>>) -> tensor<2x3x!quant.uniform<i8:f32, 2.000000e+00:3>>
  return %0 : tensor<2x3x!quant.uniform<i8:f32, 2.000000e+00:3>>
}

// CHECK: %[[BROADCAST_DIM:.*]] = arith.constant
// CHECK{LITERAL}: dense<[2, 3]> : tensor<2xi32>
// CHECK: %[[PERM:.*]] = arith.constant
// CHECK{LITERAL}: dense<[1, 0]> : tensor<2xi32>
// CHECK: %[[TRANSPOSE:.*]] = "tfl.transpose"(%[[ARG0]], %[[PERM]]) : (tensor<1x2x!quant.uniform<i8:f32, 2.000000e+00:3>>, tensor<2xi32>) -> tensor<2x1x!quant.uniform<i8:f32, 2.000000e+00:3>>
// CHECK: %[[BROADCAST:.*]] = "tfl.broadcast_to"(%[[TRANSPOSE]], %[[BROADCAST_DIM]]) : (tensor<2x1x!quant.uniform<i8:f32, 2.000000e+00:3>>, tensor<2xi32>) -> tensor<2x3x!quant.uniform<i8:f32, 2.000000e+00:3>>
// CHECK: return %[[BROADCAST]]

// -----

// Test that a quantized stablehlo.broadcast_in_dim is converted to
// tfl.expand_dims and tfl.broadcast_to when input rank is smaller than output
// rank.

// CHECK-LABEL: broadcast_in_dim_with_expand
// CHECK-SAME: %[[ARG0:.*]]: tensor<1x2x!quant.uniform<i8:f32, 2.000000e+00:3>>
func.func @broadcast_in_dim_with_expand(
    %arg0: tensor<1x2x!quant.uniform<i8:f32, 2.000000e+00:3>>
  ) -> tensor<3x2x1x1x!quant.uniform<i8:f32, 2.000000e+00:3>> {
  %0 = "stablehlo.broadcast_in_dim"(%arg0) {
    broadcast_dimensions = array<i64: 0, 1>
  } : (tensor<1x2x!quant.uniform<i8:f32, 2.000000e+00:3>>) -> tensor<3x2x1x1x!quant.uniform<i8:f32, 2.000000e+00:3>>
  return %0 : tensor<3x2x1x1x!quant.uniform<i8:f32, 2.000000e+00:3>>
}

// CHECK-DAG: %[[BROADCAST_DIM:.*]] = arith.constant dense<{{\[3, 2, 1, 1\]}}> : tensor<4xi32>
// CHECK-DAG: %[[EXPAND_DIM1:.*]] = arith.constant dense<3> : tensor<1xi32>
// CHECK-DAG: %[[EXPAND_DIM0:.*]] = arith.constant dense<2> : tensor<1xi32>
// CHECK: %[[EXPAND0:.*]] = "tfl.expand_dims"(%[[ARG0]], %[[EXPAND_DIM0]]) : (tensor<1x2x!quant.uniform<i8:f32, 2.000000e+00:3>>, tensor<1xi32>) -> tensor<1x2x1x!quant.uniform<i8:f32, 2.000000e+00:3>>
// CHECK: %[[EXPAND1:.*]] = "tfl.expand_dims"(%[[EXPAND0]], %[[EXPAND_DIM1]]) : (tensor<1x2x1x!quant.uniform<i8:f32, 2.000000e+00:3>>, tensor<1xi32>) -> tensor<1x2x1x1x!quant.uniform<i8:f32, 2.000000e+00:3>>
// CHECK: %[[BROADCAST:.*]] = "tfl.broadcast_to"(%[[EXPAND1]], %[[BROADCAST_DIM]]) : (tensor<1x2x1x1x!quant.uniform<i8:f32, 2.000000e+00:3>>, tensor<4xi32>) -> tensor<3x2x1x1x!quant.uniform<i8:f32, 2.000000e+00:3>>
// CHECK: return %[[BROADCAST]]

// -----

// Test that a quantized stablehlo.broadcast_in_dim is converted to
// tfl.transpose, tfl.expand_dims and tfl.broadcast_to when broadcast_dimensions
// is not in ascending order and input rank is smaller than output rank.

// CHECK-LABEL: broadcast_in_dim_with_transpose_and_expand
// CHECK-SAME: %[[ARG0:.*]]: tensor<2x3x4x!quant.uniform<i8:f32, 2.000000e+00:3>>
func.func @broadcast_in_dim_with_transpose_and_expand(
    %arg0: tensor<2x3x4x!quant.uniform<i8:f32, 2.000000e+00:3>>
  ) -> tensor<3x2x1x1x4x!quant.uniform<i8:f32, 2.000000e+00:3>> {
  %0 = "stablehlo.broadcast_in_dim"(%arg0) {
    broadcast_dimensions = array<i64: 1, 0, 4>
  } : (tensor<2x3x4x!quant.uniform<i8:f32, 2.000000e+00:3>>) -> tensor<3x2x1x1x4x!quant.uniform<i8:f32, 2.000000e+00:3>>
  return %0 : tensor<3x2x1x1x4x!quant.uniform<i8:f32, 2.000000e+00:3>>
}

// CHECK-DAG: %[[BROADCAST_DIM:.*]] = arith.constant dense<{{\[3, 2, 1, 1, 4\]}}> : tensor<5xi32>
// CHECK-DAG: %[[EXPAND_DIM1:.*]] = arith.constant dense<3> : tensor<1xi32>
// CHECK-DAG: %[[EXPAND_DIM0:.*]] = arith.constant dense<2> : tensor<1xi32>
// CHECK-DAG: %[[PERM:.*]] = arith.constant dense<{{\[1, 0, 2\]}}> : tensor<3xi32>
// CHECK: %[[TRANSPOSE:.*]] = "tfl.transpose"(%[[ARG0]], %[[PERM]]) : (tensor<2x3x4x!quant.uniform<i8:f32, 2.000000e+00:3>>, tensor<3xi32>) -> tensor<3x2x4x!quant.uniform<i8:f32, 2.000000e+00:3>>
// CHECK: %[[EXPAND0:.*]] = "tfl.expand_dims"(%[[TRANSPOSE]], %[[EXPAND_DIM0]]) : (tensor<3x2x4x!quant.uniform<i8:f32, 2.000000e+00:3>>, tensor<1xi32>) -> tensor<3x2x1x4x!quant.uniform<i8:f32, 2.000000e+00:3>>
// CHECK: %[[EXPAND1:.*]] = "tfl.expand_dims"(%[[EXPAND0]], %[[EXPAND_DIM1]]) : (tensor<3x2x1x4x!quant.uniform<i8:f32, 2.000000e+00:3>>, tensor<1xi32>) -> tensor<3x2x1x1x4x!quant.uniform<i8:f32, 2.000000e+00:3>>
// CHECK: %[[BROADCAST:.*]] = "tfl.broadcast_to"(%[[EXPAND1]], %[[BROADCAST_DIM]]) : (tensor<3x2x1x1x4x!quant.uniform<i8:f32, 2.000000e+00:3>>, tensor<5xi32>) -> tensor<3x2x1x1x4x!quant.uniform<i8:f32, 2.000000e+00:3>>
// CHECK: return %[[BROADCAST]]

// -----

// Test that a float stablehlo.broadcast_in_dim is not converted to tfl.broadcast_to.

// CHECK-LABEL: float_broadcast_in_dim
func.func @float_broadcast_in_dim(%arg0: tensor<1x2xf32>) -> tensor<3x2xf32> {
  %0 = "stablehlo.broadcast_in_dim"(%arg0) {
    broadcast_dimensions = array<i64: 0, 1>
  } : (tensor<1x2xf32>) -> tensor<3x2xf32>
  return %0 : tensor<3x2xf32>
}

// CHECK-NOT: tfl.broadcast_to
// CHECK-NOT: tfl.transpose
// CHECK-NOT: tfl.expand_dims
// CHECK: stablehlo.broadcast_in_dim
