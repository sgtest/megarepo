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
fn len(v: ~[const int]) -> uint {
    let mut i = 0u;
    while i < vec::len(v) { i += 1u; }
    return i;
}

fn main() {
    let v0 = ~[1, 2, 3, 4, 5];
    log(debug, len(v0));
    let v1 = ~[mut 1, 2, 3, 4, 5];
    log(debug, len(v1));
}
