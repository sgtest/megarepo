// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![deny(unused_variables)]
#![deny(unused_assignments)]
#![allow(dead_code, non_camel_case_types, trivial_numeric_casts)]

fn f1(x: isize) {
    //~^ ERROR unused variable: `x`
}

fn f1b(x: &mut isize) {
    //~^ ERROR unused variable: `x`
}

#[allow(unused_variables)]
fn f1c(x: isize) {}

fn f1d() {
    let x: isize;
    //~^ ERROR unused variable: `x`
}

fn f2() {
    let x = 3;
    //~^ ERROR unused variable: `x`
}

fn f3() {
    let mut x = 3;
    //~^ ERROR variable `x` is assigned to, but never used
    x += 4;
    //~^ ERROR value assigned to `x` is never read
}

fn f3b() {
    let mut z = 3;
    //~^ ERROR variable `z` is assigned to, but never used
    loop {
        z += 4;
    }
}

#[allow(unused_variables)]
fn f3c() {
    let mut z = 3;
    loop { z += 4; }
}

#[allow(unused_variables)]
#[allow(unused_assignments)]
fn f3d() {
    let mut x = 3;
    x += 4;
}

fn f4() {
    match Some(3) {
      Some(i) => {
        //~^ ERROR unused variable: `i`
      }
      None => {}
    }
}

enum tri {
    a(isize), b(isize), c(isize)
}

fn f4b() -> isize {
    match tri::a(3) {
      tri::a(i) | tri::b(i) | tri::c(i) => {
        i
      }
    }
}

fn f5a() {
    for x in 1..10 { }
    //~^ ERROR unused variable: `x`
}

fn f5b() {
    for (x, _) in [1, 2, 3].iter().enumerate() { }
    //~^ ERROR unused variable: `x`
}

fn f5c() {
    for (_, x) in [1, 2, 3].iter().enumerate() {
    //~^ ERROR unused variable: `x`
        continue;
        drop(*x as i32); //~ WARNING unreachable statement
    }
}

fn main() {
}
