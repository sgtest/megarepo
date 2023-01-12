#!/usr/bin/env bash

set -e

echo "--- Download pre-built client artifact"
buildkite-agent artifact download 'client.tar.gz' . --step 'puppeteer:prep'
tar -xf client.tar.gz -C .

echo "--- Pnpm install in root"
./dev/ci/pnpm-install-with-retry.sh

echo "--- Run integration test suite"
pnpm percy exec --quiet -- pnpm _cover-integration "$@"

echo "--- Process NYC report"
pnpm nyc report -r json

echo "--- Upload coverage report"
dev/ci/codecov.sh -c -F typescript -F integration
