// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// xfail-test
// xfail-fast
extern mod std;
use std::arc;
fn dispose(+_x: arc::ARC<bool>) unsafe { }

pub fn main() {
    let p = arc::arc(true);
    let x = Some(p);
    match move x {
        Some(move z) => { dispose(z); },
        None => die!()
    }
}
