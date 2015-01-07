// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

fn let_in<T, F>(x: T, f: F) where F: FnOnce(T) {}

fn main() {
    let_in(3u, |i| { assert!(i == 3is); });
    //~^ ERROR expected `usize`, found `isize`

    let_in(3i, |i| { assert!(i == 3us); });
    //~^ ERROR expected `isize`, found `usize`
}
