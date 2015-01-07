// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(unsafe_destructor)]

trait X {
    fn call<T: std::fmt::Show>(&self, x: &T);
    fn default_method<T: std::fmt::Show>(&self, x: &T) {
        println!("X::default_method {:?}", x);
    }
}

#[derive(Show)]
struct Y(int);

#[derive(Show)]
struct Z<T> {
    x: T
}

impl X for Y {
    fn call<T: std::fmt::Show>(&self, x: &T) {
        println!("X::call {:?} {:?}", self, x);
    }
}

#[unsafe_destructor]
impl<T: X + std::fmt::Show> Drop for Z<T> {
    fn drop(&mut self) {
        // These statements used to cause an ICE.
        self.x.call(self);
        self.x.default_method(self);
    }
}

pub fn main() {
    let _z = Z {x: Y(42)};
}
