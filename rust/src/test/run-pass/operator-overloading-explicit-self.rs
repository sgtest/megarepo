// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

struct S {
    x: int
}

pub impl S {
    pure fn add(&self, other: &S) -> S {
        S { x: self.x + other.x }
    }
}

pub fn main() {
    let mut s = S { x: 1 };
    s += S { x: 2 };
    fail_unless!(s.x == 3);
}

