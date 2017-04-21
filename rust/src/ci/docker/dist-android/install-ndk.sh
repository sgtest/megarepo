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

# Prep the Android NDK
#
# See https://github.com/servo/servo/wiki/Building-for-Android
curl -O https://dl.google.com/android/repository/android-ndk-r11c-linux-x86_64.zip
unzip -q android-ndk-r11c-linux-x86_64.zip
bash android-ndk-r11c/build/tools/make-standalone-toolchain.sh \
        --platform=android-9 \
        --toolchain=arm-linux-androideabi-4.9 \
        --install-dir=/android/ndk-arm-9 \
        --ndk-dir=/android/android-ndk-r11c \
        --arch=arm
bash android-ndk-r11c/build/tools/make-standalone-toolchain.sh \
        --platform=android-21 \
        --toolchain=aarch64-linux-android-4.9 \
        --install-dir=/android/ndk-arm64-21 \
        --ndk-dir=/android/android-ndk-r11c \
        --arch=arm64
bash android-ndk-r11c/build/tools/make-standalone-toolchain.sh \
        --platform=android-9 \
        --toolchain=x86-4.9 \
        --install-dir=/android/ndk-x86-9 \
        --ndk-dir=/android/android-ndk-r11c \
        --arch=x86
bash android-ndk-r11c/build/tools/make-standalone-toolchain.sh \
        --platform=android-21 \
        --toolchain=x86_64-4.9 \
        --install-dir=/android/ndk-x86_64-21 \
        --ndk-dir=/android/android-ndk-r11c \
        --arch=x86_64

rm -rf ./android-ndk-r11c-linux-x86_64.zip ./android-ndk-r11c
