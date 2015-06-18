// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// pretty-expanded FIXME #23616

#![feature(lang_items, start, no_std, core_slice_ext, core, collections)]
#![no_std]

extern crate std as other;

#[macro_use] extern crate core;
#[macro_use] extern crate collections;

use core::slice::SliceExt;

#[start]
fn start(_argc: isize, _argv: *const *const u8) -> isize {
    for _ in [1,2,3].iter() { }
    0
}
