#!/usr/bin/env bash

# This script builds the symbols go binary.
# Requires a single argument which is the path to the target bindir.

cd "$(dirname "${BASH_SOURCE[0]}")/../.."
set -eu

OUTPUT="${1:?no output path provided}"

# Environment for building linux binaries
export GO111MODULE=on
export GOARCH=amd64
export GOOS=linux

# go-sqlite3 depends on cgo. Without cgo, it will build but it'll throw an error at query time.
export CGO_ENABLED=1

# Ensure musl-gcc is available since we're building to run on Alpine, which uses musl.
if ! command -v musl-gcc >/dev/null; then
  echo "musl-gcc not found, which is needed for cgo for go-sqlite3. Run 'apt-get install -y musl-tools'."
  exit 1
fi

echo "--- go build"
pkg="github.com/sourcegraph/sourcegraph/cmd/symbols"
env CC=musl-gcc go build \
  -trimpath \
  -ldflags "-X github.com/sourcegraph/sourcegraph/internal/version.version=$VERSION  -X github.com/sourcegraph/sourcegraph/internal/version.timestamp=$(date +%s)" \
  -buildmode exe \
  -tags dist \
  -o "$OUTPUT/$(basename $pkg)" \
  "$pkg"

# We can't use -v because the spawned container might not share
# the same file system (e.g. when we're already inside docker
# and the spawned docker container will be a sibling on the host).
#
# A workaround is to feed the file into the container via stdin:
#
#     'cat FILE | docker run ... -i ... sh -c "cat > FILE && ..."'
echo "--- sanity check"
# shellcheck disable=SC2002
cat "$OUTPUT/$(basename $pkg)" | docker run \
  --rm \
  -i \
  sourcegraph/alpine@sha256:ce099fbcd3cf70b338fc4cb2a4e1fa9ae847de21afdb0a849a393b87d94fb174 \
  sh -c "cat > /symbols && chmod a+x /symbols && env SANITY_CHECK=true /symbols"
