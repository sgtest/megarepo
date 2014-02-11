// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// ignore-fast compile-flags directive doesn't work for check-fast
// compile-flags:

// check that cfg correctly chooses between the macro impls (see also
// cfg-macros-foo.rs)

#[feature(macro_rules)];

#[cfg(foo)]
#[macro_escape]
mod foo {
    macro_rules! bar {
        () => { true }
    }
}

#[cfg(not(foo))]
#[macro_escape]
mod foo {
    macro_rules! bar {
        () => { false }
    }
}

pub fn main() {
    assert!(!bar!())
}
