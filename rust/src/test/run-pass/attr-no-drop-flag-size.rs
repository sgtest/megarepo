// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(unsafe_no_drop_flag)]

use std::mem::size_of;

#[unsafe_no_drop_flag]
struct Test<T> {
    a: T
}

impl<T> Drop for Test<T> {
    fn drop(&mut self) { }
}

pub fn main() {
    assert_eq!(size_of::<isize>(), size_of::<Test<isize>>());
}
