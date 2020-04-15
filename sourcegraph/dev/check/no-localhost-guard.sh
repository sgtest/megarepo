#!/bin/bash

echo "--- no localhost guard"

cd "$(dirname "${BASH_SOURCE[0]}")/../.."

path_filter() {
  local IFS=
  local withPath="${*/#/ -o -path }"
  echo "${withPath# -o }"
}

LOCALHOST_MATCHES=$(git grep -e localhost --and --not -e '^\s*//' --and --not -e 'CI\:LOCALHOST_OK' -- '*.go' \
  ':(exclude)*_test.go' \
  ':(exclude)cmd/server/shared/nginx.go' \
  ':(exclude)pkg/conf/confdefaults' \
  ':(exclude)schema' \
  ':(exclude)vendor')

if [ ! -z "$LOCALHOST_MATCHES" ]; then
  echo
  echo "Error: Found instances of \"localhost\":"
  echo "$LOCALHOST_MATCHES" | sed 's/^/  /'

  cat <<EOF

We generally prefer to use "127.0.0.1" instead of "localhost", because
the Go DNS resolver fails to resolve "localhost" correctly in some
situations (see https://github.com/sourcegraph/issues/issues/34 and
https://github.com/sourcegraph/sourcegraph/issues/9129).

If your usage of "localhost" is valid, then either
1) add the comment "CI:LOCALHOST_OK" to the line where "localhost" occurs, or
2) add an exclusion clause in the "git grep" command in  no-localhost-guard.sh

EOF

  exit 1
fi
