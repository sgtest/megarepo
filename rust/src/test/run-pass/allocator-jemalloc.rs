// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// no-prefer-dynamic
// ignore-windows no jemalloc on windows
// ignore-bitrig no jemalloc on bitrig
// ignore-openbsd no jemalloc on openbsd
// ignore-emscripten no jemalloc on emscripten

#![feature(alloc_jemalloc)]

extern crate alloc_jemalloc;

fn main() {
    println!("{:?}", Box::new(3));
}
