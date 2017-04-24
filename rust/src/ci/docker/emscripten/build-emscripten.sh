#!/bin/bash
# Copyright 2017 The Rust Project Developers. See the COPYRIGHT
# file at the top-level directory of this distribution and at
# http://rust-lang.org/COPYRIGHT.
#
# Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
# http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
# <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
# option. This file may not be copied, modified, or distributed
# except according to those terms.

set -ex

hide_output() {
  set +x
  on_err="
echo ERROR: An error was encountered with the build.
cat /tmp/build.log
exit 1
"
  trap "$on_err" ERR
  bash -c "while true; do sleep 30; echo \$(date) - building ...; done" &
  PING_LOOP_PID=$!
  $@ &> /tmp/build.log
  trap - ERR
  kill $PING_LOOP_PID
  rm /tmp/build.log
  set -x
}

curl https://s3.amazonaws.com/mozilla-games/emscripten/releases/emsdk-portable.tar.gz | \
    tar xzf -

# Some versions of the EMSDK archive have their contents in .emsdk-portable
# and others in emsdk_portable. Make sure the EMSDK ends up in a fixed path.
if [ -d emsdk-portable ]; then
    mv emsdk-portable emsdk_portable
fi

if [ ! -d emsdk_portable ]; then
    echo "ERROR: Invalid emsdk archive. Dumping working directory." >&2
    ls -l
    exit 1
fi

# Some versions of the EMSDK set the permissions of the root directory to
# 0700. Ensure the directory is readable by all users.
chmod 755 emsdk_portable

source emsdk_portable/emsdk_env.sh
hide_output emsdk update
hide_output emsdk install --build=Release sdk-tag-1.37.10-32bit
hide_output emsdk activate --build=Release sdk-tag-1.37.10-32bit
