// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(managed_boxes)]
#![forbid(heap_memory)]
#![allow(dead_code)]

use std::gc::{Gc, GC};

struct Foo {
    x: Gc<int>, //~ ERROR type uses managed
}

struct Bar { x: Box<int> } //~ ERROR type uses owned

fn main() {
    let _x : Bar = Bar {x : box 10i}; //~ ERROR type uses owned

    box(GC) 2i; //~ ERROR type uses managed

    box 2i; //~ ERROR type uses owned
    fn g(_: Box<Clone>) {} //~ ERROR type uses owned
    proc() {}; //~ ERROR type uses owned
}
