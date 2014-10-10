// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Issue #8380

#![feature(globs)]

use std::sync::atomic::*;
use std::ptr;

fn main() {
    let x = INIT_ATOMIC_BOOL;
    let x = *&x; //~ ERROR: cannot move out of dereference
    let x = INIT_ATOMIC_INT;
    let x = *&x; //~ ERROR: cannot move out of dereference
    let x = INIT_ATOMIC_UINT;
    let x = *&x; //~ ERROR: cannot move out of dereference
    let x: AtomicPtr<uint> = AtomicPtr::new(ptr::null_mut());
    let x = *&x; //~ ERROR: cannot move out of dereference
    let x: AtomicOption<uint> = AtomicOption::empty();
    let x = *&x; //~ ERROR: cannot move out of dereference
}
