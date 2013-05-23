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
extern mod extra;
use extra::sync;
fn main() {
    let x = ~sync::RWlock();
    let mut y = None;
    do x.write_cond |cond| {
        y = Some(cond);
    }
    y.unwrap().wait();
}
