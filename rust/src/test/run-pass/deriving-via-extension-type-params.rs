// xfail-fast

// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[deriving_eq]
#[deriving_iter_bytes]
struct Foo<T> {
    x: int,
    y: T,
    z: int
}

pub fn main() {
    let a = Foo { x: 1, y: 2.0, z: 3 };
    let b = Foo { x: 1, y: 2.0, z: 3 };
    fail_unless!(a == b);
    fail_unless!(!(a != b));
    fail_unless!(a.eq(&b));
    fail_unless!(!a.ne(&b));
}

