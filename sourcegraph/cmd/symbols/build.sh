#!/usr/bin/env bash

# We want to build multiple go binaries, so we use a custom build step on CI.
cd $(dirname "${BASH_SOURCE[0]}")/../..
set -ex

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

for pkg in github.com/sourcegraph/sourcegraph/cmd/symbols; do
    go build -ldflags "-X github.com/sourcegraph/sourcegraph/pkg/version.version=$VERSION" -buildmode exe -tags dist -o $OUTPUT/$(basename $pkg) $pkg
done

mkdir "$OUTPUT/.ctags.d"
cp cmd/symbols/.ctags.d/additional-languages.ctags "$OUTPUT/.ctags.d/additional-languages.ctags"

docker build -f cmd/symbols/Dockerfile -t $IMAGE $OUTPUT
