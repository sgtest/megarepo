// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// compile-flags: -Z no-landing-pads
// ignore-fast

use std::task;

static mut HIT: bool = false;

struct A;

impl Drop for A {
    fn drop(&mut self) {
        unsafe { HIT = true; }
    }
}

fn main() {
    task::try::<()>(proc() {
        let _a = A;
        fail!();
    });
    assert!(unsafe { !HIT });
}
