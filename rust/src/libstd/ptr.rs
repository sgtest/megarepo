// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Unsafe pointer utility functions

use cast;
use libc;
use libc::{c_void, size_t};
use option::{Option, Some, None};
use sys;

#[cfg(not(test))] use cmp::{Eq, Ord};
use uint;

pub mod libc_ {
    use libc::c_void;
    use libc;

    #[nolink]
    #[abi = "cdecl"]
    pub extern {
        #[rust_stack]
        unsafe fn memmove(dest: *mut c_void,
                          src: *const c_void,
                          n: libc::size_t)
                       -> *c_void;

        #[rust_stack]
        unsafe fn memset(dest: *mut c_void,
                         c: libc::c_int,
                         len: libc::size_t)
                      -> *c_void;
    }
}

/// Calculate the offset from a pointer
#[inline(always)]
pub fn offset<T>(ptr: *T, count: uint) -> *T {
    (ptr as uint + count * sys::size_of::<T>()) as *T
}

/// Calculate the offset from a const pointer
#[inline(always)]
pub fn const_offset<T>(ptr: *const T, count: uint) -> *const T {
    (ptr as uint + count * sys::size_of::<T>()) as *T
}

/// Calculate the offset from a mut pointer
#[inline(always)]
pub fn mut_offset<T>(ptr: *mut T, count: uint) -> *mut T {
    (ptr as uint + count * sys::size_of::<T>()) as *mut T
}

/// Return the offset of the first null pointer in `buf`.
#[inline(always)]
pub unsafe fn buf_len<T>(buf: **T) -> uint {
    position(buf, |i| *i == null())
}

/// Return the first offset `i` such that `f(buf[i]) == true`.
#[inline(always)]
pub unsafe fn position<T>(buf: *T, f: &fn(&T) -> bool) -> uint {
    let mut i = 0;
    loop {
        if f(&(*offset(buf, i))) { return i; }
        else { i += 1; }
    }
}

/// Create an unsafe null pointer
#[inline(always)]
pub fn null<T>() -> *T { unsafe { cast::transmute(0u) } }

/// Create an unsafe mutable null pointer
#[inline(always)]
pub fn mut_null<T>() -> *mut T { unsafe { cast::transmute(0u) } }

/// Returns true if the pointer is equal to the null pointer.
#[inline(always)]
pub fn is_null<T>(ptr: *const T) -> bool { ptr == null() }

/// Returns true if the pointer is not equal to the null pointer.
#[inline(always)]
pub fn is_not_null<T>(ptr: *const T) -> bool { !is_null(ptr) }

/**
 * Copies data from one location to another
 *
 * Copies `count` elements (not bytes) from `src` to `dst`. The source
 * and destination may overlap.
 */
#[inline(always)]
#[cfg(target_word_size = "32")]
pub unsafe fn copy_memory<T>(dst: *mut T, src: *const T, count: uint) {
    use unstable::intrinsics::memmove32;
    let n = count * sys::size_of::<T>();
    memmove32(dst as *mut u8, src as *u8, n as u32);
}

#[inline(always)]
#[cfg(target_word_size = "64")]
pub unsafe fn copy_memory<T>(dst: *mut T, src: *const T, count: uint) {
    use unstable::intrinsics::memmove64;
    let n = count * sys::size_of::<T>();
    memmove64(dst as *mut u8, src as *u8, n as u64);
}

#[inline(always)]
#[cfg(target_word_size = "32")]
pub unsafe fn copy_nonoverlapping_memory<T>(dst: *mut T, src: *const T, count: uint) {
    #[cfg(stage0)]
    use memcpy32 = unstable::intrinsics::memmove32;
    #[cfg(not(stage0))]
    use unstable::intrinsics::memcpy32;
    let n = count * sys::size_of::<T>();
    memcpy32(dst as *mut u8, src as *u8, n as u32);
}

#[inline(always)]
#[cfg(target_word_size = "64")]
pub unsafe fn copy_nonoverlapping_memory<T>(dst: *mut T, src: *const T, count: uint) {
    #[cfg(stage0)]
    use memcpy64 = unstable::intrinsics::memmove64;
    #[cfg(not(stage0))]
    use unstable::intrinsics::memcpy64;
    let n = count * sys::size_of::<T>();
    memcpy64(dst as *mut u8, src as *u8, n as u64);
}

#[inline(always)]
pub unsafe fn set_memory<T>(dst: *mut T, c: int, count: uint) {
    let n = count * sys::size_of::<T>();
    libc_::memset(dst as *mut c_void, c as libc::c_int, n as size_t);
}

/**
  Transform a region pointer - &T - to an unsafe pointer - *T.
  This is safe, but is implemented with an unsafe block due to
  transmute.
*/
#[inline(always)]
pub fn to_unsafe_ptr<T>(thing: &T) -> *T {
    unsafe { cast::transmute(thing) }
}

/**
  Transform a const region pointer - &const T - to a const unsafe pointer -
  *const T. This is safe, but is implemented with an unsafe block due to
  transmute.
*/
#[inline(always)]
pub fn to_const_unsafe_ptr<T>(thing: &const T) -> *const T {
    unsafe { cast::transmute(thing) }
}

/**
  Transform a mutable region pointer - &mut T - to a mutable unsafe pointer -
  *mut T. This is safe, but is implemented with an unsafe block due to
  transmute.
*/
#[inline(always)]
pub fn to_mut_unsafe_ptr<T>(thing: &mut T) -> *mut T {
    unsafe { cast::transmute(thing) }
}

/**
  Cast a region pointer - &T - to a uint.
  This is safe, but is implemented with an unsafe block due to
  transmute.

  (I couldn't think of a cutesy name for this one.)
*/
#[inline(always)]
pub fn to_uint<T>(thing: &T) -> uint {
    unsafe {
        cast::transmute(thing)
    }
}

/// Determine if two borrowed pointers point to the same thing.
#[inline(always)]
pub fn ref_eq<'a,'b,T>(thing: &'a T, other: &'b T) -> bool {
    to_uint(thing) == to_uint(other)
}

/**
  Given a **T (pointer to an array of pointers),
  iterate through each *T, up to the provided `len`,
  passing to the provided callback function

  SAFETY NOTE: Pointer-arithmetic. Dragons be here.
*/
pub unsafe fn array_each_with_len<T>(arr: **T, len: uint, cb: &fn(*T)) {
    debug!("array_each_with_len: before iterate");
    if (arr as uint == 0) {
        fail!("ptr::array_each_with_len failure: arr input is null pointer");
    }
    //let start_ptr = *arr;
    uint::iterate(0, len, |e| {
        let n = offset(arr, e);
        cb(*n);
        true
    });
    debug!("array_each_with_len: after iterate");
}

/**
  Given a null-pointer-terminated **T (pointer to
  an array of pointers), iterate through each *T,
  passing to the provided callback function

  SAFETY NOTE: This will only work with a null-terminated
  pointer array. Barely less-dodgey Pointer Arithmetic.
  Dragons be here.
*/
pub unsafe fn array_each<T>(arr: **T, cb: &fn(*T)) {
    if (arr as uint == 0) {
        fail!("ptr::array_each_with_len failure: arr input is null pointer");
    }
    let len = buf_len(arr);
    debug!("array_each inferred len: %u",
                    len);
    array_each_with_len(arr, len, cb);
}

pub trait Ptr<T> {
    fn is_null(&const self) -> bool;
    fn is_not_null(&const self) -> bool;
    unsafe fn to_option(&const self) -> Option<&T>;
    fn offset(&self, count: uint) -> Self;
}

/// Extension methods for immutable pointers
impl<T> Ptr<T> for *T {
    /// Returns true if the pointer is equal to the null pointer.
    #[inline(always)]
    fn is_null(&const self) -> bool { is_null(*self) }

    /// Returns true if the pointer is not equal to the null pointer.
    #[inline(always)]
    fn is_not_null(&const self) -> bool { is_not_null(*self) }

    ///
    /// Returns `None` if the pointer is null, or else returns the value wrapped
    /// in `Some`.
    ///
    /// # Safety Notes
    ///
    /// While this method is useful for null-safety, it is important to note
    /// that this is still an unsafe operation because the returned value could
    /// be pointing to invalid memory.
    ///
    #[inline(always)]
    unsafe fn to_option(&const self) -> Option<&T> {
        if self.is_null() { None } else {
            Some(cast::transmute(*self))
        }
    }

    /// Calculates the offset from a pointer.
    #[inline(always)]
    fn offset(&self, count: uint) -> *T { offset(*self, count) }
}

/// Extension methods for mutable pointers
impl<T> Ptr<T> for *mut T {
    /// Returns true if the pointer is equal to the null pointer.
    #[inline(always)]
    fn is_null(&const self) -> bool { is_null(*self) }

    /// Returns true if the pointer is not equal to the null pointer.
    #[inline(always)]
    fn is_not_null(&const self) -> bool { is_not_null(*self) }

    ///
    /// Returns `None` if the pointer is null, or else returns the value wrapped
    /// in `Some`.
    ///
    /// # Safety Notes
    ///
    /// While this method is useful for null-safety, it is important to note
    /// that this is still an unsafe operation because the returned value could
    /// be pointing to invalid memory.
    ///
    #[inline(always)]
    unsafe fn to_option(&const self) -> Option<&T> {
        if self.is_null() { None } else {
            Some(cast::transmute(*self))
        }
    }

    /// Calculates the offset from a mutable pointer.
    #[inline(always)]
    fn offset(&self, count: uint) -> *mut T { mut_offset(*self, count) }
}

// Equality for pointers
#[cfg(not(test))]
impl<T> Eq for *const T {
    #[inline(always)]
    fn eq(&self, other: &*const T) -> bool {
        unsafe {
            let a: uint = cast::transmute(*self);
            let b: uint = cast::transmute(*other);
            return a == b;
        }
    }
    #[inline(always)]
    fn ne(&self, other: &*const T) -> bool { !(*self).eq(other) }
}

// Comparison for pointers
#[cfg(not(test))]
impl<T> Ord for *const T {
    #[inline(always)]
    fn lt(&self, other: &*const T) -> bool {
        unsafe {
            let a: uint = cast::transmute(*self);
            let b: uint = cast::transmute(*other);
            return a < b;
        }
    }
    #[inline(always)]
    fn le(&self, other: &*const T) -> bool {
        unsafe {
            let a: uint = cast::transmute(*self);
            let b: uint = cast::transmute(*other);
            return a <= b;
        }
    }
    #[inline(always)]
    fn ge(&self, other: &*const T) -> bool {
        unsafe {
            let a: uint = cast::transmute(*self);
            let b: uint = cast::transmute(*other);
            return a >= b;
        }
    }
    #[inline(always)]
    fn gt(&self, other: &*const T) -> bool {
        unsafe {
            let a: uint = cast::transmute(*self);
            let b: uint = cast::transmute(*other);
            return a > b;
        }
    }
}

// Equality for region pointers
#[cfg(not(test))]
impl<'self,T:Eq> Eq for &'self T {
    #[inline(always)]
    fn eq(&self, other: & &'self T) -> bool {
        return *(*self) == *(*other);
    }
    #[inline(always)]
    fn ne(&self, other: & &'self T) -> bool {
        return *(*self) != *(*other);
    }
}

// Comparison for region pointers
#[cfg(not(test))]
impl<'self,T:Ord> Ord for &'self T {
    #[inline(always)]
    fn lt(&self, other: & &'self T) -> bool {
        *(*self) < *(*other)
    }
    #[inline(always)]
    fn le(&self, other: & &'self T) -> bool {
        *(*self) <= *(*other)
    }
    #[inline(always)]
    fn ge(&self, other: & &'self T) -> bool {
        *(*self) >= *(*other)
    }
    #[inline(always)]
    fn gt(&self, other: & &'self T) -> bool {
        *(*self) > *(*other)
    }
}

#[cfg(test)]
pub mod ptr_tests {
    use super::*;
    use prelude::*;

    #[test]
    fn test() {
        unsafe {
            struct Pair {
                fst: int,
                snd: int
            };
            let mut p = Pair {fst: 10, snd: 20};
            let pptr: *mut Pair = &mut p;
            let iptr: *mut int = cast::transmute(pptr);
            assert_eq!(*iptr, 10);
            *iptr = 30;
            assert_eq!(*iptr, 30);
            assert_eq!(p.fst, 30);

            *pptr = Pair {fst: 50, snd: 60};
            assert_eq!(*iptr, 50);
            assert_eq!(p.fst, 50);
            assert_eq!(p.snd, 60);

            let v0 = ~[32000u16, 32001u16, 32002u16];
            let mut v1 = ~[0u16, 0u16, 0u16];

            copy_memory(mut_offset(vec::raw::to_mut_ptr(v1), 1u),
                        offset(vec::raw::to_ptr(v0), 1u), 1u);
            assert!((v1[0] == 0u16 && v1[1] == 32001u16 && v1[2] == 0u16));
            copy_memory(vec::raw::to_mut_ptr(v1),
                        offset(vec::raw::to_ptr(v0), 2u), 1u);
            assert!((v1[0] == 32002u16 && v1[1] == 32001u16 &&
                     v1[2] == 0u16));
            copy_memory(mut_offset(vec::raw::to_mut_ptr(v1), 2u),
                        vec::raw::to_ptr(v0), 1u);
            assert!((v1[0] == 32002u16 && v1[1] == 32001u16 &&
                     v1[2] == 32000u16));
        }
    }

    #[test]
    fn test_position() {
        use str::as_c_str;
        use libc::c_char;

        let s = ~"hello";
        unsafe {
            assert!(2u == as_c_str(s, |p| position(p,
                                                   |c| *c == 'l' as c_char)));
            assert!(4u == as_c_str(s, |p| position(p,
                                                   |c| *c == 'o' as c_char)));
            assert!(5u == as_c_str(s, |p| position(p,
                                                   |c| *c == 0 as c_char)));
        }
    }

    #[test]
    fn test_buf_len() {
        let s0 = ~"hello";
        let s1 = ~"there";
        let s2 = ~"thing";
        do str::as_c_str(s0) |p0| {
            do str::as_c_str(s1) |p1| {
                do str::as_c_str(s2) |p2| {
                    let v = ~[p0, p1, p2, null()];
                    do vec::as_imm_buf(v) |vp, len| {
                        assert_eq!(unsafe { buf_len(vp) }, 3u);
                        assert_eq!(len, 4u);
                    }
                }
            }
        }
    }

    #[test]
    fn test_is_null() {
        let p: *int = null();
        assert!(p.is_null());
        assert!(!p.is_not_null());

        let q = offset(p, 1u);
        assert!(!q.is_null());
        assert!(q.is_not_null());

        let mp: *mut int = mut_null();
        assert!(mp.is_null());
        assert!(!mp.is_not_null());

        let mq = mp.offset(1u);
        assert!(!mq.is_null());
        assert!(mq.is_not_null());
    }

    #[test]
    fn test_to_option() {
        let p: *int = null();
        // FIXME (#6641): Usage of unsafe methods in safe code doesn't cause an error.
        assert_eq!(p.to_option(), None);

        let q: *int = &2;
        assert_eq!(q.to_option().unwrap(), &2);     // FIXME (#6641)

        let p: *mut int = mut_null();
        assert_eq!(p.to_option(), None);            // FIXME (#6641)

        let q: *mut int = &mut 2;
        assert_eq!(q.to_option().unwrap(), &2);     // FIXME (#6641)
    }

    #[test]
    fn test_ptr_array_each_with_len() {
        unsafe {
            let one = ~"oneOne";
            let two = ~"twoTwo";
            let three = ~"threeThree";
            let arr: ~[*i8] = ~[
                ::cast::transmute(&one[0]),
                ::cast::transmute(&two[0]),
                ::cast::transmute(&three[0]),
            ];
            let expected_arr = [
                one, two, three
            ];
            let arr_ptr = &arr[0];
            let mut ctr = 0;
            let mut iteration_count = 0;
            array_each_with_len(arr_ptr, arr.len(),
                                |e| {
                                         let actual = str::raw::from_c_str(e);
                                         let expected = copy expected_arr[ctr];
                                         debug!(
                                             "test_ptr_array_each e: %s, a: %s",
                                             expected, actual);
                                         assert_eq!(actual, expected);
                                         ctr += 1;
                                         iteration_count += 1;
                                     });
            assert_eq!(iteration_count, 3u);
        }
    }
    #[test]
    fn test_ptr_array_each() {
        unsafe {
            let one = ~"oneOne";
            let two = ~"twoTwo";
            let three = ~"threeThree";
            let arr: ~[*i8] = ~[
                ::cast::transmute(&one[0]),
                ::cast::transmute(&two[0]),
                ::cast::transmute(&three[0]),
                // fake a null terminator
                0 as *i8
            ];
            let expected_arr = [
                one, two, three
            ];
            let arr_ptr = &arr[0];
            let mut ctr = 0;
            let mut iteration_count = 0;
            array_each(arr_ptr, |e| {
                let actual = str::raw::from_c_str(e);
                let expected = copy expected_arr[ctr];
                debug!(
                    "test_ptr_array_each e: %s, a: %s",
                    expected, actual);
                assert_eq!(actual, expected);
                ctr += 1;
                iteration_count += 1;
            });
            assert_eq!(iteration_count, 3);
        }
    }
    #[test]
    #[should_fail]
    #[ignore(cfg(windows))]
    fn test_ptr_array_each_with_len_null_ptr() {
        unsafe {
            array_each_with_len(0 as **libc::c_char, 1, |e| {
                str::raw::from_c_str(e);
            });
        }
    }
    #[test]
    #[should_fail]
    #[ignore(cfg(windows))]
    fn test_ptr_array_each_null_ptr() {
        unsafe {
            array_each(0 as **libc::c_char, |e| {
                str::raw::from_c_str(e);
            });
        }
    }
}
