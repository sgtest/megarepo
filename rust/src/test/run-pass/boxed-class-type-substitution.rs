// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Regression test that rustc doesn't recurse infinitely substituting
// the boxed type parameter


use std::gc::Gc;

struct Tree<T> {
    parent: Option<T>
}

fn empty<T>() -> Tree<T> { fail!() }

struct Box {
    tree: Tree<Gc<Box>>
}

fn Box() -> Box {
    Box {
        tree: empty()
    }
}

struct LayoutData {
    a_box: Option<Gc<Box>>
}

pub fn main() { }
