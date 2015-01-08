// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(unboxed_closures, overloaded_calls)]

// Tests that we can't assign to or mutably borrow upvars from `Fn`
// closures (issue #17780)

fn set(x: &mut usize) { *x = 5; }

fn main() {
    // By-ref captures
    {
        let mut x = 0us;
        let _f = |&:| x = 42; //~ ERROR cannot assign

        let mut y = 0us;
        let _g = |&:| set(&mut y); //~ ERROR cannot borrow

        let mut z = 0us;
        let _h = |&mut:| { set(&mut z); |&:| z = 42; }; //~ ERROR cannot assign
    }
    // By-value captures
    {
        let mut x = 0us;
        let _f = move |&:| x = 42; //~ ERROR cannot assign

        let mut y = 0us;
        let _g = move |&:| set(&mut y); //~ ERROR cannot borrow

        let mut z = 0us;
        let _h = move |&mut:| { set(&mut z); move |&:| z = 42; }; //~ ERROR cannot assign
    }
}
