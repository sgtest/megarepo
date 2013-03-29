// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

static x : [int, ..4] = [1,2,3,4];
static y : &'static [int] = &[1,2,3,4];

pub fn main() {
    io::println(fmt!("%?", x[1]));
    io::println(fmt!("%?", y[1]));
    assert!(x[1] == 2);
    assert!(x[3] == 4);
    assert!(x[3] == y[3]);
}
