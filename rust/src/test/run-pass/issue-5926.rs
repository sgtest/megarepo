// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[allow(unused_mut)];

pub fn main() {
    let  mut your_favorite_numbers = @[1,2,3];
    let  mut my_favorite_numbers = @[4,5,6];
    let  f = your_favorite_numbers + my_favorite_numbers;
    println(fmt!("The third favorite number is %?.", f))
}

