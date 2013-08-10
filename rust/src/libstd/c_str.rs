// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use cast;
use iterator::Iterator;
use libc;
use ops::Drop;
use option::{Option, Some, None};
use ptr::RawPtr;
use ptr;
use str::StrSlice;
use vec::ImmutableVector;

/// The representation of a C String.
///
/// This structure wraps a `*libc::c_char`, and will automatically free the
/// memory it is pointing to when it goes out of scope.
pub struct CString {
    priv buf: *libc::c_char,
    priv owns_buffer_: bool,
}

impl CString {
    /// Create a C String from a pointer.
    pub unsafe fn new(buf: *libc::c_char, owns_buffer: bool) -> CString {
        CString { buf: buf, owns_buffer_: owns_buffer }
    }

    /// Unwraps the wrapped `*libc::c_char` from the `CString` wrapper.
    pub unsafe fn unwrap(self) -> *libc::c_char {
        let mut c_str = self;
        c_str.owns_buffer_ = false;
        c_str.buf
    }

    /// Calls a closure with a reference to the underlying `*libc::c_char`.
    ///
    /// # Failure
    ///
    /// Fails if the CString is null.
    pub fn with_ref<T>(&self, f: &fn(*libc::c_char) -> T) -> T {
        if self.buf.is_null() { fail!("CString is null!"); }
        f(self.buf)
    }

    /// Calls a closure with a mutable reference to the underlying `*libc::c_char`.
    ///
    /// # Failure
    ///
    /// Fails if the CString is null.
    pub fn with_mut_ref<T>(&mut self, f: &fn(*mut libc::c_char) -> T) -> T {
        if self.buf.is_null() { fail!("CString is null!"); }
        f(unsafe { cast::transmute_mut_unsafe(self.buf) })
    }

    /// Returns true if the CString is a null.
    pub fn is_null(&self) -> bool {
        self.buf.is_null()
    }

    /// Returns true if the CString is not null.
    pub fn is_not_null(&self) -> bool {
        self.buf.is_not_null()
    }

    /// Returns whether or not the `CString` owns the buffer.
    pub fn owns_buffer(&self) -> bool {
        self.owns_buffer_
    }

    /// Converts the CString into a `&[u8]` without copying.
    ///
    /// # Failure
    ///
    /// Fails if the CString is null.
    pub fn as_bytes<'a>(&'a self) -> &'a [u8] {
        if self.buf.is_null() { fail!("CString is null!"); }
        unsafe {
            let len = libc::strlen(self.buf) as uint;
            cast::transmute((self.buf, len + 1))
        }
    }

    /// Return a CString iterator.
    fn iter<'a>(&'a self) -> CStringIterator<'a> {
        CStringIterator {
            ptr: self.buf,
            lifetime: unsafe { cast::transmute(self.buf) },
        }
    }
}

impl Drop for CString {
    fn drop(&self) {
        if self.owns_buffer_ {
            unsafe {
                libc::free(self.buf as *libc::c_void)
            }
        }
    }
}

/// A generic trait for converting a value to a CString.
pub trait ToCStr {
    /// Create a C String.
    fn to_c_str(&self) -> CString;
}

impl<'self> ToCStr for &'self str {
    #[inline]
    fn to_c_str(&self) -> CString {
        self.as_bytes().to_c_str()
    }
}

impl<'self> ToCStr for &'self [u8] {
    fn to_c_str(&self) -> CString {
        do self.as_imm_buf |self_buf, self_len| {
            unsafe {
                let buf = libc::malloc(self_len as libc::size_t + 1) as *mut u8;
                if buf.is_null() {
                    fail!("failed to allocate memory!");
                }

                ptr::copy_memory(buf, self_buf, self_len);
                *ptr::mut_offset(buf, self_len as int) = 0;

                CString::new(buf as *libc::c_char, true)
            }
        }
    }
}

/// External iterator for a CString's bytes.
///
/// Use with the `std::iterator` module.
pub struct CStringIterator<'self> {
    priv ptr: *libc::c_char,
    priv lifetime: &'self libc::c_char, // FIXME: #5922
}

impl<'self> Iterator<libc::c_char> for CStringIterator<'self> {
    fn next(&mut self) -> Option<libc::c_char> {
        let ch = unsafe { *self.ptr };
        if ch == 0 {
            None
        } else {
            self.ptr = ptr::offset(self.ptr, 1);
            Some(ch)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libc;
    use ptr;
    use option::{Some, None};

    #[test]
    fn test_to_c_str() {
        do "".to_c_str().with_ref |buf| {
            unsafe {
                assert_eq!(*ptr::offset(buf, 0), 0);
            }
        }

        do "hello".to_c_str().with_ref |buf| {
            unsafe {
                assert_eq!(*ptr::offset(buf, 0), 'h' as libc::c_char);
                assert_eq!(*ptr::offset(buf, 1), 'e' as libc::c_char);
                assert_eq!(*ptr::offset(buf, 2), 'l' as libc::c_char);
                assert_eq!(*ptr::offset(buf, 3), 'l' as libc::c_char);
                assert_eq!(*ptr::offset(buf, 4), 'o' as libc::c_char);
                assert_eq!(*ptr::offset(buf, 5), 0);
            }
        }
    }

    #[test]
    fn test_is_null() {
        let c_str = unsafe { CString::new(ptr::null(), false) };
        assert!(c_str.is_null());
        assert!(!c_str.is_not_null());
    }

    #[test]
    fn test_unwrap() {
        let c_str = "hello".to_c_str();
        unsafe { libc::free(c_str.unwrap() as *libc::c_void) }
    }

    #[test]
    fn test_with_ref() {
        let c_str = "hello".to_c_str();
        let len = unsafe { c_str.with_ref(|buf| libc::strlen(buf)) };
        assert!(!c_str.is_null());
        assert!(c_str.is_not_null());
        assert_eq!(len, 5);
    }

    #[test]
    #[should_fail]
    #[ignore(cfg(windows))]
    fn test_with_ref_empty_fail() {
        let c_str = unsafe { CString::new(ptr::null(), false) };
        c_str.with_ref(|_| ());
    }

    #[test]
    fn test_iterator() {
        let c_str = "".to_c_str();
        let mut iter = c_str.iter();
        assert_eq!(iter.next(), None);

        let c_str = "hello".to_c_str();
        let mut iter = c_str.iter();
        assert_eq!(iter.next(), Some('h' as libc::c_char));
        assert_eq!(iter.next(), Some('e' as libc::c_char));
        assert_eq!(iter.next(), Some('l' as libc::c_char));
        assert_eq!(iter.next(), Some('l' as libc::c_char));
        assert_eq!(iter.next(), Some('o' as libc::c_char));
        assert_eq!(iter.next(), None);
    }
}
