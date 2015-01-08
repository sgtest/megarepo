// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![deny(improper_ctypes)]

extern crate libc;

extern {
    pub fn bare_type1(size: isize); //~ ERROR: found rust type
    pub fn bare_type2(size: usize); //~ ERROR: found rust type
    pub fn ptr_type1(size: *const isize); //~ ERROR: found rust type
    pub fn ptr_type2(size: *const usize); //~ ERROR: found rust type

    pub fn good1(size: *const libc::c_int);
    pub fn good2(size: *const libc::c_uint);
}

fn main() {
}
