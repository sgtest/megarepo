#!/usr/bin/env bash
set -e

[ -z "$PUBLISH_TOKEN" ] && echo "You must set a \$PUBLISH_TOKEN before running this script. You can generate a token in the JetBrains marketplace." && exit 1

# Ensure we have a clean git checkout
git diff-index --quiet HEAD || (echo "Please commit your changes before releasing" && exit 1)

# Make sure we have all dependencies
pushd "../.." > /dev/null
yarn && yarn generate
popd > /dev/null || exit

# Build the JavaScript artifacts
yarn build

# Build the release candidate and publish it onto the registry
./gradlew publishPlugin

# The release script automatically changes the README and moves the unreleased
# section into a version numbered one. We don't care about this while we are
# creating pre-release versions.
if grep -q "alpha\|beta" "gradle.properties"; then
  git restore CHANGELOG.md
fi
