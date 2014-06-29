// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[allow(unreachable_code)]
pub fn expr_while_23() {
    let mut x = 23i;
    let mut y = 23i;
    let mut z = 23i;

    while x > 0i {
        x -= 1i;

        while y > 0i {
            y -= 1i;

            while z > 0i { z -= 1i; }

            if x > 10i {
                return;
                "unreachable";
            }
        }
    }
}
