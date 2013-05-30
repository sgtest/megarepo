// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::old_iter;

pub fn main() {
    let mut c = 0u;
    for [1u, 2u, 3u, 4u, 5u].eachi |i, v| {
        assert_eq!((i + 1u), *v);
        c += 1u;
    }
    assert_eq!(c, 5u);

    for old_iter::eachi(&None::<uint>) |i, v| { fail!(); }

    let mut c = 0u;
    for old_iter::eachi(&Some(1u)) |i, v| {
        assert_eq!((i + 1u), *v);
        c += 1u;
    }
    assert_eq!(c, 1u);
}
