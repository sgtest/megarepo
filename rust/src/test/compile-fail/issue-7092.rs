// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

enum Whatever {
}

fn foo(x: Whatever) {
    match x {
        Some(field) =>
//~^ ERROR: mismatched types: expected `Whatever`, found `core::option::Option<<generic #3>>`
            field.access(), //~ ERROR the type of this value must be known in this context
    }
}

fn main(){}
