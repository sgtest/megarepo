#!/bin/bash

# To compile all protobuf files in this repository, run
# "make protobuf" at the top-level.

set -eu

DST_DIR=../genproto/pluginv2

SOURCE="${BASH_SOURCE[0]}"
while [ -h "$SOURCE" ] ; do SOURCE="$(readlink "$SOURCE")"; done
DIR="$( cd -P "$( dirname "$SOURCE" )" && pwd )"

cd "$DIR"

protoc --go_out=. --go_opt=paths=source_relative \
    --go-grpc_out=. --go-grpc_opt=paths=source_relative \
    *.proto