#!/usr/bin/env bash

cd "$(dirname "${BASH_SOURCE[0]}")"
set -euo pipefail

outdir="$(pwd)/data"
rm -rf "${outdir}"
mkdir "${outdir}"

versions=(
  v3.20.0 v3.20.1
  v3.21.0 v3.21.1 v3.21.2
  v3.22.0 v3.22.1
  v3.23.0
  v3.24.0 v3.24.1
  v3.25.0 v3.25.1 v3.25.2
  v3.26.0 v3.26.1 v3.26.2 v3.26.3
  v3.27.0 v3.27.1 v3.27.2 v3.27.3 v3.27.4 v3.27.5
  v3.28.0
  v3.29.0 v3.29.1
  v3.30.0 v3.30.1 v3.30.2 v3.30.3 v3.30.4
  v3.31.0 v3.31.1 v3.31.2
  v3.32.0 v3.32.1
  v3.33.0 v3.33.1 v3.33.2
  v3.34.0 v3.34.1 v3.34.2
  v3.35.0 v3.35.1 v3.35.2
  v3.36.0 v3.36.1 v3.36.2 v3.36.3
  v3.37.0
  v3.38.0 v3.38.1
  v3.39.0 v3.39.1
  v3.40.0 v3.40.1 v3.40.2
  v3.41.0 v3.41.1
)

for version in "${versions[@]}"; do
  ./generate.sh "${version}" "${outdir}"
done
