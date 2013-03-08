// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// error-pattern: reference is not valid outside of its lifetime
extern mod std;
use std::arc;
fn main() {
    let x = ~arc::RWARC(1);
    let mut y = None;
    do x.write_downgrade |write_mode| {
        y = Some(write_mode);
    }
    // Adding this line causes a method unification failure instead
    // do (&option::unwrap(y)).write |state| { fail_unless!(*state == 1); }
}
