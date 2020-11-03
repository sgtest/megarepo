#!/usr/bin/env bash

cd "$(dirname "${BASH_SOURCE[0]}")/.."
set -euxo pipefail

box="$1"
pushd "test"

cleanup() {
  vagrant destroy -f "$box"
}

plugins=(vagrant-google vagrant-env vagrant-scp)
for i in "${plugins[@]}"; do
  if ! vagrant plugin list --no-tty | grep "$i"; then
    vagrant plugin install "$i"
  fi
done

trap cleanup EXIT
vagrant up "$box" --provider=google

vagrant scp "${box}:/sourcegraph/puppeteer/*.png" ../
vagrant scp "${box}:/sourcegraph/*.mp4" ../
vagrant scp "${box}:/sourcegraph/*.log" ../
