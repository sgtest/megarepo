#!/usr/bin/env bash

set -euf -o pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../../../../.."

tmpdir=$(mktemp -d -t melange-bin.XXXXXXXX)
function cleanup() {
  echo "Removing $tmpdir"
  rm -rf "$tmpdir"
}
trap cleanup EXIT

# TODO: Install these binaries as part of the buildkite base image
(
  cd "$tmpdir"
  mkdir bin

  # Install melange from Sourcegraph cache
  # Source: https://github.com/chainguard-dev/melange/releases/download/v0.2.0/melange_0.2.0_linux_amd64.tar.gz
  wget https://storage.googleapis.com/package-repository/ci-binaries/melange_0.2.0_linux_amd64.tar.gz
  tar zxf melange_0.2.0_linux_amd64.tar.gz
  mv melange_0.2.0_linux_amd64/melange bin/melange

  # Install apk from Sourcegraph cache
  # Source: https://gitlab.alpinelinux.org/api/v4/projects/5/packages/generic//v2.12.11/x86_64/apk.static
  wget https://storage.googleapis.com/package-repository/ci-binaries/apk-v2.12.11.tar.gz
  tar zxf apk-v2.12.11.tar.gz
  chmod +x apk
  mv apk bin/apk

  # Fetch custom-built bubblewrap 0.7.0 (temporary, until https://github.com/sourcegraph/infrastructure/pull/4520 is merged)
  # Build from source
  wget https://storage.googleapis.com/package-repository/ci-binaries/bwrap-0.7.0.tar.gz
  tar zxf bwrap-0.7.0.tar.gz
  chmod +x bwrap
  mv bwrap bin/
)

export PATH="$tmpdir/bin:$PATH"

if [ $# -eq 0 ]; then
  echo "No arguments supplied - provide the melange YAML file to build"
  exit 0
fi

name=${1%/}

pushd "wolfi-packages"

if [ ! -e "${name}.yaml" ]; then
  echo "File '$name.yaml' does not exist"
  exit 1
fi

# NOTE: Melange relies upon a more recent version of bubblewrap than ships with Ubuntu 20.04. We therefore build a recent
# bubblewrap release in buildkite-agent-stateless-bazel's Dockerfile, and ship it in /usr/local/bin

echo " * Building melange package '$name'"

# Build package
melange build "$name.yaml" --arch x86_64 --generate-index false

# Upload package as build artifact
buildkite-agent artifact upload packages/*/*

# Upload package to repo
popd
./enterprise/dev/ci/scripts/wolfi/upload-package.sh
