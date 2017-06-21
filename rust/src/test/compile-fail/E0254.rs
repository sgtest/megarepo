// Copyright 2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(alloc)]

extern crate alloc;
//~^ NOTE previous import of the extern crate `alloc` here

mod foo {
    pub trait alloc {
        fn do_something();
    }
}

use foo::alloc;
//~^ ERROR E0254
//~| NOTE `alloc` reimported here
//~| NOTE `alloc` must be defined only once in the type namespace of this module

fn main() {}
