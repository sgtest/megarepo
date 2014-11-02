// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
#![feature(globs, struct_variant)]

pub use Foo::*;

pub enum Foo {
    A,
    B(int),
    C { a: int },
}

impl Foo {
    pub fn foo() {}
}

pub mod nest {
    pub use self::Bar::*;

    pub enum Bar {
        D,
        E(int),
        F { a: int },
    }

    impl Bar {
        pub fn foo() {}
    }
}


