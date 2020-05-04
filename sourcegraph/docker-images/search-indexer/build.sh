#!/usr/bin/env bash

set -ex
cd "$(dirname "${BASH_SOURCE[0]}")"

# This merely re-tags the image to match our official versioning scheme. The
# actual image currently lives here:
# https://github.com/sourcegraph/zoekt/blob/master/Dockerfile.indexserver
#
# The images are tagged using the same pseudo-versions as go mod, so we
# extract the version from our go.mod

version=$(go mod edit -print | awk '/sourcegraph\/zoekt/ {print substr($4, 2)}')

docker pull index.docker.io/sourcegraph/zoekt-indexserver:"$version"
docker tag index.docker.io/sourcegraph/zoekt-indexserver:"$version" "$IMAGE"
