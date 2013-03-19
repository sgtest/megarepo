// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// xfail-test
fn function() -> &mut [int] {
    let mut x: &'static mut [int] = &[1,2,3];
    x[0] = 12345;
    x //~ ERROR bad
}

fn main() {
    let x = function();
    error!("%?", x);
}
