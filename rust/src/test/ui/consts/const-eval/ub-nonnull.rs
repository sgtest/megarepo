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
use std::ptr::NonNull;
use std::num::{NonZeroU8, NonZeroUsize};

const NULL_PTR: NonNull<u8> = unsafe { mem::transmute(0usize) };
//~^ ERROR this constant likely exhibits undefined behavior

const NULL_U8: NonZeroU8 = unsafe { mem::transmute(0u8) };
//~^ ERROR this constant likely exhibits undefined behavior
const NULL_USIZE: NonZeroUsize = unsafe { mem::transmute(0usize) };
//~^ ERROR this constant likely exhibits undefined behavior

fn main() {}
