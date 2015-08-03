// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(no_std, core, libc)]
#![no_std]
#![feature(lang_items)]

extern crate core;
extern crate libc;

#[lang = "stack_exhausted"]
extern fn stack_exhausted() {}

#[lang = "eh_personality"]
extern fn eh_personality() {}

#[lang = "eh_unwind_resume"]
extern fn eh_unwind_resume() {}

#[lang = "panic_fmt"]
extern fn rust_begin_unwind(msg: core::fmt::Arguments, file: &'static str,
                            line: u32) -> ! {
    loop {}
}
