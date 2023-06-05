#!/usr/bin/env bash

# This file is called by nix-shell when setting up the shell. It is
# responsible for setting up the development environment outside of what nix's
# package management.
#
# The main goal of this is to start stateful services which aren't managed by
# sourcegraph's developer tools. In particular this is our databases, which
# are used by both our tests and development server.

pushd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null || exit

. ./start-postgres.sh
. ./start-redis.sh

# We disable postgres_exporter since it expects postgres to be running on TCP.
export SRC_DEV_EXCEPT="${SRC_DEV_EXCEPT:-postgres_exporter}"

popd >/dev/null || exit

# We run this check afterwards so we can read the values exported by the
# start-*.sh scripts. We need to smuggle in these envvars for tests.
if [ -f /etc/NIXOS ]; then
  cat <<EOF > .bazelrc-nix
build --extra_toolchains=@zig_sdk//toolchain:linux_amd64_gnu.2.34
build --action_env=PATH=$BAZEL_ACTION_PATH
build --action_env=REDIS_ENDPOINT
build --action_env=PGHOST
build --action_env=PGDATA
build --action_env=PGDATABASE
build --action_env=PGDATASOURCE
build --action_env=PGUSER
EOF
fi
