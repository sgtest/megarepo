#!/bin/sh
# Copyright 2016 The Rust Project Developers. See the COPYRIGHT
# file at the top-level directory of this distribution and at
# http://rust-lang.org/COPYRIGHT.
#
# Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
# http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
# <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
# option. This file may not be copied, modified, or distributed
# except according to those terms.

set -ex

# We need to mitigate rust-lang/rust#34978 when compiling musl itself as well
export CFLAGS="-fPIC -Wa,-mrelax-relocations=no"
export CXXFLAGS="-Wa,-mrelax-relocations=no"

MUSL=musl-1.1.16
curl https://www.musl-libc.org/releases/$MUSL.tar.gz | tar xzf -
cd $MUSL
CC=gcc \
  CFLAGS="$CFLAGS -m32" \
  ./configure --prefix=/musl-i686 --disable-shared \
    --target=i686
make AR=ar RANLIB=ranlib -j10
make install
cd ..

# To build MUSL we're going to need a libunwind lying around, so acquire that
# here and build it.
curl -L https://github.com/llvm-mirror/llvm/archive/release_37.tar.gz | tar xzf -
curl -L https://github.com/llvm-mirror/libunwind/archive/release_37.tar.gz | tar xzf -

# Whoa what's this mysterious patch we're applying to libunwind! Why are we
# swapping the values of ESP/EBP in libunwind?!
#
# Discovered in #35599 it turns out that the vanilla build of libunwind is not
# suitable for unwinding 32-bit musl. After some investigation it ended up
# looking like the register values for ESP/EBP were indeed incorrect (swapped)
# in the source. Similar commits in libunwind (r280099 and r282589) have noticed
# this for other platforms, and we just need to realize it for musl linux as
# well.
#
# More technical info can be found at #35599
cd libunwind-release_37
patch -Np1 < /build/musl-libunwind-patch.patch
cd ..

mkdir libunwind-build
cd libunwind-build
CFLAGS="$CFLAGS -m32" CXXFLAGS="$CXXFLAGS -m32" cmake ../libunwind-release_37 \
          -DLLVM_PATH=/build/llvm-release_37 \
          -DLIBUNWIND_ENABLE_SHARED=0
make -j10
cp lib/libunwind.a /musl-i686/lib
