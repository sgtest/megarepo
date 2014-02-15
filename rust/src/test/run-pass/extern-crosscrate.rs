// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// ignore-fast
//aux-build:extern-crosscrate-source.rs

extern crate externcallback = "externcallback#0.1";

fn fact(n: uint) -> uint {
    unsafe {
        info!("n = {}", n);
        externcallback::rustrt::rust_dbg_call(externcallback::cb, n)
    }
}

pub fn main() {
    let result = fact(10u);
    info!("result = {}", result);
    assert_eq!(result, 3628800u);
}
