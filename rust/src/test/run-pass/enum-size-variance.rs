// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
//
#![warn(variant_size_differences)]
#![allow(dead_code)]

enum Enum1 { }

enum Enum2 { A, B, C }

enum Enum3 { D(isize), E, F }

enum Enum4 { H(isize), I(isize), J }

enum Enum5 {
    L(isize, isize, isize, isize), //~ WARNING three times larger
    M(isize),
    N
}

enum Enum6<T, U> {
    O(T),
    P(U),
    Q(isize)
}

#[allow(variant_size_differences)]
enum Enum7 {
    R(isize, isize, isize, isize),
    S(isize),
    T
}
pub fn main() { }
