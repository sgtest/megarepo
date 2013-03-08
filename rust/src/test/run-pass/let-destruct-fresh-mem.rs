// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

struct X { x: int, y: @A }
struct A { a: int }

pub fn main() {
    let u = X {x: 10, y: @A {a: 20}};
    let mut X {x: x, y: @A {a: a}} = u;
    x = 100;
    a = 100;
    fail_unless!((x == 100));
    fail_unless!((a == 100));
    fail_unless!((u.x == 10));
    fail_unless!((u.y.a == 20));
}
