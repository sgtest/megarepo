// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

trait t1 : t2 {
//~^ NOTE the cycle begins when computing the supertraits of `t1`...
//~| NOTE ...which then requires computing the supertraits of `t2`...
}

trait t2 : t1 {
//~^ ERROR unsupported cyclic reference between types/traits detected
//~| cyclic reference
//~| NOTE ...which then again requires computing the supertraits of `t1`, completing the cycle
}

fn main() { }
