// Copyright 2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(rustc_attrs)]
#![allow(dead_code)]
#![allow(unused_imports)]

mod foo {
    extern crate core;
}

// Check that private crates can be used from outside their modules, albeit with warnings
use foo::core; //~ WARN extern crate `core` is private
//~^ WARN this was previously accepted by the compiler but is being phased out
use foo::core::cell; //~ WARN extern crate `core` is private
//~^ WARN this was previously accepted by the compiler but is being phased out

fn f() {
    foo::core::cell::Cell::new(0); //~ WARN extern crate `core` is private
    //~^ WARN this was previously accepted by the compiler but is being phased out

    use foo::*;
    mod core {} // Check that private crates are not glob imported
}

#[rustc_error]
fn main() {} //~ ERROR compilation successful
