// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![unstable(feature = "std_misc")]

use borrow::{Cow, ToOwned};
use boxed::{self, Box};
use clone::Clone;
use convert::{Into, From};
use cmp::{PartialEq, Eq, PartialOrd, Ord, Ordering};
use error::Error;
use fmt;
use io;
use iter::Iterator;
use libc;
use mem;
use ops::Deref;
use option::Option::{self, Some, None};
use result::Result::{self, Ok, Err};
use slice;
use str;
use string::String;
use vec::Vec;

/// A type representing an owned C-compatible string
///
/// This type serves the primary purpose of being able to safely generate a
/// C-compatible string from a Rust byte slice or vector. An instance of this
/// type is a static guarantee that the underlying bytes contain no interior 0
/// bytes and the final byte is 0.
///
/// A `CString` is created from either a byte slice or a byte vector. After
/// being created, a `CString` predominately inherits all of its methods from
/// the `Deref` implementation to `[libc::c_char]`. Note that the underlying
/// array is represented as an array of `libc::c_char` as opposed to `u8`. A
/// `u8` slice can be obtained with the `as_bytes` method.  Slices produced from
/// a `CString` do *not* contain the trailing nul terminator unless otherwise
/// specified.
///
/// # Examples
///
/// ```no_run
/// # #![feature(libc)]
/// # extern crate libc;
/// # fn main() {
/// use std::ffi::CString;
/// use libc;
///
/// extern {
///     fn my_printer(s: *const libc::c_char);
/// }
///
/// let c_to_print = CString::new("Hello, world!").unwrap();
/// unsafe {
///     my_printer(c_to_print.as_ptr());
/// }
/// # }
/// ```
#[derive(PartialEq, PartialOrd, Eq, Ord, Hash)]
#[stable(feature = "rust1", since = "1.0.0")]
pub struct CString {
    inner: Box<[u8]>,
}

/// Representation of a borrowed C string.
///
/// This dynamically sized type is only safely constructed via a borrowed
/// version of an instance of `CString`. This type can be constructed from a raw
/// C string as well and represents a C string borrowed from another location.
///
/// Note that this structure is **not** `repr(C)` and is not recommended to be
/// placed in the signatures of FFI functions. Instead safe wrappers of FFI
/// functions may leverage the unsafe `from_ptr` constructor to provide a safe
/// interface to other consumers.
///
/// # Examples
///
/// Inspecting a foreign C string
///
/// ```no_run
/// # #![feature(libc)]
/// extern crate libc;
/// use std::ffi::CStr;
///
/// extern { fn my_string() -> *const libc::c_char; }
///
/// fn main() {
///     unsafe {
///         let slice = CStr::from_ptr(my_string());
///         println!("string length: {}", slice.to_bytes().len());
///     }
/// }
/// ```
///
/// Passing a Rust-originating C string
///
/// ```no_run
/// # #![feature(libc)]
/// extern crate libc;
/// use std::ffi::{CString, CStr};
///
/// fn work(data: &CStr) {
///     extern { fn work_with(data: *const libc::c_char); }
///
///     unsafe { work_with(data.as_ptr()) }
/// }
///
/// fn main() {
///     let s = CString::new("data data data data").unwrap();
///     work(&s);
/// }
/// ```
///
/// Converting a foreign C string into a Rust `String`
///
/// ```no_run
/// # #![feature(libc,cstr_to_str)]
/// extern crate libc;
/// use std::ffi::CStr;
///
/// extern { fn my_string() -> *const libc::c_char; }
///
/// fn my_string_safe() -> String {
///     unsafe {
///         CStr::from_ptr(my_string()).to_string_lossy().into_owned()
///     }
/// }
///
/// fn main() {
///     println!("string: {}", my_string_safe());
/// }
/// ```
#[derive(Hash)]
#[stable(feature = "rust1", since = "1.0.0")]
pub struct CStr {
    // FIXME: this should not be represented with a DST slice but rather with
    //        just a raw `libc::c_char` along with some form of marker to make
    //        this an unsized type. Essentially `sizeof(&CStr)` should be the
    //        same as `sizeof(&c_char)` but `CStr` should be an unsized type.
    inner: [libc::c_char]
}

/// An error returned from `CString::new` to indicate that a nul byte was found
/// in the vector provided.
#[derive(Clone, PartialEq, Debug)]
#[stable(feature = "rust1", since = "1.0.0")]
pub struct NulError(usize, Vec<u8>);

impl CString {
    /// Creates a new C-compatible string from a container of bytes.
    ///
    /// This method will consume the provided data and use the underlying bytes
    /// to construct a new string, ensuring that there is a trailing 0 byte.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #![feature(libc)]
    /// extern crate libc;
    /// use std::ffi::CString;
    ///
    /// extern { fn puts(s: *const libc::c_char); }
    ///
    /// fn main() {
    ///     let to_print = CString::new("Hello!").unwrap();
    ///     unsafe {
    ///         puts(to_print.as_ptr());
    ///     }
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// This function will return an error if the bytes yielded contain an
    /// internal 0 byte. The error returned will contain the bytes as well as
    /// the position of the nul byte.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn new<T: Into<Vec<u8>>>(t: T) -> Result<CString, NulError> {
        let bytes = t.into();
        match bytes.iter().position(|x| *x == 0) {
            Some(i) => Err(NulError(i, bytes)),
            None => Ok(unsafe { CString::from_vec_unchecked(bytes) }),
        }
    }

    /// Creates a C-compatible string from a byte vector without checking for
    /// interior 0 bytes.
    ///
    /// This method is equivalent to `new` except that no runtime assertion
    /// is made that `v` contains no 0 bytes, and it requires an actual
    /// byte vector, not anything that can be converted to one with Into.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub unsafe fn from_vec_unchecked(mut v: Vec<u8>) -> CString {
        v.push(0);
        CString { inner: v.into_boxed_slice() }
    }

    /// Retakes ownership of a CString that was transferred to C.
    ///
    /// The only appropriate argument is a pointer obtained by calling
    /// `into_ptr`. The length of the string will be recalculated
    /// using the pointer.
    #[unstable(feature = "cstr_memory", reason = "recently added")]
    pub unsafe fn from_ptr(ptr: *const libc::c_char) -> CString {
        let len = libc::strlen(ptr) + 1; // Including the NUL byte
        let slice = slice::from_raw_parts(ptr, len as usize);
        CString { inner: mem::transmute(slice) }
    }

    /// Transfers ownership of the string to a C caller.
    ///
    /// The pointer must be returned to Rust and reconstituted using
    /// `from_ptr` to be properly deallocated. Specifically, one
    /// should *not* use the standard C `free` function to deallocate
    /// this string.
    ///
    /// Failure to call `from_ptr` will lead to a memory leak.
    #[unstable(feature = "cstr_memory", reason = "recently added")]
    pub fn into_ptr(self) -> *const libc::c_char {
        // It is important that the bytes be sized to fit - we need
        // the capacity to be determinable from the string length, and
        // shrinking to fit is the only way to be sure.
        boxed::into_raw(self.inner) as *const libc::c_char
    }

    /// Returns the contents of this `CString` as a slice of bytes.
    ///
    /// The returned slice does **not** contain the trailing nul separator and
    /// it is guaranteed to not have any interior nul bytes.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn as_bytes(&self) -> &[u8] {
        &self.inner[..self.inner.len() - 1]
    }

    /// Equivalent to the `as_bytes` function except that the returned slice
    /// includes the trailing nul byte.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn as_bytes_with_nul(&self) -> &[u8] {
        &self.inner
    }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl Clone for CString {
    fn clone(&self) -> Self {
        CString { inner: self.inner.to_owned().into_boxed_slice() }
    }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl Deref for CString {
    type Target = CStr;

    fn deref(&self) -> &CStr {
        unsafe { mem::transmute(self.as_bytes_with_nul()) }
    }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl fmt::Debug for CString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&String::from_utf8_lossy(self.as_bytes()), f)
    }
}

impl NulError {
    /// Returns the position of the nul byte in the slice that was provided to
    /// `CString::new`.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn nul_position(&self) -> usize { self.0 }

    /// Consumes this error, returning the underlying vector of bytes which
    /// generated the error in the first place.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn into_vec(self) -> Vec<u8> { self.1 }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl Error for NulError {
    fn description(&self) -> &str { "nul byte found in data" }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl fmt::Display for NulError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "nul byte found in provided data at position: {}", self.0)
    }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl From<NulError> for io::Error {
    fn from(_: NulError) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidInput,
                       "data provided contains a nul byte")
    }
}

impl CStr {
    /// Casts a raw C string to a safe C string wrapper.
    ///
    /// This function will cast the provided `ptr` to the `CStr` wrapper which
    /// allows inspection and interoperation of non-owned C strings. This method
    /// is unsafe for a number of reasons:
    ///
    /// * There is no guarantee to the validity of `ptr`
    /// * The returned lifetime is not guaranteed to be the actual lifetime of
    ///   `ptr`
    /// * There is no guarantee that the memory pointed to by `ptr` contains a
    ///   valid nul terminator byte at the end of the string.
    ///
    /// > **Note**: This operation is intended to be a 0-cost cast but it is
    /// > currently implemented with an up-front calculation of the length of
    /// > the string. This is not guaranteed to always be the case.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #![feature(libc)]
    /// # extern crate libc;
    /// # fn main() {
    /// use std::ffi::CStr;
    /// use std::str;
    /// use libc;
    ///
    /// extern {
    ///     fn my_string() -> *const libc::c_char;
    /// }
    ///
    /// unsafe {
    ///     let slice = CStr::from_ptr(my_string());
    ///     println!("string returned: {}",
    ///              str::from_utf8(slice.to_bytes()).unwrap());
    /// }
    /// # }
    /// ```
    #[stable(feature = "rust1", since = "1.0.0")]
    pub unsafe fn from_ptr<'a>(ptr: *const libc::c_char) -> &'a CStr {
        let len = libc::strlen(ptr);
        mem::transmute(slice::from_raw_parts(ptr, len as usize + 1))
    }

    /// Returns the inner pointer to this C string.
    ///
    /// The returned pointer will be valid for as long as `self` is and points
    /// to a contiguous region of memory terminated with a 0 byte to represent
    /// the end of the string.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn as_ptr(&self) -> *const libc::c_char {
        self.inner.as_ptr()
    }

    /// Converts this C string to a byte slice.
    ///
    /// This function will calculate the length of this string (which normally
    /// requires a linear amount of work to be done) and then return the
    /// resulting slice of `u8` elements.
    ///
    /// The returned slice will **not** contain the trailing nul that this C
    /// string has.
    ///
    /// > **Note**: This method is currently implemented as a 0-cost cast, but
    /// > it is planned to alter its definition in the future to perform the
    /// > length calculation whenever this method is called.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn to_bytes(&self) -> &[u8] {
        let bytes = self.to_bytes_with_nul();
        &bytes[..bytes.len() - 1]
    }

    /// Converts this C string to a byte slice containing the trailing 0 byte.
    ///
    /// This function is the equivalent of `to_bytes` except that it will retain
    /// the trailing nul instead of chopping it off.
    ///
    /// > **Note**: This method is currently implemented as a 0-cost cast, but
    /// > it is planned to alter its definition in the future to perform the
    /// > length calculation whenever this method is called.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn to_bytes_with_nul(&self) -> &[u8] {
        unsafe { mem::transmute::<&[libc::c_char], &[u8]>(&self.inner) }
    }

    /// Yields a `&str` slice if the `CStr` contains valid UTF-8.
    ///
    /// This function will calculate the length of this string and check for
    /// UTF-8 validity, and then return the `&str` if it's valid.
    ///
    /// > **Note**: This method is currently implemented to check for validity
    /// > after a 0-cost cast, but it is planned to alter its definition in the
    /// > future to perform the length calculation in addition to the UTF-8
    /// > check whenever this method is called.
    #[unstable(feature = "cstr_to_str", reason = "recently added")]
    pub fn to_str(&self) -> Result<&str, str::Utf8Error> {
        // NB: When CStr is changed to perform the length check in .to_bytes() instead of in
        // from_ptr(), it may be worth considering if this should be rewritten to do the UTF-8
        // check inline with the length calculation instead of doing it afterwards.
        str::from_utf8(self.to_bytes())
    }

    /// Converts a `CStr` into a `Cow<str>`.
    ///
    /// This function will calculate the length of this string (which normally
    /// requires a linear amount of work to be done) and then return the
    /// resulting slice as a `Cow<str>`, replacing any invalid UTF-8 sequences
    /// with `U+FFFD REPLACEMENT CHARACTER`.
    ///
    /// > **Note**: This method is currently implemented to check for validity
    /// > after a 0-cost cast, but it is planned to alter its definition in the
    /// > future to perform the length calculation in addition to the UTF-8
    /// > check whenever this method is called.
    #[unstable(feature = "cstr_to_str", reason = "recently added")]
    pub fn to_string_lossy(&self) -> Cow<str> {
        String::from_utf8_lossy(self.to_bytes())
    }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl PartialEq for CStr {
    fn eq(&self, other: &CStr) -> bool {
        self.to_bytes().eq(other.to_bytes())
    }
}
#[stable(feature = "rust1", since = "1.0.0")]
impl Eq for CStr {}
#[stable(feature = "rust1", since = "1.0.0")]
impl PartialOrd for CStr {
    fn partial_cmp(&self, other: &CStr) -> Option<Ordering> {
        self.to_bytes().partial_cmp(&other.to_bytes())
    }
}
#[stable(feature = "rust1", since = "1.0.0")]
impl Ord for CStr {
    fn cmp(&self, other: &CStr) -> Ordering {
        self.to_bytes().cmp(&other.to_bytes())
    }
}

#[cfg(test)]
mod tests {
    use prelude::v1::*;
    use super::*;
    use libc;
    use borrow::Cow::{Borrowed, Owned};

    #[test]
    fn c_to_rust() {
        let data = b"123\0";
        let ptr = data.as_ptr() as *const libc::c_char;
        unsafe {
            assert_eq!(CStr::from_ptr(ptr).to_bytes(), b"123");
            assert_eq!(CStr::from_ptr(ptr).to_bytes_with_nul(), b"123\0");
        }
    }

    #[test]
    fn simple() {
        let s = CString::new("1234").unwrap();
        assert_eq!(s.as_bytes(), b"1234");
        assert_eq!(s.as_bytes_with_nul(), b"1234\0");
    }

    #[test]
    fn build_with_zero1() {
        assert!(CString::new(&b"\0"[..]).is_err());
    }
    #[test]
    fn build_with_zero2() {
        assert!(CString::new(vec![0]).is_err());
    }

    #[test]
    fn build_with_zero3() {
        unsafe {
            let s = CString::from_vec_unchecked(vec![0]);
            assert_eq!(s.as_bytes(), b"\0");
        }
    }

    #[test]
    fn formatted() {
        let s = CString::new(&b"12"[..]).unwrap();
        assert_eq!(format!("{:?}", s), "\"12\"");
    }

    #[test]
    fn borrowed() {
        unsafe {
            let s = CStr::from_ptr(b"12\0".as_ptr() as *const _);
            assert_eq!(s.to_bytes(), b"12");
            assert_eq!(s.to_bytes_with_nul(), b"12\0");
        }
    }

    #[test]
    fn to_str() {
        let data = b"123\xE2\x80\xA6\0";
        let ptr = data.as_ptr() as *const libc::c_char;
        unsafe {
            assert_eq!(CStr::from_ptr(ptr).to_str(), Ok("123…"));
            assert_eq!(CStr::from_ptr(ptr).to_string_lossy(), Borrowed("123…"));
        }
        let data = b"123\xE2\0";
        let ptr = data.as_ptr() as *const libc::c_char;
        unsafe {
            assert!(CStr::from_ptr(ptr).to_str().is_err());
            assert_eq!(CStr::from_ptr(ptr).to_string_lossy(), Owned::<str>(format!("123\u{FFFD}")));
        }
    }
}
