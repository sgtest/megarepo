// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Tests that are conservative around thin/fat pointer mismatches.

#![allow(dead_code)]

use std::mem::transmute;

fn a<T, Sized? U>(x: &[T]) -> &U {
    unsafe { transmute(x) } //~ ERROR transmute called on types with potentially different sizes
}

fn b<Sized? T, Sized? U>(x: &T) -> &U {
    unsafe { transmute(x) } //~ ERROR transmute called on types with potentially different sizes
}

fn c<T, U>(x: &T) -> &U {
    unsafe { transmute(x) }
}

fn d<T, U>(x: &[T]) -> &[U] {
    unsafe { transmute(x) }
}

fn e<Sized? T, U>(x: &T) -> &U {
    unsafe { transmute(x) } //~ ERROR transmute called on types with potentially different sizes
}

fn f<T, Sized? U>(x: &T) -> &U {
    unsafe { transmute(x) } //~ ERROR transmute called on types with potentially different sizes
}

fn main() { }
