#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"

export JAEGER_VERSION="${JAEGER_VERSION:-1.36.0}"
IMAGE=${IMAGE:-sourcegraph/jaeger-all-in-one}

echo "Building image ${IMAGE} from Jaeger ${JAEGER_VERSION}"

docker build --no-cache -t "${IMAGE}" . \
  --progress=plain \
  --build-arg JAEGER_VERSION \
  --build-arg COMMIT_SHA \
  --build-arg DATE \
  --build-arg VERSION
