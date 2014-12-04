// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Tests that unsafe extern fn pointers do not implement any Fn traits.

#![feature(unboxed_closures)]

use std::ops::{Fn,FnMut,FnOnce};

unsafe fn square(x: &int) -> int { (*x) * (*x) }

fn call_it<F:Fn(&int)->int>(_: &F, _: int) -> int { 0 }
fn call_it_mut<F:FnMut(&int)->int>(_: &mut F, _: int) -> int { 0 }
fn call_it_once<F:FnOnce(&int)->int>(_: F, _: int) -> int { 0 }

fn main() {
    let x = call_it(&square, 22); //~ ERROR not implemented
    let y = call_it_mut(&mut square, 22); //~ ERROR not implemented
    let z = call_it_once(square, 22); //~ ERROR not implemented
}

