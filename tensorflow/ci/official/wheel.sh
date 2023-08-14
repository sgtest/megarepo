#!/bin/bash
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
source "${BASH_SOURCE%/*}/utilities/setup.sh"

# Record GPU count and CUDA version status
if [[ "$TFCI_NVIDIA_SMI_ENABLE" == 1 ]]; then
  tfrun nvidia-smi
fi

# Update the version numbers for Nightly only
if [[ "$TFCI_NIGHTLY_UPDATE_VERSION_ENABLE" == 1 ]]; then
  tfrun python3 tensorflow/tools/ci_build/update_version.py --nightly
fi

tfrun bazel "${TFCI_BAZEL_BAZELRC_ARGS[@]}" build "${TFCI_BAZEL_COMMON_ARGS[@]}" //tensorflow/tools/pip_package:build_pip_package
tfrun ./bazel-bin/tensorflow/tools/pip_package/build_pip_package build "${TFCI_BUILD_PIP_PACKAGE_ARGS[@]}"
tfrun ./ci/official/utilities/rename_and_verify_wheels.sh "$TFCI_OUTPUT_DIR"

if [[ "$TFCI_UPLOAD_WHL_PYPI_ENABLE" == 1 ]]; then
  twine upload "${TFCI_UPLOAD_WHL_PYPI_ARGS[@]}" "$TFCI_OUTPUT_DIR"/*.whl
fi
if [[ "$TFCI_UPLOAD_WHL_GCS_ENABLE" == 1 ]]; then
  gsutil cp "$TFCI_OUTPUT_DIR"/*.whl "$TFCI_UPLOAD_WHL_GCS_URI"
fi

if [[ "$TFCI_WHL_BAZEL_TEST_ENABLE" == 1 ]]; then
  tfrun bazel "${TFCI_BAZEL_BAZELRC_ARGS[@]}" test "${TFCI_BAZEL_COMMON_ARGS[@]}" --config=nonpip
fi
