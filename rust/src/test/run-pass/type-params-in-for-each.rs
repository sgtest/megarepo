// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

struct S<T> {
    a: T,
    b: uint,
}

fn range(lo: uint, hi: uint, it: fn(uint)) {
    let mut lo_ = lo;
    while lo_ < hi { it(lo_); lo_ += 1u; }
}

fn create_index<T>(index: ~[S<T>], hash_fn: extern fn(T) -> uint) {
    range(0u, 256u, |_i| { let bucket: ~[T] = ~[]; } )
}

pub fn main() { }
