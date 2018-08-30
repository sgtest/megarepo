// Copyright 2018 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(const_transmute)]

use std::mem;

static FOO: bool = unsafe { mem::transmute(3u8) };
//~^ ERROR this static likely exhibits undefined behavior
//~^^ type validation failed: encountered 3, but expected something in the range 0..=1

fn main() {}
