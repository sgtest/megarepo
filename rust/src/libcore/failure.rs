// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Failure support for libcore
//!
//! The core library cannot define failure, but it does *declare* failure. This
//! means that the functions inside of libcore are allowed to fail, but to be
//! useful an upstream crate must define failure for libcore to use. The current
//! interface for failure is:
//!
//! ```ignore
//! fn begin_unwind(fmt: &fmt::Arguments, file: &str, line: uint) -> !;
//! ```
//!
//! This definition allows for failing with any general message, but it does not
//! allow for failing with a `~Any` value. The reason for this is that libcore
//! is not allowed to allocate.
//!
//! This module contains a few other failure functions, but these are just the
//! necessary lang items for the compiler. All failure is funneled through this
//! one function. Currently, the actual symbol is declared in the standard
//! library, but the location of this may change over time.

#![allow(dead_code, missing_doc)]

use fmt;
use intrinsics;

#[cold] #[inline(never)] // this is the slow path, always
#[lang="fail_"]
fn fail_(expr: &'static str, file: &'static str, line: uint) -> ! {
    format_args!(|args| -> () {
        begin_unwind(args, file, line);
    }, "{}", expr);

    unsafe { intrinsics::abort() }
}

#[cold]
#[lang="fail_bounds_check"]
fn fail_bounds_check(file: &'static str, line: uint,
                     index: uint, len: uint) -> ! {
    format_args!(|args| -> () {
        begin_unwind(args, file, line);
    }, "index out of bounds: the len is {} but the index is {}", len, index);
    unsafe { intrinsics::abort() }
}

#[cold]
pub fn begin_unwind(fmt: &fmt::Arguments, file: &'static str, line: uint) -> ! {
    #[allow(ctypes)]
    extern {
        #[lang = "begin_unwind"]
        fn begin_unwind(fmt: &fmt::Arguments, file: &'static str,
                        line: uint) -> !;
    }
    unsafe { begin_unwind(fmt, file, line) }
}
