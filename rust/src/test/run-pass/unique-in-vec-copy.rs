// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

pub fn main() {
    let mut a = ~[~10];
    let b = a.clone();

    assert!(*a[0] == 10);
    assert!(*b[0] == 10);

    // This should only modify the value in a, not b
    *a[0] = 20;

    assert!(*a[0] == 20);
    assert!(*b[0] == 10);
}
