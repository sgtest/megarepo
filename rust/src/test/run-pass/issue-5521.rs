// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// aux-build:issue-5521.rs
// ignore-fast

#[feature(managed_boxes)];

extern crate foo = "issue-5521";

fn foo(a: foo::map) {
    if false {
        fail!();
    } else {
        let _b = a.get(&2);
    }
}

fn main() {}
