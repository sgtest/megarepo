#!/usr/bin/env bash

set -euo pipefail

function usage {
  cat <<EOF
Usage: go-test.sh [only|exclude package-path-1 package-path-2 ...]

Run go tests, optionally restricting which ones based on the only and exclude coommands.

EOF
}

function go_test() {
  local test_packages
  test_packages="$1"
  local tmpfile
  tmpfile=$(mktemp)
  # Interpolate tmpfile right now, so the trap set by the function
  # always work, even if ran outside the function body.
  # shellcheck disable=SC2064
  trap "rm \"$tmpfile\"" EXIT

  set +eo pipefail # so we still get the result if the test failed
  local test_exit_code
  # shellcheck disable=SC2086
  go test \
    -timeout 10m \
    -coverprofile=coverage.txt \
    -covermode=atomic \
    -race \
    -v \
    $test_packages | tee "$tmpfile" | richgo testfilter
  # Save the test exit code so we can return it after saving the test report
  test_exit_code="${PIPESTATUS[0]}"
  set -eo pipefail # resume being strict about errors

  mkdir -p './test-reports'
  go-junit-report <"$tmpfile" >>./test-reports/go-test-junit.xml

  return "$test_exit_code"
}

if [ "$1" == "-h" ]; then
  usage
  exit 1
fi

if [ -n "$1" ]; then
  FILTER_ACTION=$1
  shift
  FILTER_TARGETS=$*
fi

# Display to the user what kind of filtering is happening here
if [ -n "$FILTER_ACTION" ]; then
  echo -e "--- :information_source: \033[0;34mFiltering go tests: $FILTER_ACTION $FILTER_TARGETS\033[0m"
fi

# Buildkite analytics

# https://github.com/sourcegraph/sourcegraph/issues/28469
# TODO is that the best way to handle this?
go install github.com/jstemmer/go-junit-report@latest
asdf reshim golang

# For searcher
echo "--- comby install"
./dev/comby-install-or-upgrade.sh

# For code insights test
./dev/codeinsights-db.sh &
export CODEINSIGHTS_PGDATASOURCE=postgres://postgres:password@127.0.0.1:5435/postgres
export DB_STARTUP_TIMEOUT=360s # codeinsights-db needs more time to start in some instances.

# Disable GraphQL logs which are wildly noisy
export NO_GRAPHQL_LOG=true

# Install richgo for better output
go install github.com/kyoh86/richgo@latest
asdf reshim golang

# Used to ignore directories (for example, when using submodules)
#   (It appears to be unused, but it's actually used doing -v below)
#
# shellcheck disable=SC2034
declare -A IGNORED_DIRS=(
  ["./docker-images/syntax-highlighter"]=1
)

# We have multiple go.mod files and go list doesn't recurse into them.
find . -name go.mod -exec dirname '{}' \; | while read -r d; do

  # Skip any ignored directories.
  if [ -v "IGNORED_DIRS[$d]" ]; then
    continue
  fi

  pushd "$d" >/dev/null

  # Separate out time for go mod from go test
  echo "--- $d go mod download"
  go mod download

  patterns="${FILTER_TARGETS[*]// /\\|}" # replace spaces with \| to have multiple patterns being matched
  case "$FILTER_ACTION" in
    exclude)
      TEST_PACKAGES=$(go list ./... | { grep -v "$patterns" || true; }) # -v to reject
      if [ -n "$TEST_PACKAGES" ]; then
        echo "+++ $d go test"
        go_test "$TEST_PACKAGES"
      else
        echo "~~~ $d go test (skipping)"
      fi
      ;;
    only)
      TEST_PACKAGES=$(go list ./... | { grep "$patterns" || true; }) # select only what we need
      if [ -n "$TEST_PACKAGES" ]; then
        echo "+++ $d go test"
        go_test "$TEST_PACKAGES"
      else
        echo "~~~ $d go test (skipping)"
      fi
      ;;
    *)
      TEST_PACKAGES="./..."
      echo "+++ $d go test"
      go_test "$TEST_PACKAGES"
      ;;
  esac

  popd >/dev/null
done
