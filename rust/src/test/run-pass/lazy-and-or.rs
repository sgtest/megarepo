// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.



fn incr(x: &mut int) -> bool { *x += 1; fail_unless!((false)); return false; }

pub fn main() {
    let x = 1 == 2 || 3 == 3;
    fail_unless!((x));
    let mut y: int = 10;
    log(debug, x || incr(&mut y));
    fail_unless!((y == 10));
    if true && x { fail_unless!((true)); } else { fail_unless!((false)); }
}
