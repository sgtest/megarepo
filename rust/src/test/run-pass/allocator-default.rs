// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(alloc_jemalloc, alloc_system)]

#[cfg(not(any(target_env = "msvc", target_os = "bitrig", target_os = "openbsd")))]
extern crate alloc_jemalloc;
#[cfg(any(target_env = "msvc", target_os = "bitrig", target_os = "openbsd"))]
extern crate alloc_system;

fn main() {
    println!("{:?}", Box::new(3));
}
