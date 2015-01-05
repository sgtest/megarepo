// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Tests that a closure which requires mutable access to the referent
// of an `&mut` requires a "unique" borrow -- that is, the variable to
// be borrowed (here, `x`) will not be borrowed *mutably*, but
//  may be *immutable*, but we cannot allow
// multiple borrows.

fn get(x: &int) -> int {
    *x
}

fn set(x: &mut int) -> int {
    *x
}

fn a(x: &mut int) {
    let c1 = |&mut:| get(x);
    let c2 = |&mut:| get(x);
}

fn b(x: &mut int) {
    let c1 = |&mut:| get(x);
    let c2 = |&mut:| set(x); //~ ERROR closure requires unique access to `x`
}

fn c(x: &mut int) {
    let c1 = |&mut:| get(x);
    let c2 = |&mut:| { get(x); set(x); }; //~ ERROR closure requires unique access to `x`
}

fn d(x: &mut int) {
    let c1 = |&mut:| set(x);
    let c2 = |&mut:| set(x); //~ ERROR closure requires unique access to `x`
}

fn e(x: &mut int) {
    let c1 = |&mut:| x = panic!(); //~ ERROR closure cannot assign to immutable local variable
}

fn main() {
}
