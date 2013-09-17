// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// aux-build:issue-4208-cc.rs
// xfail-fast - Windows hates cross-crate tests

extern mod numeric;
use numeric::*;

fn foo<T, A:Angle<T>>(theta: A) -> T { sin(&theta) }

fn main() {}
