// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// compile-flags: -Z parse-only

// For style and consistency reasons, non-parametrized enum variants must
// be used simply as `ident` instead of `ident ()`.
// This test-case covers enum declaration.

enum Foo {
    Bar(), //~ ERROR nullary enum variants are written with no trailing `( )`
    Baz(), //~ ERROR nullary enum variants are written with no trailing `( )`
    Bazar
}

fn main() {
    println!("{}", match Bar { Bar => 1, Baz => 2, Bazar => 3 })
}
