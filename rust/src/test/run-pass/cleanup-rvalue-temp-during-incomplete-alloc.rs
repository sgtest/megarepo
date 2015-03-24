// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Test cleanup of rvalue temporary that occurs while `box` construction
// is in progress. This scenario revealed a rather terrible bug.  The
// ingredients are:
//
// 1. Partial cleanup of `box` is in scope,
// 2. cleanup of return value from `get_bar()` is in scope,
// 3. do_it() panics.
//
// This led to a bug because `the top-most frame that was to be
// cleaned (which happens to be the partial cleanup of `box`) required
// multiple basic blocks, which led to us dropping part of the cleanup
// from the top-most frame.
//
// It's unclear how likely such a bug is to recur, but it seems like a
// scenario worth testing.

// pretty-expanded FIXME #23616

#![allow(unknown_features)]
#![feature(box_syntax)]

use std::thread;

enum Conzabble {
    Bickwick(Foo)
}

struct Foo { field: Box<uint> }

fn do_it(x: &[uint]) -> Foo {
    panic!()
}

fn get_bar(x: uint) -> Vec<uint> { vec!(x * 2) }

pub fn fails() {
    let x = 2;
    let mut y: Vec<Box<_>> = Vec::new();
    y.push(box Conzabble::Bickwick(do_it(&get_bar(x))));
}

pub fn main() {
    thread::spawn(fails).join();
}
