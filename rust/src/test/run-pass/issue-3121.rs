// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


use std::gc::{Gc, GC};

enum side { mayo, catsup, vinegar }
enum order { hamburger, fries(side), shake }
enum meal { to_go(order), for_here(order) }

fn foo(m: Gc<meal>, cond: bool) {
    match *m {
      to_go(_) => { }
      for_here(_) if cond => {}
      for_here(hamburger) => {}
      for_here(fries(_s)) => {}
      for_here(shake) => {}
    }
}

pub fn main() {
    foo(box(GC) for_here(hamburger), true)
}
