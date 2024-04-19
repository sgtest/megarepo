#!/usr/bin/env bash

set -e

RUNNER="$1"

# internal/database/. artifacts are being loaded as arguments, 13 is the beginning of passed arguments to the cli tool
# Args:
# bazel-bin/testing/tools/upgradetest/sh_upgradetest_run
# testing/tools/upgradetest/upgradetest-darwin-arm64
# cmd/migrator/image_tarball.sh
# cmd/frontend/image_tarball.sh
# internal/database/_codeinsights_squashed.sql
# internal/database/_codeintel_squashed.sql
# internal/database/_frontend_squashed.sql
# internal/database/_schema.codeinsights.json
# internal/database/_schema.codeinsights.md
# internal/database/_schema.codeintel.json
# internal/database/_schema.codeintel.md
# internal/database/_schema.json
# internal/database/_schema.md
"$RUNNER" "${@:2}"
