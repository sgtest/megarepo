// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[deriving_eq]
struct Bike {
    name: ~str,
}

pub fn main() {
    let town_bike = Bike { name: ~"schwinn" };
    let my_bike = Bike { name: ~"surly" };

    fail_unless!(town_bike != my_bike);
}
