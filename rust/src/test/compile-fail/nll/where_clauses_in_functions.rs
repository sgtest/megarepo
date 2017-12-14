// Copyright 2017 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// compile-flags: -Z borrowck=mir -Z nll

#![allow(dead_code)]

fn foo<'a, 'b>(x: &'a u32, y: &'b u32) -> (&'a u32, &'b u32)
where
    'a: 'b,
{
    (x, y)
}

fn bar<'a, 'b>(x: &'a u32, y: &'b u32) -> (&'a u32, &'b u32) {
    foo(x, y)
    //~^ ERROR free region `'_#1r` does not outlive free region `'_#2r`
    //~| WARNING not reporting region error due to -Znll
}

fn main() {}
