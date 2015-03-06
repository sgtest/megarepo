// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// this has to be separate to internal-unstable.rs because these tests
// have error messages pointing deep into the internals of the
// cross-crate macros, and hence need to use error-pattern instead of
// the // ~ form.

// aux-build:internal_unstable.rs
// error-pattern:use of unstable library feature 'function'
// error-pattern:use of unstable library feature 'struct_field'
// error-pattern:compilation successful
#![feature(rustc_attrs)]

#[macro_use]
extern crate internal_unstable;

#[rustc_error]
fn main() {
    call_unstable_noallow!();

    construct_unstable_noallow!(0);
}
