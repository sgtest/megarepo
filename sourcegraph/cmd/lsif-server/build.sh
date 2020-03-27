#!/usr/bin/env bash

# We want to build multiple go binaries, so we use a custom build step on CI.
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
set -eux

OUTPUT=`mktemp -d -t sgdockerbuild_XXXXXXX`
cleanup() {
    rm -rf "$OUTPUT"
}
trap cleanup EXIT

# Environment for building linux binaries
export GO111MODULE=on
export GOARCH=amd64
export GOOS=linux
export CGO_ENABLED=0

cp -a ./cmd/lsif-server/precise-code-intel "$OUTPUT"
cp -a ./cmd/lsif-server/prometheus.yml "$OUTPUT"

echo "--- go build"
go build \
    -trimpath \
    -ldflags "-X github.com/sourcegraph/sourcegraph/internal/version.version=$VERSION"  \
    -o "$OUTPUT/lsif-server" github.com/sourcegraph/sourcegraph/cmd/lsif-server

echo "--- docker build"
docker build -f cmd/lsif-server/Dockerfile -t "$IMAGE" "$OUTPUT" \
    --progress=plain \
    --build-arg COMMIT_SHA \
    --build-arg DATE \
    --build-arg VERSION
