// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::cmp::PartialEq;

trait MyNum : Add<Self,Self> + Sub<Self,Self> + Mul<Self,Self> + PartialEq { }

#[deriving(Show)]
struct MyInt { val: int }

impl Add<MyInt, MyInt> for MyInt {
    fn add(&self, other: &MyInt) -> MyInt { mi(self.val + other.val) }
}

impl Sub<MyInt, MyInt> for MyInt {
    fn sub(&self, other: &MyInt) -> MyInt { mi(self.val - other.val) }
}

impl Mul<MyInt, MyInt> for MyInt {
    fn mul(&self, other: &MyInt) -> MyInt { mi(self.val * other.val) }
}

impl PartialEq for MyInt {
    fn eq(&self, other: &MyInt) -> bool { self.val == other.val }
    fn ne(&self, other: &MyInt) -> bool { !self.eq(other) }
}

impl MyNum for MyInt {}

fn f<T:MyNum>(x: T, y: T) -> (T, T, T) {
    return (x + y, x - y, x * y);
}

fn mi(v: int) -> MyInt { MyInt { val: v } }

pub fn main() {
    let (x, y) = (mi(3), mi(5));
    let (a, b, c) = f(x, y);
    assert_eq!(a, mi(8));
    assert_eq!(b, mi(-2));
    assert_eq!(c, mi(15));
}
