// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use cast;
use char::Char;
use clone::Clone;
use container::Container;
use default::Default;
use intrinsics;
use iter::{Iterator, FromIterator};
use mem;
use num::{CheckedMul, CheckedAdd};
use option::{Some, None};
use ptr::RawPtr;
use ptr;
use raw::Vec;
use slice::ImmutableVector;
use str::StrSlice;

#[cfg(not(test))] use ops::Add;
#[cfg(not(test))] use slice::Vector;

#[allow(ctypes)]
extern {
    fn malloc(size: uint) -> *u8;
    fn free(ptr: *u8);
}

unsafe fn alloc(cap: uint) -> *mut Vec<()> {
    let cap = cap.checked_add(&mem::size_of::<Vec<()>>()).unwrap();
    let ret = malloc(cap) as *mut Vec<()>;
    if ret.is_null() {
        intrinsics::abort();
    }
    (*ret).fill = 0;
    (*ret).alloc = cap;
    ret
}

// Strings

impl Default for ~str {
    fn default() -> ~str {
        unsafe {
            // Get some memory
            let ptr = alloc(0);

            // Initialize the memory
            (*ptr).fill = 0;
            (*ptr).alloc = 0;

            cast::transmute(ptr)
        }
    }
}

impl Clone for ~str {
    fn clone(&self) -> ~str {
        // Don't use the clone() implementation above because it'll start
        // requiring the eh_personality lang item (no fun)
        unsafe {
            let bytes = self.as_bytes().as_ptr();
            let len = self.len();

            let ptr = alloc(len) as *mut Vec<u8>;
            ptr::copy_nonoverlapping_memory(&mut (*ptr).data, bytes, len);
            (*ptr).fill = len;
            (*ptr).alloc = len;

            cast::transmute(ptr)
        }
    }
}

impl FromIterator<char> for ~str {
    #[inline]
    fn from_iter<T: Iterator<char>>(mut iterator: T) -> ~str {
        let (lower, _) = iterator.size_hint();
        let mut cap = if lower == 0 {16} else {lower};
        let mut len = 0;
        let mut tmp = [0u8, ..4];

        unsafe {
            let mut ptr = alloc(cap) as *mut Vec<u8>;
            let mut ret = cast::transmute(ptr);
            for ch in iterator {
                let amt = ch.encode_utf8(tmp);

                if len + amt > cap {
                    cap = cap.checked_mul(&2).unwrap();
                    if cap < len + amt {
                        cap = len + amt;
                    }
                    let ptr2 = alloc(cap) as *mut Vec<u8>;
                    ptr::copy_nonoverlapping_memory(&mut (*ptr2).data,
                                                    &(*ptr).data,
                                                    len);
                    free(ptr as *u8);
                    cast::forget(ret);
                    ret = cast::transmute(ptr2);
                    ptr = ptr2;
                }

                let base = &mut (*ptr).data as *mut u8;
                for byte in tmp.slice_to(amt).iter() {
                    *base.offset(len as int) = *byte;
                    len += 1;
                }
                (*ptr).fill = len;
            }
            ret
        }
    }
}

#[cfg(not(test))]
impl<'a> Add<&'a str,~str> for &'a str {
    #[inline]
    fn add(&self, rhs: & &'a str) -> ~str {
        let amt = self.len().checked_add(&rhs.len()).unwrap();
        unsafe {
            let ptr = alloc(amt) as *mut Vec<u8>;
            let base = &mut (*ptr).data as *mut _;
            ptr::copy_nonoverlapping_memory(base,
                                            self.as_bytes().as_ptr(),
                                            self.len());
            let base = base.offset(self.len() as int);
            ptr::copy_nonoverlapping_memory(base,
                                            rhs.as_bytes().as_ptr(),
                                            rhs.len());
            (*ptr).fill = amt;
            (*ptr).alloc = amt;
            cast::transmute(ptr)
        }
    }
}

// Arrays

impl<A: Clone> Clone for ~[A] {
    #[inline]
    fn clone(&self) -> ~[A] {
        self.iter().map(|a| a.clone()).collect()
    }
}

impl<A> FromIterator<A> for ~[A] {
    fn from_iter<T: Iterator<A>>(mut iterator: T) -> ~[A] {
        let (lower, _) = iterator.size_hint();
        let cap = if lower == 0 {16} else {lower};
        let mut cap = cap.checked_mul(&mem::size_of::<A>()).unwrap();
        let mut len = 0;

        unsafe {
            let mut ptr = alloc(cap) as *mut Vec<A>;
            let mut ret = cast::transmute(ptr);
            for elt in iterator {
                if len * mem::size_of::<A>() >= cap {
                    cap = cap.checked_mul(&2).unwrap();
                    let ptr2 = alloc(cap) as *mut Vec<A>;
                    ptr::copy_nonoverlapping_memory(&mut (*ptr2).data,
                                                    &(*ptr).data,
                                                    len);
                    free(ptr as *u8);
                    cast::forget(ret);
                    ret = cast::transmute(ptr2);
                    ptr = ptr2;
                }

                let base = &mut (*ptr).data as *mut A;
                intrinsics::move_val_init(&mut *base.offset(len as int), elt);
                len += 1;
                (*ptr).fill = len * mem::nonzero_size_of::<A>();
            }
            ret
        }
    }
}

#[cfg(not(test))]
impl<'a,T:Clone, V: Vector<T>> Add<V, ~[T]> for &'a [T] {
    #[inline]
    fn add(&self, rhs: &V) -> ~[T] {
        let first = self.iter().map(|t| t.clone());
        first.chain(rhs.as_slice().iter().map(|t| t.clone())).collect()
    }
}

#[cfg(not(test))]
impl<T:Clone, V: Vector<T>> Add<V, ~[T]> for ~[T] {
    #[inline]
    fn add(&self, rhs: &V) -> ~[T] {
        self.as_slice() + rhs.as_slice()
    }
}
