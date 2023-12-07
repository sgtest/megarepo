# Copyright 2023 The TensorFlow Authors. All Rights Reserved.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.
# ==============================================================================
"""Base test class for quantize_model Tests."""
from typing import Mapping, Sequence, Optional

from absl.testing import parameterized
import numpy as np
import tensorflow  # pylint: disable=unused-import

from tensorflow.python.eager import def_function
from tensorflow.python.framework import dtypes
from tensorflow.python.framework import ops
from tensorflow.python.framework import tensor_spec
from tensorflow.python.module import module
from tensorflow.python.ops import array_ops
from tensorflow.python.ops import math_ops
from tensorflow.python.ops import nn_ops
from tensorflow.python.platform import test
from tensorflow.python.saved_model import save as saved_model_save
from tensorflow.python.types import core


class QuantizedModelTest(test.TestCase, parameterized.TestCase):
  """Base test class for StableHLO quant tests."""

  def setUp(self) -> None:
    super().setUp()

    # Many test cases for quantization involve creating and saving the input
    # model and saving the output quantized model. These two member
    # attributes can be used to specify the paths for such models,
    # respectively. These paths will be cleaned up after each test case.
    self._input_saved_model_path = self.create_tempdir('input').full_path
    self._output_saved_model_path = self.create_tempdir('output').full_path
    # Extra output path occasionally used for comparing two different
    # quantized models.
    self._output_saved_model_path_2 = self.create_tempdir('output2').full_path

  def _create_matmul_model(
      self,
      input_shape: Sequence[int],
      weight_shape: Sequence[int],
      saved_model_path: str,
      has_bias: bool = False,
      activation_fn: Optional[ops.Operation] = None,
      bias_size: Optional[int] = None,
      use_biasadd: bool = True,
  ) -> module.Module:
    class MatmulModel(module.Module):
      """A simple model with a single matmul.

      Bias and activation function are optional.
      """

      def __init__(
          self,
          weight_shape: Sequence[int],
          bias_size: Optional[int] = None,
          activation_fn: Optional[ops.Operation] = None,
          use_biasadd: bool = True,
      ) -> None:
        """Initializes a MatmulModel.

        Args:
          weight_shape: Shape of the weight tensor.
          bias_size: If None, do not use bias. Else, use given size as bias.
          activation_fn: The activation function to be used. No activation
            function if None.
          use_biasadd: If True, use BiasAdd for adding bias, else use AddV2.
        """
        self.bias_size = bias_size
        self.activation_fn = activation_fn
        self.use_biasadd = use_biasadd
        self.filters = np.random.uniform(low=-1.0, high=1.0, size=weight_shape)

        if bias_size is not None:
          self.bias = np.random.uniform(low=-1.0, high=1.0, size=bias_size)

      def has_bias(self) -> bool:
        return self.bias_size is not None

      def has_reshape(self) -> bool:
        return self.has_bias() and self.bias_size != self.filters.shape[-1]

      @def_function.function
      def matmul(self, input_tensor: core.Tensor) -> Mapping[str, core.Tensor]:
        """Performs a matrix multiplication.

        Depending on self.has_bias and self.activation_fn, it may add a bias
        term or
        go through the activaction function.

        Args:
          input_tensor: Input tensor to matmul with the filter.

        Returns:
          A map of: output key -> output result.
        """
        out = math_ops.matmul(input_tensor, self.filters, name='sample/matmul')

        return {'output': out}

    # If bias_size is not explictly given, it should default to width of weight.
    if bias_size is None and has_bias:
      bias_size = weight_shape[-1]

    # Verify that when bias_size is not None, has_bias should be True.
    # And if bias_size is None, has_bias should be False.
    assert (bias_size is None) != has_bias

    model = MatmulModel(weight_shape, bias_size, activation_fn)
    saved_model_save.save(
        model,
        saved_model_path,
        signatures=model.matmul.get_concrete_function(
            tensor_spec.TensorSpec(
                shape=input_shape, dtype=dtypes.float32, name='input_tensor'
            )
        ),
    )
    return model

  def _create_matmul_and_same_scale_model(
      self,
      input_shape: Sequence[int],
      weight_shape: Sequence[int],
      saved_model_path: str,
      same_scale_op: str,
  ) -> module.Module:
    class MatmulAndSameScaleModel(module.Module):
      """A simple model with a same-scale op.

      Op name in StableHLO dialect is given as a string.
      """

      def __init__(
          self,
          weight_shape: Sequence[int],
          same_scale_op: str,
      ) -> None:
        """Initializes a MatmulModel.

        Args:
          weight_shape: Shape of the weight tensor.
          same_scale_op: Name of the same-scale op to be tested. Raises error
            when an unknown name is given.
        """
        self.filters = np.random.uniform(low=-1.0, high=1.0, size=weight_shape)
        self.same_scale_op = same_scale_op

      @def_function.function
      def matmul_and_same_scale(
          self, input_tensor: core.Tensor
      ) -> Mapping[str, core.Tensor]:
        """Performs a matrix multiplication.

        Args:
          input_tensor: Input tensor to matmul with the filter.

        Returns:
          A map of: output key -> output result.
        """
        out = math_ops.matmul(input_tensor, self.filters, name='sample/matmul')

        if self.same_scale_op == 'concatenate':
          ones = array_ops.ones_like(out)
          out = array_ops.concat([out, ones], 0)
        elif self.same_scale_op == 'gather':
          out = array_ops.gather(out, indices=[0], axis=0)
        elif self.same_scale_op == 'pad':
          paddings = array_ops.ones(
              (array_ops.rank(out), 2), dtype=dtypes.int32
          )
          out = array_ops.pad(out, paddings, 'CONSTANT')
        elif self.same_scale_op == 'reshape':
          out = array_ops.reshape(out, (array_ops.size(out), -1))
        elif self.same_scale_op == 'select':
          rng = np.random.default_rng(seed=1234)
          condition = ops.convert_to_tensor(
              rng.uniform(low=0.0, high=1.0, size=out.shape) < 0.5
          )
          ones = array_ops.ones_like(out)
          out = math_ops.select(condition, out, ones)
        elif self.same_scale_op == 'slice':
          begin = array_ops.zeros(
              (array_ops.rank(out)), dtype=dtypes.int32
          )
          size = array_ops.ones(
              (array_ops.rank(out)), dtype=dtypes.int32
          )
          out = array_ops.slice(out, begin, size)
        elif self.same_scale_op == 'transpose':
          out = array_ops.transpose(out)
        else:
          raise NotImplementedError(
              '{} is not implemented for integration test.'.format(
                  self.same_scale_op
              )
          )

        return {'output': out}

    model = MatmulAndSameScaleModel(weight_shape, same_scale_op)
    saved_model_save.save(
        model,
        saved_model_path,
        signatures=model.matmul_and_same_scale.get_concrete_function(
            tensor_spec.TensorSpec(
                shape=input_shape, dtype=dtypes.float32, name='input_tensor'
            )
        ),
    )
    return model

  def _create_conv2d_model(
      self,
      input_shape: Sequence[int],
      filter_shape: Sequence[int],
      saved_model_path: str,
      has_bias: bool = False,
      has_batch_norm: bool = False,
      activation_fn: Optional[ops.Operation] = None,
      strides: Sequence[int] = (1, 1, 1, 1),
      dilations: Sequence[int] = (1, 1, 1, 1),
      padding: str = 'SAME',
  ) -> module.Module:
    class ConvModel(module.Module):
      """A simple model with a single conv2d, bias and relu."""

      def __init__(self):
        self.out_channel_size = filter_shape[-1]

        # This ensures filters will have different value range per out channel
        self.filters = np.stack(
            [
                np.random.uniform(
                    low=-(i + 1), high=(i + 1), size=filter_shape[:-1]
                ).astype('f4')
                for i in range(self.out_channel_size)
            ],
            axis=-1,
        )

        self.bias = np.random.uniform(
            low=0, high=10, size=(self.out_channel_size)
        ).astype('f4')

      @def_function.function
      def conv2d(self, input_tensor: core.Tensor) -> Mapping[str, core.Tensor]:
        """Performs a 2D convolution operation.

        Args:
          input_tensor: Input tensor to perform convolution on.

        Returns:
          A map of: output key -> output result.
        """
        scale = [1.0] * self.out_channel_size
        offset = [0.5] * self.out_channel_size
        mean, variance = scale, offset
        out = nn_ops.conv2d(
            input_tensor,
            self.filters,
            strides=strides,
            dilations=dilations,
            padding=padding,
            data_format='NHWC',
            name='sample/conv',
        )
        if has_batch_norm:
          # Fusing is supported for non-training case.
          out, _, _, _, _, _ = nn_ops.fused_batch_norm_v3(
              out, scale, offset, mean, variance, is_training=False
          )
        return {'output': out}

    model = ConvModel()
    saved_model_save.save(
        model,
        saved_model_path,
        signatures=model.conv2d.get_concrete_function(
            tensor_spec.TensorSpec(
                shape=input_shape, dtype=dtypes.float32, name='input_tensor'
            )
        ),
    )
    return model
