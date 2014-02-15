// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

extern crate sync;
use sync::RWArc;
fn main() {
    let x = ~RWArc::new(1);
    let mut y = None;
    x.write_downgrade(|write_mode| {
        y = Some(x.downgrade(write_mode));
        //~^ ERROR cannot infer
    });
    y.unwrap();
    // Adding this line causes a method unification failure instead
    // (&option::unwrap(y)).read(|state| { assert!(*state == 1); })
}
