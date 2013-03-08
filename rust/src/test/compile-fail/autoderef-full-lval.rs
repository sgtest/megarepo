// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// error-pattern: mismatched types
struct clam {
    x: @int,
    y: @int,
}

struct fish {
    a: @int,
}

fn main() {
    let a: clam = clam{x: @1, y: @2};
    let b: clam = clam{x: @10, y: @20};
    let z: int = a.x + b.y;
    log(debug, z);
    fail_unless!((z == 21));
    let forty: fish = fish{a: @40};
    let two: fish = fish{a: @2};
    let answer: int = forty.a + two.a;
    log(debug, answer);
    fail_unless!((answer == 42));
}
