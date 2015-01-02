// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::num::ToPrimitive;

trait Add {
    fn to_int(&self) -> int;
    fn add_dynamic(&self, other: &Add) -> int;
}

impl Add for int {
    fn to_int(&self) -> int { *self }
    fn add_dynamic(&self, other: &Add) -> int {
        self.to_int() + other.to_int() //~ ERROR multiple applicable methods in scope
    }
}

fn main() { }
