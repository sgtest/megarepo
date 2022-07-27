#!/usr/bin/env bash

set -e

echo "ENTERPRISE=$ENTERPRISE"
echo "NODE_ENV=$NODE_ENV"
echo "# Note: NODE_ENV only used for build command"

echo "--- Yarn install in root"
NODE_ENV='' ./dev/ci/yarn-install-with-retry.sh

cd "$1"
echo "--- browserslist"
NODE_ENV='' yarn -s run browserslist

echo "--- build"
yarn -s run build --color

# Only run bundlesize if intended and if there is valid a script provided in the relevant package.json
if [ "$CHECK_BUNDLESIZE" ] && jq -e '.scripts.bundlesize' package.json >/dev/null; then
  echo "--- bundlesize"
  yarn -s run bundlesize --enable-github-checks
fi
