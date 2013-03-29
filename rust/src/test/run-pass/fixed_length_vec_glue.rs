// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

struct Struc { a: u8, b: [int, ..3], c: int }

pub fn main() {
    let arr = [1,2,3];
    let struc = Struc {a: 13u8, b: arr, c: 42};
    let s = sys::log_str(&struc);
    assert!((s == ~"{a: 13, b: [1, 2, 3], c: 42}"));
}
