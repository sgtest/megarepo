// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.




// -*- rust -*-
struct Point {x: int, y: int, mut z: int}

fn f(p: @Point) { assert (p.z == 12); p.z = 13; assert (p.z == 13); }

fn main() {
    let a: Point = Point {x: 10, y: 11, mut z: 12};
    let b: @Point = @copy a;
    assert (b.z == 12);
    f(b);
    assert (a.z == 12);
    assert (b.z == 13);
}
