#!/usr/bin/env bash

echo "--- golangci-lint"
trap 'rm -f "$TMPFILE"' EXIT
set -e
TMPFILE=$(mktemp)

echo "0" >"$TMPFILE"

cd "$(dirname "${BASH_SOURCE[0]}")/../.."

export GOBIN="$PWD/.bin"
export PATH=$GOBIN:$PATH
export GO111MODULE=on

config_file="$(pwd)/.golangci.yml"
lint_script="$(pwd)/dev/golangci-lint.sh"

run() {
  LINTER_ARG=${1}

  set +e
  OUT=$("$lint_script" --config "$config_file" run "$LINTER_ARG")
  EXIT_CODE=$?
  set -e

  echo -e "$OUT"

  if [ $EXIT_CODE -ne 0 ]; then
    # We want to return after running all tests, we don't want to fail fast, so
    # we store the EXIT_CODE (in a tmp file as this is running in a sub-shell).
    echo "$EXIT_CODE" >"$TMPFILE"
    echo -e "$OUT" >./annotations/go-lint
    echo "^^^ +++"
  fi
}

# If no args are given, traverse through each project with a `go.mod`
if [ $# -eq 0 ]; then
  find . -name go.mod -exec dirname '{}' \; | while read -r d; do
    pushd "$d" >/dev/null

    echo "--- golangci-lint $d"

    run "./..."

    popd >/dev/null
  done
else
  run "$@"
fi

read -r EXIT_CODE <"$TMPFILE"
exit "$EXIT_CODE"
