#!/usr/bin/env bash

set -ex
cd "$(dirname "${BASH_SOURCE[0]}")"

# Keep in sync with version in go.mod
export OTEL_COLLECTOR_VERSION="${OTEL_COLLECTOR_VERSION:-0.71.0}"

docker build -t "${IMAGE:-sourcegraph/opentelemetry-collector}" . \
  --platform linux/amd64 \
  --build-arg OTEL_COLLECTOR_VERSION \
  --build-arg COMMIT_SHA \
  --build-arg DATE \
  --build-arg VERSION
