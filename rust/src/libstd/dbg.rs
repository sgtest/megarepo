// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[forbid(deprecated_mode)];
//! Unsafe debugging functions for inspecting values.

use core::cast::reinterpret_cast;
use core::ptr;
use core::sys;

#[abi = "cdecl"]
extern mod rustrt {
    #[legacy_exports];
    unsafe fn debug_tydesc(td: *sys::TypeDesc);
    unsafe fn debug_opaque(td: *sys::TypeDesc, x: *());
    unsafe fn debug_box(td: *sys::TypeDesc, x: *());
    unsafe fn debug_tag(td: *sys::TypeDesc, x: *());
    unsafe fn debug_fn(td: *sys::TypeDesc, x: *());
    unsafe fn debug_ptrcast(td: *sys::TypeDesc, x: *()) -> *();
    unsafe fn rust_dbg_breakpoint();
}

pub fn debug_tydesc<T>() {
    unsafe {
        rustrt::debug_tydesc(sys::get_type_desc::<T>());
    }
}

pub fn debug_opaque<T>(x: T) {
    unsafe {
        rustrt::debug_opaque(sys::get_type_desc::<T>(),
                             ptr::addr_of(&x) as *());
    }
}

pub fn debug_box<T>(x: @T) {
    unsafe {
        rustrt::debug_box(sys::get_type_desc::<T>(),
                          ptr::addr_of(&x) as *());
    }
}

pub fn debug_tag<T>(x: T) {
    unsafe {
        rustrt::debug_tag(sys::get_type_desc::<T>(),
                          ptr::addr_of(&x) as *());
    }
}

pub fn debug_fn<T>(x: T) {
    unsafe {
        rustrt::debug_fn(sys::get_type_desc::<T>(),
                         ptr::addr_of(&x) as *());
    }
}

pub unsafe fn ptr_cast<T, U>(x: @T) -> @U {
    reinterpret_cast(
        &rustrt::debug_ptrcast(sys::get_type_desc::<T>(),
                              reinterpret_cast(&x)))
}

/// Triggers a debugger breakpoint
pub fn breakpoint() {
    unsafe {
        rustrt::rust_dbg_breakpoint();
    }
}

#[test]
fn test_breakpoint_should_not_abort_process_when_not_under_gdb() {
    // Triggering a breakpoint involves raising SIGTRAP, which terminates
    // the process under normal circumstances
    breakpoint();
}

// Local Variables:
// mode: rust;
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
