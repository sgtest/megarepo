#!/bin/sh
#
# ignore-tidy-linelength

set -ex

# Originally from https://releases.llvm.org/8.0.0/clang+llvm-8.0.0-x86_64-linux-gnu-ubuntu-14.04.tar.xz
curl https://s3-us-west-1.amazonaws.com/rust-lang-ci2/rust-ci-mirror/clang%2Bllvm-8.0.0-x86_64-linux-gnu-ubuntu-14.04.tar.xz | \
  tar xJf -
export PATH=`pwd`/clang+llvm-8.0.0-x86_64-linux-gnu-ubuntu-14.04/bin:$PATH

git clone https://github.com/CraneStation/wasi-sysroot

cd wasi-sysroot
git reset --hard 320054e84f8f2440def3b1c8700cedb8fd697bf8
make -j$(nproc) INSTALL_DIR=/wasm32-unknown-wasi install

cd ..
rm -rf reference-sysroot-wasi
rm -rf clang+llvm*
