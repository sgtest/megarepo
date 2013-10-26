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
    let mut x: u8 = 12u8;
    let y: u8 = 12u8;
    x = x + 1u8;
    x = x - 1u8;
    assert_eq!(x, y);
    // x = 14u8;
    // x = x + 1u8;

}
