// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use libc::c_void;
#[cfg(unix)]
use libc::c_int;
#[cfg(unix)]
use ptr::null;
#[cfg(windows)]
use libc::types::os::arch::extra::{DWORD, LPVOID, BOOL};

#[cfg(unix)]
pub type Key = pthread_key_t;

#[cfg(unix)]
pub unsafe fn create(key: &mut Key) {
    assert_eq!(0, pthread_key_create(key, null()));
}

#[cfg(unix)]
pub unsafe fn set(key: Key, value: *mut c_void) {
    assert_eq!(0, pthread_setspecific(key, value));
}

#[cfg(unix)]
pub unsafe fn get(key: Key) -> *mut c_void {
    pthread_getspecific(key)
}

#[cfg(target_os="macos")]
#[allow(non_camel_case_types)] // foreign type
type pthread_key_t = ::libc::c_ulong;

#[cfg(target_os="linux")]
#[cfg(target_os="freebsd")]
#[cfg(target_os="android")]
#[allow(non_camel_case_types)] // foreign type
type pthread_key_t = ::libc::c_uint;

#[cfg(unix)]
extern {
    #[fast_ffi]
    fn pthread_key_create(key: *mut pthread_key_t, dtor: *u8) -> c_int;
    #[fast_ffi]
    fn pthread_setspecific(key: pthread_key_t, value: *mut c_void) -> c_int;
    #[fast_ffi]
    fn pthread_getspecific(key: pthread_key_t) -> *mut c_void;
}

#[cfg(windows)]
pub type Key = DWORD;

#[cfg(windows)]
pub unsafe fn create(key: &mut Key) {
    static TLS_OUT_OF_INDEXES: DWORD = 0xFFFFFFFF;
    *key = unsafe { TlsAlloc() };
    assert!(*key != TLS_OUT_OF_INDEXES);
}

#[cfg(windows)]
pub unsafe fn set(key: Key, value: *mut c_void) {
    unsafe { assert!(0 != TlsSetValue(key, value)) }
}

#[cfg(windows)]
pub unsafe fn get(key: Key) -> *mut c_void {
    TlsGetValue(key)
}

#[cfg(windows)]
#[abi = "stdcall"]
extern "stdcall" {
       fn TlsAlloc() -> DWORD;
       fn TlsSetValue(dwTlsIndex: DWORD, lpTlsvalue: LPVOID) -> BOOL;
       fn TlsGetValue(dwTlsIndex: DWORD) -> LPVOID;
}

#[test]
fn tls_smoke_test() {
    use cast::transmute;
    unsafe {
        let mut key = 0;
        let value = ~20;
        create(&mut key);
        set(key, transmute(value));
        let value: ~int = transmute(get(key));
        assert_eq!(value, ~20);
        let value = ~30;
        set(key, transmute(value));
        let value: ~int = transmute(get(key));
        assert_eq!(value, ~30);
    }
}
