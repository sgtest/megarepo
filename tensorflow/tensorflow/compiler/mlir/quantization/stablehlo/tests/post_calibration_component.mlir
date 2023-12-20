// RUN: stablehlo-quant-opt %s -split-input-file -stablehlo-test-post-calibration-component | FileCheck %s

// Tests that a simple dot_general (lifted as a function) with CustomAggregators
// around it is quantized. The resulting graph has quantized types unpacked into
// int ops.
func.func @main(%arg0: tensor<1x1024xf32>) -> tensor<1x3xf32> {
  %0 = "tf.Const"() <{value = dense<0.5> : tensor<1024x3xf32>}> : () -> tensor<1024x3xf32>
  %1 = "tf.CustomAggregator"(%arg0) <{id = "1"}> {calibration_method = 1 : i64, device = "", initial_num_bins = 0 : i64, max = 0.999992311 : f32, max_percentile = 0.000000e+00 : f32, min = 7.547870e-07 : f32, min_percentile = 0.000000e+00 : f32} : (tensor<1x1024xf32>) -> tensor<1x1024xf32>
  %2 = "tf.XlaCallModule"(%1, %0) <{Sout = [#tf_type.shape<1x3>], dim_args_spec = [], disabled_checks = [], has_token_input_output = false, module = "", platforms = [], version = 5 : i64}> {_entry_function = @composite_dot_general_fn_1, _original_entry_function = "composite_dot_general_fn_1", _stablehlo_module_attrs = {}, _tfl_quant_trait = "fully_quantizable", device = ""} : (tensor<1x1024xf32>, tensor<1024x3xf32>) -> tensor<1x3xf32>
  %3 = "tf.CustomAggregator"(%2) <{id = "2"}> {calibration_method = 1 : i64, device = "", initial_num_bins = 0 : i64, max = 18.3033524 : f32, max_percentile = 0.000000e+00 : f32, min = -17.5216827 : f32, min_percentile = 0.000000e+00 : f32} : (tensor<1x3xf32>) -> tensor<1x3xf32>
  return %3 : tensor<1x3xf32>
}
func.func private @composite_dot_general_fn_1(%arg0: tensor<1x1024xf32>, %arg1: tensor<1024x3xf32>) -> tensor<1x3xf32> attributes {_from_xla_call_module} {
  %0 = stablehlo.dot_general %arg0, %arg1, contracting_dims = [1] x [0] : (tensor<1x1024xf32>, tensor<1024x3xf32>) -> tensor<1x3xf32>
  return %0 : tensor<1x3xf32>
}
// CHECK-LABEL: func.func @main
// CHECK-SAME: (%[[ARG_0:.+]]: tensor<1x1024xf32>) -> tensor<1x3xf32>
// CHECK: %[[XLA_CALL_MODULE_0:.+]] = "tf.XlaCallModule"(%[[ARG_0]]) <{Sout = [#tf_type.shape<1x3>], {{.*}}, module = "", platforms = ["CPU", "TPU"], version = 9 : i64}> {_entry_function = @main_0, {{.*}}} : (tensor<1x1024xf32>) -> tensor<1x3xf32>
// CHECK-NEXT: return %[[XLA_CALL_MODULE_0]] : tensor<1x3xf32>

// CHECK: func.func private @main_0(%[[ARG_1:.+]]: tensor<1x1024xf32>) -> tensor<1x3xf32>
// CHECK-SAME: attributes {_from_xla_call_module}

// Tests that the dot_general accepts i8 tensors and outputs an i32 tensor.
// Note: Argument quantization sequence omitted.
// CHECK: stablehlo.dot_general %{{.+}}, %{{.+}}, contracting_dims = [1] x [0] : (tensor<1x1024xi8>, tensor<1024x3xi8>) -> tensor<1x3xi32>

// Note: Result dequantization sequence omitted.
// CHECK: return %{{.+}} : tensor<1x3xf32>

// -----

// Tests that a simple dot_general without CustomAggregators is not quantized.

func.func @main(%arg0: tensor<1x1024xf32>) -> tensor<1x3xf32> {
  %0 = "tf.Const"() <{value = dense<0.5> : tensor<1024x3xf32>}> : () -> tensor<1024x3xf32>
  %2 = "tf.XlaCallModule"(%arg0, %0) <{Sout = [#tf_type.shape<1x3>], dim_args_spec = [], disabled_checks = [], has_token_input_output = false, module = "", platforms = [], version = 5 : i64}> {_entry_function = @composite_dot_general_fn_1, _original_entry_function = "composite_dot_general_fn_1", _stablehlo_module_attrs = {}, _tfl_quant_trait = "fully_quantizable", device = ""} : (tensor<1x1024xf32>, tensor<1024x3xf32>) -> tensor<1x3xf32>
  return %2 : tensor<1x3xf32>
}
func.func private @composite_dot_general_fn_1(%arg0: tensor<1x1024xf32>, %arg1: tensor<1024x3xf32>) -> tensor<1x3xf32> attributes {_from_xla_call_module} {
  %0 = stablehlo.dot_general %arg0, %arg1, contracting_dims = [1] x [0] : (tensor<1x1024xf32>, tensor<1024x3xf32>) -> tensor<1x3xf32>
  return %0 : tensor<1x3xf32>
}
// CHECK-LABEL: func.func @main
// CHECK-SAME: (%[[ARG_0:.+]]: tensor<1x1024xf32>) -> tensor<1x3xf32>
// CHECK: %[[XLA_CALL_MODULE_0:.+]] = "tf.XlaCallModule"() <{Sout = [#tf_type.shape<1024x3>], {{.*}}, module = "", platforms = ["CPU", "TPU"], version = 9 : i64}>
// CHECK-SAME: {_entry_function = @main_0, _stablehlo_module_attrs = {jax.uses_shape_polymorphism = true}} : () -> tensor<1024x3xf32>
// CHECK: %[[XLA_CALL_MODULE_1:.+]] = "tf.XlaCallModule"(%[[ARG_0]], %[[XLA_CALL_MODULE_0]]) <{Sout = [#tf_type.shape<1x3>], {{.*}}, module = "", platforms = [], version = 5 : i64}> {_entry_function = @main_1, _original_entry_function = "composite_dot_general_fn_1", _stablehlo_module_attrs = {}, _tfl_quant_trait = "fully_quantizable", device = ""} : (tensor<1x1024xf32>, tensor<1024x3xf32>) -> tensor<1x3xf32>
// CHECK: return %[[XLA_CALL_MODULE_1]] : tensor<1x3xf32>

// CHECK: func.func private @main_0() -> tensor<1024x3xf32>
// CHECK-SAME: attributes {_from_xla_call_module}
// CHECK: %[[CONST_0:.+]] = stablehlo.constant dense<{{.*}}> : tensor<1024x3xf32>
// CHECK: return %[[CONST_0]] : tensor<1024x3xf32>

// CHECK: func.func private @main_1(%[[ARG_1:.+]]: tensor<1x1024xf32>, %[[ARG_2:.+]]: tensor<1024x3xf32>) -> tensor<1x3xf32>
// CHECK-SAME: attributes {_from_xla_call_module}
// CHECK: %[[DOT_GENERAL_0:.+]] = stablehlo.dot_general %[[ARG_1]], %[[ARG_2]], contracting_dims = [1] x [0] : (tensor<1x1024xf32>, tensor<1024x3xf32>) -> tensor<1x3xf32>
