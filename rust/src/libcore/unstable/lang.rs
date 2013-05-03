// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Runtime calls emitted by the compiler.

use cast::transmute;
use libc::{c_char, c_uchar, c_void, size_t, uintptr_t, c_int};
use managed::raw::BoxRepr;
use str;
use sys;
use unstable::exchange_alloc;
use cast::transmute;
use rt::{context, OldTaskContext};
use rt::local_services::borrow_local_services;

#[allow(non_camel_case_types)]
pub type rust_task = c_void;

#[cfg(target_word_size = "32")]
pub static FROZEN_BIT: uint = 0x80000000;
#[cfg(target_word_size = "64")]
pub static FROZEN_BIT: uint = 0x8000000000000000;

pub mod rustrt {
    use libc::{c_char, uintptr_t};

    pub extern {
        #[rust_stack]
        unsafe fn rust_upcall_malloc(td: *c_char, size: uintptr_t) -> *c_char;

        #[rust_stack]
        unsafe fn rust_upcall_free(ptr: *c_char);

        #[fast_ffi]
        unsafe fn rust_upcall_malloc_noswitch(td: *c_char,
                                              size: uintptr_t)
                                           -> *c_char;

        #[fast_ffi]
        unsafe fn rust_upcall_free_noswitch(ptr: *c_char);
    }
}

#[lang="fail_"]
pub fn fail_(expr: *c_char, file: *c_char, line: size_t) -> ! {
    sys::begin_unwind_(expr, file, line);
}

#[lang="fail_bounds_check"]
pub fn fail_bounds_check(file: *c_char, line: size_t,
                                index: size_t, len: size_t) {
    let msg = fmt!("index out of bounds: the len is %d but the index is %d",
                    len as int, index as int);
    do str::as_buf(msg) |p, _len| {
        fail_(p as *c_char, file, line);
    }
}

pub fn fail_borrowed() {
    let msg = "borrowed";
    do str::as_buf(msg) |msg_p, _| {
        do str::as_buf("???") |file_p, _| {
            fail_(msg_p as *c_char, file_p as *c_char, 0);
        }
    }
}

// FIXME #4942: Make these signatures agree with exchange_alloc's signatures
#[lang="exchange_malloc"]
#[inline(always)]
pub unsafe fn exchange_malloc(td: *c_char, size: uintptr_t) -> *c_char {
    transmute(exchange_alloc::malloc(transmute(td), transmute(size)))
}

// NB: Calls to free CANNOT be allowed to fail, as throwing an exception from
// inside a landing pad may corrupt the state of the exception handler. If a
// problem occurs, call exit instead.
#[lang="exchange_free"]
#[inline(always)]
pub unsafe fn exchange_free(ptr: *c_char) {
    exchange_alloc::free(transmute(ptr))
}

#[lang="malloc"]
#[inline(always)]
#[cfg(stage0)] // For some reason this isn't working on windows in stage0
pub unsafe fn local_malloc(td: *c_char, size: uintptr_t) -> *c_char {
    return rustrt::rust_upcall_malloc_noswitch(td, size);
}

#[lang="malloc"]
#[inline(always)]
#[cfg(not(stage0))]
pub unsafe fn local_malloc(td: *c_char, size: uintptr_t) -> *c_char {
    match context() {
        OldTaskContext => {
            return rustrt::rust_upcall_malloc_noswitch(td, size);
        }
        _ => {
            let mut alloc = ::ptr::null();
            do borrow_local_services |srv| {
                alloc = srv.heap.alloc(td as *c_void, size as uint) as *c_char;
            }
            return alloc;
        }
    }
}

// NB: Calls to free CANNOT be allowed to fail, as throwing an exception from
// inside a landing pad may corrupt the state of the exception handler. If a
// problem occurs, call exit instead.
#[lang="free"]
#[inline(always)]
#[cfg(stage0)]
pub unsafe fn local_free(ptr: *c_char) {
    rustrt::rust_upcall_free_noswitch(ptr);
}

// NB: Calls to free CANNOT be allowed to fail, as throwing an exception from
// inside a landing pad may corrupt the state of the exception handler. If a
// problem occurs, call exit instead.
#[lang="free"]
#[inline(always)]
#[cfg(not(stage0))]
pub unsafe fn local_free(ptr: *c_char) {
    match context() {
        OldTaskContext => {
            rustrt::rust_upcall_free_noswitch(ptr);
        }
        _ => {
            do borrow_local_services |srv| {
                srv.heap.free(ptr as *c_void);
            }
        }
    }
}

#[lang="borrow_as_imm"]
#[inline(always)]
pub unsafe fn borrow_as_imm(a: *u8) {
    let a: *mut BoxRepr = transmute(a);
    (*a).header.ref_count |= FROZEN_BIT;
}

#[lang="return_to_mut"]
#[inline(always)]
pub unsafe fn return_to_mut(a: *u8) {
    // Sometimes the box is null, if it is conditionally frozen.
    // See e.g. #4904.
    if !a.is_null() {
        let a: *mut BoxRepr = transmute(a);
        (*a).header.ref_count &= !FROZEN_BIT;
    }
}

#[lang="check_not_borrowed"]
#[inline(always)]
pub unsafe fn check_not_borrowed(a: *u8) {
    let a: *mut BoxRepr = transmute(a);
    if ((*a).header.ref_count & FROZEN_BIT) != 0 {
        fail_borrowed();
    }
}

#[lang="strdup_uniq"]
#[inline(always)]
pub unsafe fn strdup_uniq(ptr: *c_uchar, len: uint) -> ~str {
    str::raw::from_buf_len(ptr, len)
}

#[lang="start"]
#[cfg(stage0)]
pub fn start(main: *u8, argc: int, argv: *c_char,
             crate_map: *u8) -> int {
    use libc::getenv;
    use rt::start;

    unsafe {
        let use_old_rt = do str::as_c_str("RUST_NEWRT") |s| {
            getenv(s).is_null()
        };
        if use_old_rt {
            return rust_start(main as *c_void, argc as c_int, argv,
                              crate_map as *c_void) as int;
        } else {
            return start(main, argc, argv, crate_map);
        }
    }

    extern {
        fn rust_start(main: *c_void, argc: c_int, argv: *c_char,
                      crate_map: *c_void) -> c_int;
    }
}

#[lang="start"]
#[cfg(not(stage0))]
pub fn start(main: *u8, argc: int, argv: **c_char,
             crate_map: *u8) -> int {
    use libc::getenv;
    use rt::start;

    unsafe {
        let use_old_rt = do str::as_c_str("RUST_NEWRT") |s| {
            getenv(s).is_null()
        };
        if use_old_rt {
            return rust_start(main as *c_void, argc as c_int, argv,
                              crate_map as *c_void) as int;
        } else {
            return start(main, argc, argv, crate_map);
        }
    }

    extern {
        fn rust_start(main: *c_void, argc: c_int, argv: **c_char,
                      crate_map: *c_void) -> c_int;
    }
}
