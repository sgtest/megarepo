#!/usr/bin/env bash

# This script builds the symbols docker image.

cd "$(dirname "${BASH_SOURCE[0]}")/../../.."
set -eu

OUTPUT=$(mktemp -d -t sgdockerbuild_XXXXXXX)
cleanup() {
  rm -rf "$OUTPUT"
}
trap cleanup EXIT

cp -a ./cmd/symbols/ctags-install-alpine.sh "$OUTPUT"

# Build go binary into $OUTPUT
./enterprise/cmd/symbols/go-build.sh "$OUTPUT"

echo "--- docker build"
docker build -f enterprise/cmd/symbols/Dockerfile -t "$IMAGE" "$OUTPUT" \
  --progress=plain \
  --build-arg COMMIT_SHA \
  --build-arg DATE \
  --build-arg VERSION
