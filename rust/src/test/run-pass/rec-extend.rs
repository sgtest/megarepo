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
struct Point {x: int, y: int}

pub fn main() {
    let origin: Point = Point {x: 0, y: 0};
    let right: Point = Point {x: origin.x + 10,.. origin};
    let up: Point = Point {y: origin.y + 10,.. origin};
    fail_unless!((origin.x == 0));
    fail_unless!((origin.y == 0));
    fail_unless!((right.x == 10));
    fail_unless!((right.y == 0));
    fail_unless!((up.x == 0));
    fail_unless!((up.y == 10));
}
