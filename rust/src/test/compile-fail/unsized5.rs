// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Test `?Sized` types not allowed in fields (except the last one).

struct S1<X: ?Sized> {
    f1: X, //~ ERROR `core::marker::Sized` is not implemented
    f2: int,
}
struct S2<X: ?Sized> {
    f: int,
    g: X, //~ ERROR `core::marker::Sized` is not implemented
    h: int,
}
struct S3 {
    f: str, //~ ERROR `core::marker::Sized` is not implemented
    g: [uint]
}
struct S4 {
    f: str, //~ ERROR `core::marker::Sized` is not implemented
    g: uint
}
enum E<X: ?Sized> {
    V1(X, int), //~ERROR `core::marker::Sized` is not implemented
}
enum F<X: ?Sized> {
    V2{f1: X, f: int}, //~ERROR `core::marker::Sized` is not implemented
}

pub fn main() {
}
