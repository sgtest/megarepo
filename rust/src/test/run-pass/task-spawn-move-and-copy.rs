// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::task;

pub fn main() {
    let (p, ch) = Chan::<uint>::new();

    let x = ~1;
    let x_in_parent = &(*x) as *int as uint;

    task::spawn(proc() {
        let x_in_child = &(*x) as *int as uint;
        ch.send(x_in_child);
    });

    let x_in_child = p.recv();
    assert_eq!(x_in_parent, x_in_child);
}
