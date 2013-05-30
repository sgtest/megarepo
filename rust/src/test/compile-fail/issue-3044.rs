// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::vec;

fn main() {
    let needlesArr: ~[char] = ~['a', 'f'];
    do vec::foldr(needlesArr) |x, y| {
    }
    //~^ ERROR 2 parameters were supplied (including the closure passed by the `do` keyword)
    //
    // the first error is, um, non-ideal.
}
