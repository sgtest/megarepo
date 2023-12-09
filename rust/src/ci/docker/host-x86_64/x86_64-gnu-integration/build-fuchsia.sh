#!/usr/bin/env bash

# Downloads and builds the Fuchsia operating system using a toolchain installed
# in $RUST_INSTALL_DIR.

set -euf -o pipefail

INTEGRATION_SHA=66793c4894bf6204579bbee3b79956335f31c768
PICK_REFS=()

checkout=fuchsia
jiri=.jiri_root/bin/jiri

set -x

# This script will:
# - create a directory named "fuchsia" if it does not exist
# - download "jiri" to "fuchsia/.jiri_root/bin"
curl -s "https://fuchsia.googlesource.com/jiri/+/HEAD/scripts/bootstrap_jiri?format=TEXT" \
    | base64 --decode \
    | bash -s $checkout

cd $checkout

$jiri init \
    -partial=true \
    -analytics-opt=false \
    .

$jiri import \
    -name=integration \
    -revision=$INTEGRATION_SHA \
    -overwrite=true \
    flower \
    "https://fuchsia.googlesource.com/integration"

if [ -d ".git" ]; then
    # Wipe out any local changes if we're reusing a checkout.
    git checkout --force JIRI_HEAD
fi

$jiri update -autoupdate=false

echo integration commit = $(git -C integration rev-parse HEAD)

for git_ref in "${PICK_REFS[@]}"; do
    git fetch https://fuchsia.googlesource.com/fuchsia $git_ref
    git cherry-pick --no-commit FETCH_HEAD
done

bash scripts/rust/build_fuchsia_from_rust_ci.sh
