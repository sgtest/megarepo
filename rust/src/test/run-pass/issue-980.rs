// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


use std::cell::RefCell;
use std::gc::{Gc, GC};

enum maybe_pointy {
    no_pointy,
    yes_pointy(Gc<RefCell<Pointy>>),
}

struct Pointy {
    x: maybe_pointy
}

pub fn main() {
    let m = box(GC) RefCell::new(Pointy { x : no_pointy });
    *m.borrow_mut() = Pointy {
        x: yes_pointy(m)
    };
}
