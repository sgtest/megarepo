// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

fn f(mut y: ~int) {
    *y = 5;
    assert *y == 5;
}

fn g() {
    let frob: fn(~int) = |mut q| { *q = 2; assert *q == 2; };
    let w = ~37;
    frob(w);

}

fn main() {
    let z = ~17;
    f(z);
    g();
}