// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

extern fn f() {
}

extern fn g() {
}

pub fn main() {
    // extern functions are *u8 types
    let a: *u8 = f;
    let b: *u8 = f;
    let c: *u8 = g;

    fail_unless!(a == b);
    fail_unless!(a != c);
}
