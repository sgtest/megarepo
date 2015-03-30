// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// pretty-expanded FIXME #23616

#![feature(std_misc, libc)]

extern crate libc;

use std::thunk::Thunk;

fn foo(_: Thunk) {}

fn main() {
    foo(loop {
        unsafe { libc::exit(0 as libc::c_int); }
    });
    2_usize + (loop {});
    //~^ ERROR E0277
    //~| ERROR E0277
}
