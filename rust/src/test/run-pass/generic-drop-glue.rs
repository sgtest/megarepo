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

use std::gc::{Gc, GC};

struct Pair { x: Gc<int>, y: Gc<int> }

fn f<T>(t: T) { let _t1: T = t; }

pub fn main() { let x = Pair {x: box(GC) 10, y: box(GC) 12}; f(x); }
