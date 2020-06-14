#!/usr/bin/env bash

# We want to build multiple go binaries, so we use a custom build step on CI.
set -ex
cd "$(dirname "${BASH_SOURCE[0]}")"/../../..

OUTPUT=$(mktemp -d -t sgdockerbuild_XXXXXXX)
cleanup() {
  rm -rf "$OUTPUT"
}
trap cleanup EXIT

# Environment for building linux binaries
export GO111MODULE=on
export GOARCH=amd64
export GOOS=linux
export CGO_ENABLED=0

pkg=github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend
go build -trimpath -ldflags "-X github.com/sourcegraph/sourcegraph/internal/version.version=$VERSION  -X github.com/sourcegraph/sourcegraph/internal/version.timestamp=$(date +%s)" -buildmode exe -tags dist -o "$OUTPUT/$(basename $pkg)" "$pkg"

docker build -f enterprise/cmd/frontend/Dockerfile -t "$IMAGE" "$OUTPUT" \
  --progress=plain \
  --build-arg COMMIT_SHA \
  --build-arg DATE \
  --build-arg VERSION
