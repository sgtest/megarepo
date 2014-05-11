// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Global storage for command line arguments
//!
//! The current incarnation of the Rust runtime expects for
//! the processes `argc` and `argv` arguments to be stored
//! in a globally-accessible location for use by the `os` module.
//!
//! Only valid to call on linux. Mac and Windows use syscalls to
//! discover the command line arguments.
//!
//! FIXME #7756: Would be nice for this to not exist.
//! FIXME #7756: This has a lot of C glue for lack of globals.

use option::Option;
use vec::Vec;
#[cfg(test)] use option::{Some, None};
#[cfg(test)] use realstd;
#[cfg(test)] use realargs = realstd::rt::args;

/// One-time global initialization.
#[cfg(not(test))]
pub unsafe fn init(argc: int, argv: **u8) { imp::init(argc, argv) }
#[cfg(test)]
pub unsafe fn init(argc: int, argv: **u8) { realargs::init(argc, argv) }

/// One-time global cleanup.
#[cfg(not(test))] pub unsafe fn cleanup() { imp::cleanup() }
#[cfg(test)]      pub unsafe fn cleanup() { realargs::cleanup() }

/// Take the global arguments from global storage.
#[cfg(not(test))] pub fn take() -> Option<Vec<~[u8]>> { imp::take() }
#[cfg(test)]      pub fn take() -> Option<Vec<~[u8]>> {
    match realargs::take() {
        realstd::option::Some(v) => Some(unsafe{ ::mem::transmute(v) }),
        realstd::option::None => None,
    }
}

/// Give the global arguments to global storage.
///
/// It is an error if the arguments already exist.
#[cfg(not(test))] pub fn put(args: Vec<~[u8]>) { imp::put(args) }
#[cfg(test)]      pub fn put(args: Vec<~[u8]>) { realargs::put(unsafe { ::mem::transmute(args) }) }

/// Make a clone of the global arguments.
#[cfg(not(test))] pub fn clone() -> Option<Vec<~[u8]>> { imp::clone() }
#[cfg(test)]      pub fn clone() -> Option<Vec<~[u8]>> {
    match realargs::clone() {
        realstd::option::Some(v) => Some(unsafe { ::mem::transmute(v) }),
        realstd::option::None => None,
    }
}

#[cfg(target_os = "linux")]
#[cfg(target_os = "android")]
#[cfg(target_os = "freebsd")]
mod imp {
    use clone::Clone;
    use iter::Iterator;
    use option::{Option, Some, None};
    use owned::Box;
    use unstable::mutex::{StaticNativeMutex, NATIVE_MUTEX_INIT};
    use mem;
    use vec::Vec;
    #[cfg(not(test))] use ptr::RawPtr;

    static mut global_args_ptr: uint = 0;
    static mut lock: StaticNativeMutex = NATIVE_MUTEX_INIT;

    #[cfg(not(test))]
    pub unsafe fn init(argc: int, argv: **u8) {
        let args = load_argc_and_argv(argc, argv);
        put(args);
    }

    #[cfg(not(test))]
    pub unsafe fn cleanup() {
        rtassert!(take().is_some());
        lock.destroy();
    }

    pub fn take() -> Option<Vec<~[u8]>> {
        with_lock(|| unsafe {
            let ptr = get_global_ptr();
            let val = mem::replace(&mut *ptr, None);
            val.as_ref().map(|s: &Box<Vec<~[u8]>>| (**s).clone())
        })
    }

    pub fn put(args: Vec<~[u8]>) {
        with_lock(|| unsafe {
            let ptr = get_global_ptr();
            rtassert!((*ptr).is_none());
            (*ptr) = Some(box args.clone());
        })
    }

    pub fn clone() -> Option<Vec<~[u8]>> {
        with_lock(|| unsafe {
            let ptr = get_global_ptr();
            (*ptr).as_ref().map(|s: &Box<Vec<~[u8]>>| (**s).clone())
        })
    }

    fn with_lock<T>(f: || -> T) -> T {
        unsafe {
            let _guard = lock.lock();
            f()
        }
    }

    fn get_global_ptr() -> *mut Option<Box<Vec<~[u8]>>> {
        unsafe { mem::transmute(&global_args_ptr) }
    }

    // Copied from `os`.
    #[cfg(not(test))]
    unsafe fn load_argc_and_argv(argc: int, argv: **u8) -> Vec<~[u8]> {
        use c_str::CString;
        use ptr::RawPtr;
        use libc;
        use slice::CloneableVector;
        use vec::Vec;

        Vec::from_fn(argc as uint, |i| {
            let cs = CString::new(*(argv as **libc::c_char).offset(i as int), false);
            cs.as_bytes_no_nul().to_owned()
        })
    }

    #[cfg(test)]
    mod tests {
        use prelude::*;
        use super::*;
        use unstable::finally::Finally;

        #[test]
        fn smoke_test() {
            // Preserve the actual global state.
            let saved_value = take();

            let expected = vec![bytes!("happy").to_owned(), bytes!("today?").to_owned()];

            put(expected.clone());
            assert!(clone() == Some(expected.clone()));
            assert!(take() == Some(expected.clone()));
            assert!(take() == None);

            (|| {
            }).finally(|| {
                // Restore the actual global state.
                match saved_value {
                    Some(ref args) => put(args.clone()),
                    None => ()
                }
            })
        }
    }
}

#[cfg(target_os = "macos", not(test))]
#[cfg(target_os = "win32", not(test))]
mod imp {
    use option::Option;
    use vec::Vec;

    pub unsafe fn init(_argc: int, _argv: **u8) {
    }

    pub fn cleanup() {
    }

    pub fn take() -> Option<Vec<~[u8]>> {
        fail!()
    }

    pub fn put(_args: Vec<~[u8]>) {
        fail!()
    }

    pub fn clone() -> Option<Vec<~[u8]>> {
        fail!()
    }
}
