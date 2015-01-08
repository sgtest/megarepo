// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Test that immutable pattern bindings cannot be reassigned.

enum E {
    Foo(isize)
}

struct S {
    bar: isize,
}

pub fn main() {
    match 1is {
        x => {
            x += 1; //~ ERROR re-assignment of immutable variable `x`
        }
    }

    match E::Foo(1) {
        E::Foo(x) => {
            x += 1; //~ ERROR re-assignment of immutable variable `x`
        }
    }

    match (S { bar: 1 }) {
        S { bar: x } => {
            x += 1; //~ ERROR re-assignment of immutable variable `x`
        }
    }

    match (1is,) {
        (x,) => {
            x += 1; //~ ERROR re-assignment of immutable variable `x`
        }
    }

    match [1is,2,3] {
        [x,_,_] => {
            x += 1; //~ ERROR re-assignment of immutable variable `x`
        }
    }
}
