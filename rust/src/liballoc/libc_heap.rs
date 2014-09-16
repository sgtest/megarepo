// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


//! The global (exchange) heap.

use libc::{c_void, size_t, free, malloc, realloc};
use core::ptr::{RawPtr, null_mut};

/// A wrapper around libc::malloc, aborting on out-of-memory.
#[inline]
pub unsafe fn malloc_raw(size: uint) -> *mut u8 {
    // `malloc(0)` may allocate, but it may also return a null pointer
    // http://pubs.opengroup.org/onlinepubs/9699919799/functions/malloc.html
    if size == 0 {
        null_mut()
    } else {
        let p = malloc(size as size_t);
        if p.is_null() {
            ::oom();
        }
        p as *mut u8
    }
}

/// A wrapper around libc::realloc, aborting on out-of-memory.
#[inline]
pub unsafe fn realloc_raw(ptr: *mut u8, size: uint) -> *mut u8 {
    // `realloc(ptr, 0)` may allocate, but it may also return a null pointer
    // http://pubs.opengroup.org/onlinepubs/9699919799/functions/realloc.html
    if size == 0 {
        free(ptr as *mut c_void);
        null_mut()
    } else {
        let p = realloc(ptr as *mut c_void, size as size_t);
        if p.is_null() {
            ::oom();
        }
        p as *mut u8
    }
}
