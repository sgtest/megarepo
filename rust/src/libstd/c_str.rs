// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! C-string manipulation and management
//!
//! This modules provides the basic methods for creating and manipulating
//! null-terminated strings for use with FFI calls (back to C). Most C APIs require
//! that the string being passed to them is null-terminated, and by default rust's
//! string types are *not* null terminated.
//!
//! The other problem with translating Rust strings to C strings is that Rust
//! strings can validly contain a null-byte in the middle of the string (0 is a
//! valid Unicode codepoint). This means that not all Rust strings can actually be
//! translated to C strings.
//!
//! # Creation of a C string
//!
//! A C string is managed through the `CString` type defined in this module. It
//! "owns" the internal buffer of characters and will automatically deallocate the
//! buffer when the string is dropped. The `ToCStr` trait is implemented for `&str`
//! and `&[u8]`, but the conversions can fail due to some of the limitations
//! explained above.
//!
//! This also means that currently whenever a C string is created, an allocation
//! must be performed to place the data elsewhere (the lifetime of the C string is
//! not tied to the lifetime of the original string/data buffer). If C strings are
//! heavily used in applications, then caching may be advisable to prevent
//! unnecessary amounts of allocations.
//!
//! Be carefull to remember that the memory is managed by C allocator API and not
//! by Rust allocator API.
//! That means that the CString pointers should be freed with C allocator API
//! if you intend to do that on your own, as the behaviour if you free them with
//! Rust's allocator API is not well defined
//!
//! An example of creating and using a C string would be:
//!
//! ```rust
//! extern crate libc;
//!
//! use std::c_str::ToCStr;
//!
//! extern {
//!     fn puts(s: *const libc::c_char);
//! }
//!
//! fn main() {
//!     let my_string = "Hello, world!";
//!
//!     // Allocate the C string with an explicit local that owns the string. The
//!     // `c_buffer` pointer will be deallocated when `my_c_string` goes out of scope.
//!     let my_c_string = my_string.to_c_str();
//!     unsafe {
//!         puts(my_c_string.as_ptr());
//!     }
//!
//!     // Don't save/return the pointer to the C string, the `c_buffer` will be
//!     // deallocated when this block returns!
//!     my_string.with_c_str(|c_buffer| {
//!         unsafe { puts(c_buffer); }
//!     });
//! }
//! ```

use core::prelude::*;
use libc;

use cmp::Ordering;
use fmt;
use hash;
use mem;
use ptr;
use slice::{mod, IntSliceExt};
use str;
use string::String;
use core::kinds::marker;

/// The representation of a C String.
///
/// This structure wraps a `*libc::c_char`, and will automatically free the
/// memory it is pointing to when it goes out of scope.
#[allow(missing_copy_implementations)]
pub struct CString {
    buf: *const libc::c_char,
    owns_buffer_: bool,
}

unsafe impl Send for CString { }
unsafe impl Sync for CString { }

impl Clone for CString {
    /// Clone this CString into a new, uniquely owned CString. For safety
    /// reasons, this is always a deep clone with the memory allocated
    /// with C's allocator API, rather than the usual shallow clone.
    fn clone(&self) -> CString {
        let len = self.len() + 1;
        let buf = unsafe { libc::malloc(len as libc::size_t) } as *mut libc::c_char;
        if buf.is_null() { ::alloc::oom() }
        unsafe { ptr::copy_nonoverlapping_memory(buf, self.buf, len); }
        CString { buf: buf as *const libc::c_char, owns_buffer_: true }
    }
}

impl PartialEq for CString {
    fn eq(&self, other: &CString) -> bool {
        // Check if the two strings share the same buffer
        if self.buf as uint == other.buf as uint {
            true
        } else {
            unsafe {
                libc::strcmp(self.buf, other.buf) == 0
            }
        }
    }
}

impl PartialOrd for CString {
    #[inline]
    fn partial_cmp(&self, other: &CString) -> Option<Ordering> {
        self.as_bytes().partial_cmp(other.as_bytes())
    }
}

impl Eq for CString {}

impl<S: hash::Writer> hash::Hash<S> for CString {
    #[inline]
    fn hash(&self, state: &mut S) {
        self.as_bytes().hash(state)
    }
}

impl CString {
    /// Create a C String from a pointer, with memory managed by C's allocator
    /// API, so avoid calling it with a pointer to memory managed by Rust's
    /// allocator API, as the behaviour would not be well defined.
    ///
    ///# Panics
    ///
    /// Panics if `buf` is null
    pub unsafe fn new(buf: *const libc::c_char, owns_buffer: bool) -> CString {
        assert!(!buf.is_null());
        CString { buf: buf, owns_buffer_: owns_buffer }
    }

    /// Return a pointer to the NUL-terminated string data.
    ///
    /// `.as_ptr` returns an internal pointer into the `CString`, and
    /// may be invalidated when the `CString` falls out of scope (the
    /// destructor will run, freeing the allocation if there is
    /// one).
    ///
    /// ```rust
    /// use std::c_str::ToCStr;
    ///
    /// let foo = "some string";
    ///
    /// // right
    /// let x = foo.to_c_str();
    /// let p = x.as_ptr();
    ///
    /// // wrong (the CString will be freed, invalidating `p`)
    /// let p = foo.to_c_str().as_ptr();
    /// ```
    ///
    /// # Example
    ///
    /// ```rust
    /// extern crate libc;
    ///
    /// use std::c_str::ToCStr;
    ///
    /// fn main() {
    ///     let c_str = "foo bar".to_c_str();
    ///     unsafe {
    ///         libc::puts(c_str.as_ptr());
    ///     }
    /// }
    /// ```
    pub fn as_ptr(&self) -> *const libc::c_char {
        self.buf
    }

    /// Return a mutable pointer to the NUL-terminated string data.
    ///
    /// `.as_mut_ptr` returns an internal pointer into the `CString`, and
    /// may be invalidated when the `CString` falls out of scope (the
    /// destructor will run, freeing the allocation if there is
    /// one).
    ///
    /// ```rust
    /// use std::c_str::ToCStr;
    ///
    /// let foo = "some string";
    ///
    /// // right
    /// let mut x = foo.to_c_str();
    /// let p = x.as_mut_ptr();
    ///
    /// // wrong (the CString will be freed, invalidating `p`)
    /// let p = foo.to_c_str().as_mut_ptr();
    /// ```
    pub fn as_mut_ptr(&mut self) -> *mut libc::c_char {
        self.buf as *mut _
    }

    /// Returns whether or not the `CString` owns the buffer.
    pub fn owns_buffer(&self) -> bool {
        self.owns_buffer_
    }

    /// Converts the CString into a `&[u8]` without copying.
    /// Includes the terminating NUL byte.
    #[inline]
    pub fn as_bytes<'a>(&'a self) -> &'a [u8] {
        unsafe {
            slice::from_raw_buf(&self.buf, self.len() + 1).as_unsigned()
        }
    }

    /// Converts the CString into a `&[u8]` without copying.
    /// Does not include the terminating NUL byte.
    #[inline]
    pub fn as_bytes_no_nul<'a>(&'a self) -> &'a [u8] {
        unsafe {
            slice::from_raw_buf(&self.buf, self.len()).as_unsigned()
        }
    }

    /// Converts the CString into a `&str` without copying.
    /// Returns None if the CString is not UTF-8.
    #[inline]
    pub fn as_str<'a>(&'a self) -> Option<&'a str> {
        let buf = self.as_bytes_no_nul();
        str::from_utf8(buf).ok()
    }

    /// Return a CString iterator.
    pub fn iter<'a>(&'a self) -> CChars<'a> {
        CChars {
            ptr: self.buf,
            marker: marker::ContravariantLifetime,
        }
    }

    /// Unwraps the wrapped `*libc::c_char` from the `CString` wrapper.
    ///
    /// Any ownership of the buffer by the `CString` wrapper is
    /// forgotten, meaning that the backing allocation of this
    /// `CString` is not automatically freed if it owns the
    /// allocation. In this case, a user of `.unwrap()` should ensure
    /// the allocation is freed, to avoid leaking memory. You should
    /// use libc's memory allocator in this case.
    ///
    /// Prefer `.as_ptr()` when just retrieving a pointer to the
    /// string data, as that does not relinquish ownership.
    pub unsafe fn into_inner(mut self) -> *const libc::c_char {
        self.owns_buffer_ = false;
        self.buf
    }

    /// Deprecated, use into_inner() instead
    #[deprecated = "renamed to into_inner()"]
    pub unsafe fn unwrap(self) -> *const libc::c_char { self.into_inner() }

    /// Return the number of bytes in the CString (not including the NUL
    /// terminator).
    #[inline]
    pub fn len(&self) -> uint {
        unsafe { libc::strlen(self.buf) as uint }
    }

    /// Returns if there are no bytes in this string
    #[inline]
    pub fn is_empty(&self) -> bool { self.len() == 0 }
}

impl Drop for CString {
    fn drop(&mut self) {
        if self.owns_buffer_ {
            unsafe {
                libc::free(self.buf as *mut libc::c_void)
            }
        }
    }
}

impl fmt::Show for CString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        String::from_utf8_lossy(self.as_bytes_no_nul()).fmt(f)
    }
}

/// A generic trait for converting a value to a CString.
pub trait ToCStr for Sized? {
    /// Copy the receiver into a CString.
    ///
    /// # Panics
    ///
    /// Panics the task if the receiver has an interior null.
    fn to_c_str(&self) -> CString;

    /// Unsafe variant of `to_c_str()` that doesn't check for nulls.
    unsafe fn to_c_str_unchecked(&self) -> CString;

    /// Work with a temporary CString constructed from the receiver.
    /// The provided `*libc::c_char` will be freed immediately upon return.
    ///
    /// # Example
    ///
    /// ```rust
    /// extern crate libc;
    ///
    /// use std::c_str::ToCStr;
    ///
    /// fn main() {
    ///     let s = "PATH".with_c_str(|path| unsafe {
    ///         libc::getenv(path)
    ///     });
    /// }
    /// ```
    ///
    /// # Panics
    ///
    /// Panics the task if the receiver has an interior null.
    #[inline]
    fn with_c_str<T, F>(&self, f: F) -> T where
        F: FnOnce(*const libc::c_char) -> T,
    {
        let c_str = self.to_c_str();
        f(c_str.as_ptr())
    }

    /// Unsafe variant of `with_c_str()` that doesn't check for nulls.
    #[inline]
    unsafe fn with_c_str_unchecked<T, F>(&self, f: F) -> T where
        F: FnOnce(*const libc::c_char) -> T,
    {
        let c_str = self.to_c_str_unchecked();
        f(c_str.as_ptr())
    }
}

impl ToCStr for str {
    #[inline]
    fn to_c_str(&self) -> CString {
        self.as_bytes().to_c_str()
    }

    #[inline]
    unsafe fn to_c_str_unchecked(&self) -> CString {
        self.as_bytes().to_c_str_unchecked()
    }

    #[inline]
    fn with_c_str<T, F>(&self, f: F) -> T where
        F: FnOnce(*const libc::c_char) -> T,
    {
        self.as_bytes().with_c_str(f)
    }

    #[inline]
    unsafe fn with_c_str_unchecked<T, F>(&self, f: F) -> T where
        F: FnOnce(*const libc::c_char) -> T,
    {
        self.as_bytes().with_c_str_unchecked(f)
    }
}

impl ToCStr for String {
    #[inline]
    fn to_c_str(&self) -> CString {
        self.as_bytes().to_c_str()
    }

    #[inline]
    unsafe fn to_c_str_unchecked(&self) -> CString {
        self.as_bytes().to_c_str_unchecked()
    }

    #[inline]
    fn with_c_str<T, F>(&self, f: F) -> T where
        F: FnOnce(*const libc::c_char) -> T,
    {
        self.as_bytes().with_c_str(f)
    }

    #[inline]
    unsafe fn with_c_str_unchecked<T, F>(&self, f: F) -> T where
        F: FnOnce(*const libc::c_char) -> T,
    {
        self.as_bytes().with_c_str_unchecked(f)
    }
}

// The length of the stack allocated buffer for `vec.with_c_str()`
const BUF_LEN: uint = 128;

impl ToCStr for [u8] {
    fn to_c_str(&self) -> CString {
        let mut cs = unsafe { self.to_c_str_unchecked() };
        check_for_null(self, cs.as_mut_ptr());
        cs
    }

    unsafe fn to_c_str_unchecked(&self) -> CString {
        let self_len = self.len();
        let buf = libc::malloc(self_len as libc::size_t + 1) as *mut u8;
        if buf.is_null() { ::alloc::oom() }

        ptr::copy_memory(buf, self.as_ptr(), self_len);
        *buf.offset(self_len as int) = 0;

        CString::new(buf as *const libc::c_char, true)
    }

    fn with_c_str<T, F>(&self, f: F) -> T where
        F: FnOnce(*const libc::c_char) -> T,
    {
        unsafe { with_c_str(self, true, f) }
    }

    unsafe fn with_c_str_unchecked<T, F>(&self, f: F) -> T where
        F: FnOnce(*const libc::c_char) -> T,
    {
        with_c_str(self, false, f)
    }
}

impl<'a, Sized? T: ToCStr> ToCStr for &'a T {
    #[inline]
    fn to_c_str(&self) -> CString {
        (**self).to_c_str()
    }

    #[inline]
    unsafe fn to_c_str_unchecked(&self) -> CString {
        (**self).to_c_str_unchecked()
    }

    #[inline]
    fn with_c_str<T, F>(&self, f: F) -> T where
        F: FnOnce(*const libc::c_char) -> T,
    {
        (**self).with_c_str(f)
    }

    #[inline]
    unsafe fn with_c_str_unchecked<T, F>(&self, f: F) -> T where
        F: FnOnce(*const libc::c_char) -> T,
    {
        (**self).with_c_str_unchecked(f)
    }
}

// Unsafe function that handles possibly copying the &[u8] into a stack array.
unsafe fn with_c_str<T, F>(v: &[u8], checked: bool, f: F) -> T where
    F: FnOnce(*const libc::c_char) -> T,
{
    let c_str = if v.len() < BUF_LEN {
        let mut buf: [u8; BUF_LEN] = mem::uninitialized();
        slice::bytes::copy_memory(&mut buf, v);
        buf[v.len()] = 0;

        let buf = buf.as_mut_ptr();
        if checked {
            check_for_null(v, buf as *mut libc::c_char);
        }

        return f(buf as *const libc::c_char)
    } else if checked {
        v.to_c_str()
    } else {
        v.to_c_str_unchecked()
    };

    f(c_str.as_ptr())
}

#[inline]
fn check_for_null(v: &[u8], buf: *mut libc::c_char) {
    for i in range(0, v.len()) {
        unsafe {
            let p = buf.offset(i as int);
            assert!(*p != 0);
        }
    }
}

/// External iterator for a CString's bytes.
///
/// Use with the `std::iter` module.
#[allow(raw_pointer_deriving)]
#[deriving(Clone)]
pub struct CChars<'a> {
    ptr: *const libc::c_char,
    marker: marker::ContravariantLifetime<'a>,
}

impl<'a> Iterator<libc::c_char> for CChars<'a> {
    fn next(&mut self) -> Option<libc::c_char> {
        let ch = unsafe { *self.ptr };
        if ch == 0 {
            None
        } else {
            self.ptr = unsafe { self.ptr.offset(1) };
            Some(ch)
        }
    }
}

/// Parses a C "multistring", eg windows env values or
/// the req->ptr result in a uv_fs_readdir() call.
///
/// Optionally, a `count` can be passed in, limiting the
/// parsing to only being done `count`-times.
///
/// The specified closure is invoked with each string that
/// is found, and the number of strings found is returned.
pub unsafe fn from_c_multistring<F>(buf: *const libc::c_char,
                                    count: Option<uint>,
                                    mut f: F)
                                    -> uint where
    F: FnMut(&CString),
{

    let mut curr_ptr: uint = buf as uint;
    let mut ctr = 0;
    let (limited_count, limit) = match count {
        Some(limit) => (true, limit),
        None => (false, 0)
    };
    while ((limited_count && ctr < limit) || !limited_count)
          && *(curr_ptr as *const libc::c_char) != 0 as libc::c_char {
        let cstr = CString::new(curr_ptr as *const libc::c_char, false);
        f(&cstr);
        curr_ptr += cstr.len() + 1;
        ctr += 1;
    }
    return ctr;
}

#[cfg(test)]
mod tests {
    use prelude::v1::*;
    use super::*;
    use ptr;
    use thread::Thread;
    use libc;

    #[test]
    fn test_str_multistring_parsing() {
        unsafe {
            let input = b"zero\0one\0\0";
            let ptr = input.as_ptr();
            let expected = ["zero", "one"];
            let mut it = expected.iter();
            let result = from_c_multistring(ptr as *const libc::c_char, None, |c| {
                let cbytes = c.as_bytes_no_nul();
                assert_eq!(cbytes, it.next().unwrap().as_bytes());
            });
            assert_eq!(result, 2);
            assert!(it.next().is_none());
        }
    }

    #[test]
    fn test_str_to_c_str() {
        let c_str = "".to_c_str();
        unsafe {
            assert_eq!(*c_str.as_ptr().offset(0), 0);
        }

        let c_str = "hello".to_c_str();
        let buf = c_str.as_ptr();
        unsafe {
            assert_eq!(*buf.offset(0), 'h' as libc::c_char);
            assert_eq!(*buf.offset(1), 'e' as libc::c_char);
            assert_eq!(*buf.offset(2), 'l' as libc::c_char);
            assert_eq!(*buf.offset(3), 'l' as libc::c_char);
            assert_eq!(*buf.offset(4), 'o' as libc::c_char);
            assert_eq!(*buf.offset(5), 0);
        }
    }

    #[test]
    fn test_vec_to_c_str() {
        let b: &[u8] = &[];
        let c_str = b.to_c_str();
        unsafe {
            assert_eq!(*c_str.as_ptr().offset(0), 0);
        }

        let c_str = b"hello".to_c_str();
        let buf = c_str.as_ptr();
        unsafe {
            assert_eq!(*buf.offset(0), 'h' as libc::c_char);
            assert_eq!(*buf.offset(1), 'e' as libc::c_char);
            assert_eq!(*buf.offset(2), 'l' as libc::c_char);
            assert_eq!(*buf.offset(3), 'l' as libc::c_char);
            assert_eq!(*buf.offset(4), 'o' as libc::c_char);
            assert_eq!(*buf.offset(5), 0);
        }

        let c_str = b"foo\xFF".to_c_str();
        let buf = c_str.as_ptr();
        unsafe {
            assert_eq!(*buf.offset(0), 'f' as libc::c_char);
            assert_eq!(*buf.offset(1), 'o' as libc::c_char);
            assert_eq!(*buf.offset(2), 'o' as libc::c_char);
            assert_eq!(*buf.offset(3), 0xffu8 as libc::c_char);
            assert_eq!(*buf.offset(4), 0);
        }
    }

    #[test]
    fn test_unwrap() {
        let c_str = "hello".to_c_str();
        unsafe { libc::free(c_str.into_inner() as *mut libc::c_void) }
    }

    #[test]
    fn test_as_ptr() {
        let c_str = "hello".to_c_str();
        let len = unsafe { libc::strlen(c_str.as_ptr()) };
        assert_eq!(len, 5);
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

    #[test]
    fn test_to_c_str_fail() {
        assert!(Thread::spawn(move|| { "he\x00llo".to_c_str() }).join().is_err());
    }

    #[test]
    fn test_to_c_str_unchecked() {
        unsafe {
            let c_string = "he\x00llo".to_c_str_unchecked();
            let buf = c_string.as_ptr();
            assert_eq!(*buf.offset(0), 'h' as libc::c_char);
            assert_eq!(*buf.offset(1), 'e' as libc::c_char);
            assert_eq!(*buf.offset(2), 0);
            assert_eq!(*buf.offset(3), 'l' as libc::c_char);
            assert_eq!(*buf.offset(4), 'l' as libc::c_char);
            assert_eq!(*buf.offset(5), 'o' as libc::c_char);
            assert_eq!(*buf.offset(6), 0);
        }
    }

    #[test]
    fn test_as_bytes() {
        let c_str = "hello".to_c_str();
        assert_eq!(c_str.as_bytes(), b"hello\0");
        let c_str = "".to_c_str();
        assert_eq!(c_str.as_bytes(), b"\0");
        let c_str = b"foo\xFF".to_c_str();
        assert_eq!(c_str.as_bytes(), b"foo\xFF\0");
    }

    #[test]
    fn test_as_bytes_no_nul() {
        let c_str = "hello".to_c_str();
        assert_eq!(c_str.as_bytes_no_nul(), b"hello");
        let c_str = "".to_c_str();
        let exp: &[u8] = &[];
        assert_eq!(c_str.as_bytes_no_nul(), exp);
        let c_str = b"foo\xFF".to_c_str();
        assert_eq!(c_str.as_bytes_no_nul(), b"foo\xFF");
    }

    #[test]
    fn test_as_str() {
        let c_str = "hello".to_c_str();
        assert_eq!(c_str.as_str(), Some("hello"));
        let c_str = "".to_c_str();
        assert_eq!(c_str.as_str(), Some(""));
        let c_str = b"foo\xFF".to_c_str();
        assert_eq!(c_str.as_str(), None);
    }

    #[test]
    #[should_fail]
    fn test_new_fail() {
        let _c_str = unsafe { CString::new(ptr::null(), false) };
    }

    #[test]
    fn test_clone() {
        let a = "hello".to_c_str();
        let b = a.clone();
        assert!(a == b);
    }

    #[test]
    fn test_clone_noleak() {
        fn foo<F>(f: F) where F: FnOnce(&CString) {
            let s = "test".to_string();
            let c = s.to_c_str();
            // give the closure a non-owned CString
            let mut c_ = unsafe { CString::new(c.as_ptr(), false) };
            f(&c_);
            // muck with the buffer for later printing
            unsafe { *c_.as_mut_ptr() = 'X' as libc::c_char }
        }

        let mut c_: Option<CString> = None;
        foo(|c| {
            c_ = Some(c.clone());
            c.clone();
            // force a copy, reading the memory
            c.as_bytes().to_vec();
        });
        let c_ = c_.unwrap();
        // force a copy, reading the memory
        c_.as_bytes().to_vec();
    }
}

#[cfg(test)]
mod bench {
    extern crate test;

    use prelude::v1::*;
    use self::test::Bencher;
    use libc;
    use c_str::ToCStr;

    #[inline]
    fn check(s: &str, c_str: *const libc::c_char) {
        let s_buf = s.as_ptr();
        for i in range(0, s.len()) {
            unsafe {
                assert_eq!(
                    *s_buf.offset(i as int) as libc::c_char,
                    *c_str.offset(i as int));
            }
        }
    }

    static S_SHORT: &'static str = "Mary";
    static S_MEDIUM: &'static str = "Mary had a little lamb";
    static S_LONG: &'static str = "\
        Mary had a little lamb, Little lamb
        Mary had a little lamb, Little lamb
        Mary had a little lamb, Little lamb
        Mary had a little lamb, Little lamb
        Mary had a little lamb, Little lamb
        Mary had a little lamb, Little lamb";

    fn bench_to_string(b: &mut Bencher, s: &str) {
        b.iter(|| {
            let c_str = s.to_c_str();
            check(s, c_str.as_ptr());
        })
    }

    #[bench]
    fn bench_to_c_str_short(b: &mut Bencher) {
        bench_to_string(b, S_SHORT)
    }

    #[bench]
    fn bench_to_c_str_medium(b: &mut Bencher) {
        bench_to_string(b, S_MEDIUM)
    }

    #[bench]
    fn bench_to_c_str_long(b: &mut Bencher) {
        bench_to_string(b, S_LONG)
    }

    fn bench_to_c_str_unchecked(b: &mut Bencher, s: &str) {
        b.iter(|| {
            let c_str = unsafe { s.to_c_str_unchecked() };
            check(s, c_str.as_ptr())
        })
    }

    #[bench]
    fn bench_to_c_str_unchecked_short(b: &mut Bencher) {
        bench_to_c_str_unchecked(b, S_SHORT)
    }

    #[bench]
    fn bench_to_c_str_unchecked_medium(b: &mut Bencher) {
        bench_to_c_str_unchecked(b, S_MEDIUM)
    }

    #[bench]
    fn bench_to_c_str_unchecked_long(b: &mut Bencher) {
        bench_to_c_str_unchecked(b, S_LONG)
    }

    fn bench_with_c_str(b: &mut Bencher, s: &str) {
        b.iter(|| {
            s.with_c_str(|c_str_buf| check(s, c_str_buf))
        })
    }

    #[bench]
    fn bench_with_c_str_short(b: &mut Bencher) {
        bench_with_c_str(b, S_SHORT)
    }

    #[bench]
    fn bench_with_c_str_medium(b: &mut Bencher) {
        bench_with_c_str(b, S_MEDIUM)
    }

    #[bench]
    fn bench_with_c_str_long(b: &mut Bencher) {
        bench_with_c_str(b, S_LONG)
    }

    fn bench_with_c_str_unchecked(b: &mut Bencher, s: &str) {
        b.iter(|| {
            unsafe {
                s.with_c_str_unchecked(|c_str_buf| check(s, c_str_buf))
            }
        })
    }

    #[bench]
    fn bench_with_c_str_unchecked_short(b: &mut Bencher) {
        bench_with_c_str_unchecked(b, S_SHORT)
    }

    #[bench]
    fn bench_with_c_str_unchecked_medium(b: &mut Bencher) {
        bench_with_c_str_unchecked(b, S_MEDIUM)
    }

    #[bench]
    fn bench_with_c_str_unchecked_long(b: &mut Bencher) {
        bench_with_c_str_unchecked(b, S_LONG)
    }
}
