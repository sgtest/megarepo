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
"""Tests for distributed save/load with the new load algorithm."""

import os
import shutil
import tempfile
import threading
import time
from typing import Optional

from absl.testing import parameterized

from tensorflow.python.data.experimental.kernel_tests.service import test_base as data_service_test_base
from tensorflow.python.data.experimental.ops import data_service_ops
from tensorflow.python.data.experimental.ops import distributed_save_op
from tensorflow.python.data.kernel_tests import test_base
from tensorflow.python.data.ops import dataset_ops
from tensorflow.python.data.ops import load_op
from tensorflow.python.framework import combinations
from tensorflow.python.platform import googletest
from tensorflow.python.platform import test


class TestSnapshot:
  """Test data for snapshots."""

  def __init__(self):
    temp_dir = tempfile.mkdtemp(dir=googletest.GetTempDir())
    self.path = os.path.join(
        tempfile.mkdtemp(dir=temp_dir), "distributed_save_load_test")

  def __del__(self):
    shutil.rmtree(self.path)


class DistributedSaveLoadTest(
    data_service_test_base.TestBase, parameterized.TestCase):
  """Tests for distributed save/load with the new load algorithm.

  TODO(b/297930782): Add fault tolerance tests.
  """

  @combinations.generate(
      combinations.times(
          test_base.default_test_combinations(),
          combinations.combine(
              num_workers=[1, 3],
              num_elements=[0, 10],
              num_repetitions=[1, 3],
              compression=[None, "AUTO", "GZIP"])))
  def test_save_load(
      self,
      num_workers: int,
      num_elements: int,
      num_repetitions: int,
      compression: Optional[str]):
    test_snapshot = TestSnapshot()
    cluster = data_service_test_base.TestCluster(num_workers=num_workers)
    dataset = dataset_ops.Dataset.range(num_elements)
    dataset = dataset.repeat(num_repetitions)
    self.evaluate(
        distributed_save_op.distributed_save(
            dataset, test_snapshot.path, cluster.dispatcher_address()))

    # Unlike the old load op, v2 does not need to wait for snapshot to finish.
    dataset = load_op._load_distributed_snapshot_v2(test_snapshot.path)
    self.assertDatasetProduces(
        dataset,
        list(range(num_elements)) * num_repetitions,
        assert_items_equal=True)

  @combinations.generate(
      combinations.times(
          test_base.default_test_combinations(),
          combinations.combine(num_workers=[1, 3])))
  def test_concurrent_save_load(self, num_workers: int):
    test_snapshot = TestSnapshot()
    cluster = data_service_test_base.TestCluster(num_workers=num_workers)

    def load_thread_fn():
      dataset = load_op._load_distributed_snapshot_v2(test_snapshot.path)
      self.assertDatasetProduces(
          dataset, list(range(10)), assert_items_equal=True)
    load_thread = threading.Thread(target=load_thread_fn, name="load_thread")
    load_thread.start()

    def save_thread_fn():
      time.sleep(5)
      dataset = dataset_ops.Dataset.range(10)
      self.evaluate(
          distributed_save_op.distributed_save(
              dataset, test_snapshot.path, cluster.dispatcher_address()))
    save_thread = threading.Thread(target=save_thread_fn, name="save_thread")
    save_thread.start()
    save_thread.join()
    load_thread.join()

  @combinations.generate(
      combinations.times(
          test_base.default_test_combinations(),
          combinations.combine(num_workers=[1, 3], num_elements=[0, 10])))
  def test_distributed_load(self, num_workers: int, num_elements: int):
    self.skipTest(
        "TODO(b/297930782): Fix deadlock when calling "
        "TaskRunner::GetProcessingTimeNsec(): The heartbeat thread tries to "
        "lock task runner when building a heartbeat request, while the task "
        "runner may be waiting for the next element while holding the lock.")
    test_snapshot = TestSnapshot()
    cluster = data_service_test_base.TestCluster(num_workers=num_workers)
    dataset = dataset_ops.Dataset.range(num_elements)
    self.evaluate(
        distributed_save_op.distributed_save(
            dataset, test_snapshot.path, cluster.dispatcher_address()))

    dataset = load_op._load_distributed_snapshot_v2(test_snapshot.path)
    # TODO(b/297930782): Support dynamic sharding.
    dataset = dataset.apply(
        data_service_ops.distribute(
            data_service_ops.ShardingPolicy.OFF, cluster.dispatcher_address()))
    self.assertDatasetProduces(
        dataset,
        list(range(num_elements)) * num_workers,
        assert_items_equal=True)

  @combinations.generate(
      combinations.times(
          test_base.default_test_combinations(),
          combinations.combine(num_workers=[1, 3])))
  def test_save_before_sample(self, num_workers: int):
    num_elements = 10
    num_datasets = 3
    test_snapshot = TestSnapshot()
    cluster = data_service_test_base.TestCluster(num_workers=num_workers)
    datasets = [
        dataset_ops.Dataset.range(num_elements) for i in range(num_datasets)]
    for i, dataset in enumerate(datasets):
      self.evaluate(
          distributed_save_op.distributed_save(
              dataset,
              os.path.join(test_snapshot.path, f"dataset_{i}"),
              cluster.dispatcher_address()))

    loaded_datasets = []
    for i in range(len(datasets)):
      loaded_datasets.append(
          load_op._load_distributed_snapshot_v2(
              os.path.join(test_snapshot.path, f"dataset_{i}")))
    dataset = dataset_ops.Dataset.sample_from_datasets(
        loaded_datasets,
        weights=[1.0] * num_datasets,
        stop_on_empty_dataset=False)
    self.assertDatasetProduces(
        dataset,
        list(range(num_elements)) * num_datasets,
        assert_items_equal=True)

  @combinations.generate(
      combinations.times(
          test_base.default_test_combinations(),
          combinations.combine(num_workers=[1, 3], num_repetitions=[1, 3])))
  def test_save_after_sample(self, num_workers: int, num_repetitions: int):
    num_elements = 10
    num_datasets = 3
    test_snapshot = TestSnapshot()
    cluster = data_service_test_base.TestCluster(num_workers=num_workers)
    datasets = [
        dataset_ops.Dataset.range(num_elements) for i in range(num_datasets)]
    if num_repetitions > 1:
      datasets = [dataset.repeat(num_repetitions) for dataset in datasets]
    dataset = dataset_ops.Dataset.sample_from_datasets(
        datasets, weights=[1.0] * num_datasets, stop_on_empty_dataset=False)
    self.evaluate(
        distributed_save_op.distributed_save(
            dataset, test_snapshot.path, cluster.dispatcher_address()))

    dataset = load_op._load_distributed_snapshot_v2(test_snapshot.path)
    self.assertDatasetProduces(
        dataset,
        list(range(num_elements)) * num_datasets * num_repetitions,
        assert_items_equal=True)

  @combinations.generate(
      combinations.times(
          test_base.default_test_combinations(),
          combinations.combine(num_workers=[1, 3])))
  def test_enumerate(self, num_workers: int):
    test_snapshot = TestSnapshot()
    cluster = data_service_test_base.TestCluster(num_workers)
    dataset = dataset_ops.Dataset.from_tensor_slices(["a", "b", "c"])
    dataset = dataset.repeat(3)
    dataset = dataset.enumerate()
    self.evaluate(
        distributed_save_op.distributed_save(
            dataset, test_snapshot.path, cluster.dispatcher_address()))

    dataset = load_op._load_distributed_snapshot_v2(test_snapshot.path)
    indexes, elements = map(list, zip(*self.getDatasetOutput(dataset)))
    if num_workers == 1:
      self.assertCountEqual(indexes, list(range(9)))
    self.assertCountEqual(elements, [b"a", b"b", b"c"] * 3)


if __name__ == "__main__":
  test.main()
