#!/usr/bin/env bash

set -e
cd "$(dirname "${BASH_SOURCE[0]}")/.." # cd to repo root dir

# We'll exclude generating the CLI reference documentation by default due to the
# relatively high cost of fetching and building src-cli.
go list ./... | grep -v 'doc/cli/references' | xargs go generate -x

# Ignore the submodules in docker-images syntax-highlighter.
#
# Disable shellcheck for this line because we actually want them to be space separated
# (goimports doesn't accept passing args by stdin)
#
# shellcheck disable=SC2046
GOBIN="$PWD/.bin" go install golang.org/x/tools/cmd/goimports && \
    ./.bin/goimports -w $(
        comm -12 <(git ls-files | sort) <(find . -type f -name '*.go' -printf "%P\n" | sort)
    )

go mod tidy
