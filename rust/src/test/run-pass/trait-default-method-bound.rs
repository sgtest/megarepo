// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[allow(default_methods)];

trait A {
    fn g() -> int { 10 }
}

impl A for int { }

fn f<T:A>(i: T) {
    fail_unless!(i.g() == 10);
}

pub fn main () {
    f(0);
}
