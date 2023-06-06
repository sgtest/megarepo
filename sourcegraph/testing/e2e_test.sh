#!/usr/bin/env bash

set -eu
source ./testing/tools/integration_runner.sh || exit 1

tarball="$1"
image_name="$2"
e2e_test="$3"
mocha_config="$4"
files="$5"

url="http://localhost:7080"

SOURCEGRAPH_BASE_URL="$url"
export SOURCEGRAPH_BASE_URL

# Backend integration tests uses a specific GITHUB_TOKEN that is available as GHE_GITHUB_TOKEN
# because it refers to our internal GitHub enterprise instance used for testing.
GITHUB_TOKEN="$GHE_GITHUB_TOKEN"
export GITHUB_TOKEN

ALLOW_SINGLE_DOCKER_CODE_INSIGHTS="true"
export ALLOW_SINGLE_DOCKER_CODE_INSIGHTS

run_server_image "$tarball" "$image_name" "$url" "7080"

echo "--- e2e test //client/web/src/end-to-end:e2e"
"$e2e_test" --config "$mocha_config" --retries 4 "$files"

echo "--- done"
