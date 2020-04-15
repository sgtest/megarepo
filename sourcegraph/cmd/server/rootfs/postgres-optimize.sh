#!/usr/bin/env bash

set -euo pipefail

export PG_DATA="$1"

# This function allows us to start Postgres listening
# only on a UNIX socket. This is needed for intermediary upgrade operations
# to run without interference from external clients via TCP.
function pg() {
  pg_ctl -w -l "/var/lib/postgresql/pg_ctl.log" \
    -D "$PG_DATA" \
    -U "postgres" \
    -o "-p 5432 -c listen_addresses=''" \
    "$1"
}

pg start

# Apply post pg_upgrade fixes and optimizations.
if [ -e reindex_hash.sql ]; then
  echo "[INFO] Re-indexing hash based indexes"
  psql -U "postgres" -d postgres -f reindex_hash.sql
fi

echo "[INFO] Re-building optimizer statistics"
vacuumdb --all --analyze-in-stages

pg stop
