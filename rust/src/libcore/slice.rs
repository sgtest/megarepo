// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Slice management and manipulation
//!
//! For more details `std::slice`.

#![stable]
#![doc(primitive = "slice")]

// How this module is organized.
//
// The library infrastructure for slices is fairly messy. There's
// a lot of stuff defined here. Let's keep it clean.
//
// Since slices don't support inherent methods; all operations
// on them are defined on traits, which are then reexported from
// the prelude for convenience. So there are a lot of traits here.
//
// The layout of this file is thus:
//
// * Slice-specific 'extension' traits and their implementations. This
//   is where most of the slice API resides.
// * Implementations of a few common traits with important slice ops.
// * Definitions of a bunch of iterators.
// * Free functions.
// * The `raw` and `bytes` submodules.
// * Boilerplate trait implementations.

use mem::transmute;
use clone::Clone;
use cmp::{Ordering, PartialEq, PartialOrd, Eq, Ord};
use cmp::Ordering::{Less, Equal, Greater};
use cmp;
use default::Default;
use iter::*;
use kinds::Copy;
use num::Int;
use ops::{FnMut, self};
use option::Option;
use option::Option::{None, Some};
use result::Result;
use result::Result::{Ok, Err};
use ptr;
use ptr::PtrExt;
use mem;
use mem::size_of;
use kinds::{Sized, marker};
use raw::Repr;
// Avoid conflicts with *both* the Slice trait (buggy) and the `slice::raw` module.
use raw::Slice as RawSlice;


//
// Extension traits
//

/// Extension methods for slices.
#[allow(missing_docs)] // docs in libcollections
pub trait SliceExt for Sized? {
    type Item;

    fn slice<'a>(&'a self, start: uint, end: uint) -> &'a [Self::Item];
    fn slice_from<'a>(&'a self, start: uint) -> &'a [Self::Item];
    fn slice_to<'a>(&'a self, end: uint) -> &'a [Self::Item];
    fn split_at<'a>(&'a self, mid: uint) -> (&'a [Self::Item], &'a [Self::Item]);
    fn iter<'a>(&'a self) -> Iter<'a, Self::Item>;
    fn split<'a, P>(&'a self, pred: P) -> Split<'a, Self::Item, P>
                    where P: FnMut(&Self::Item) -> bool;
    fn splitn<'a, P>(&'a self, n: uint, pred: P) -> SplitN<'a, Self::Item, P>
                     where P: FnMut(&Self::Item) -> bool;
    fn rsplitn<'a, P>(&'a self,  n: uint, pred: P) -> RSplitN<'a, Self::Item, P>
                      where P: FnMut(&Self::Item) -> bool;
    fn windows<'a>(&'a self, size: uint) -> Windows<'a, Self::Item>;
    fn chunks<'a>(&'a self, size: uint) -> Chunks<'a, Self::Item>;
    fn get<'a>(&'a self, index: uint) -> Option<&'a Self::Item>;
    fn first<'a>(&'a self) -> Option<&'a Self::Item>;
    fn tail<'a>(&'a self) -> &'a [Self::Item];
    fn init<'a>(&'a self) -> &'a [Self::Item];
    fn last<'a>(&'a self) -> Option<&'a Self::Item>;
    unsafe fn get_unchecked<'a>(&'a self, index: uint) -> &'a Self::Item;
    fn as_ptr(&self) -> *const Self::Item;
    fn binary_search_by<F>(&self, f: F) -> Result<uint, uint> where
        F: FnMut(&Self::Item) -> Ordering;
    fn len(&self) -> uint;
    fn is_empty(&self) -> bool { self.len() == 0 }
    fn get_mut<'a>(&'a mut self, index: uint) -> Option<&'a mut Self::Item>;
    fn as_mut_slice<'a>(&'a mut self) -> &'a mut [Self::Item];
    fn slice_mut<'a>(&'a mut self, start: uint, end: uint) -> &'a mut [Self::Item];
    fn slice_from_mut<'a>(&'a mut self, start: uint) -> &'a mut [Self::Item];
    fn slice_to_mut<'a>(&'a mut self, end: uint) -> &'a mut [Self::Item];
    fn iter_mut<'a>(&'a mut self) -> IterMut<'a, Self::Item>;
    fn first_mut<'a>(&'a mut self) -> Option<&'a mut Self::Item>;
    fn tail_mut<'a>(&'a mut self) -> &'a mut [Self::Item];
    fn init_mut<'a>(&'a mut self) -> &'a mut [Self::Item];
    fn last_mut<'a>(&'a mut self) -> Option<&'a mut Self::Item>;
    fn split_mut<'a, P>(&'a mut self, pred: P) -> SplitMut<'a, Self::Item, P>
                        where P: FnMut(&Self::Item) -> bool;
    fn splitn_mut<P>(&mut self, n: uint, pred: P) -> SplitNMut<Self::Item, P>
                     where P: FnMut(&Self::Item) -> bool;
    fn rsplitn_mut<P>(&mut self,  n: uint, pred: P) -> RSplitNMut<Self::Item, P>
                      where P: FnMut(&Self::Item) -> bool;
    fn chunks_mut<'a>(&'a mut self, chunk_size: uint) -> ChunksMut<'a, Self::Item>;
    fn swap(&mut self, a: uint, b: uint);
    fn split_at_mut<'a>(&'a mut self, mid: uint) -> (&'a mut [Self::Item], &'a mut [Self::Item]);
    fn reverse(&mut self);
    unsafe fn get_unchecked_mut<'a>(&'a mut self, index: uint) -> &'a mut Self::Item;
    fn as_mut_ptr(&mut self) -> *mut Self::Item;

    fn position_elem(&self, t: &Self::Item) -> Option<uint> where Self::Item: PartialEq;

    fn rposition_elem(&self, t: &Self::Item) -> Option<uint> where Self::Item: PartialEq;

    fn contains(&self, x: &Self::Item) -> bool where Self::Item: PartialEq;

    fn starts_with(&self, needle: &[Self::Item]) -> bool where Self::Item: PartialEq;

    fn ends_with(&self, needle: &[Self::Item]) -> bool where Self::Item: PartialEq;

    fn binary_search(&self, x: &Self::Item) -> Result<uint, uint> where Self::Item: Ord;
    fn next_permutation(&mut self) -> bool where Self::Item: Ord;
    fn prev_permutation(&mut self) -> bool where Self::Item: Ord;

    fn clone_from_slice(&mut self, &[Self::Item]) -> uint where Self::Item: Clone;
}

#[unstable]
impl<T> SliceExt for [T] {
    type Item = T;

    #[inline]
    fn slice(&self, start: uint, end: uint) -> &[T] {
        assert!(start <= end);
        assert!(end <= self.len());
        unsafe {
            transmute(RawSlice {
                data: self.as_ptr().offset(start as int),
                len: (end - start)
            })
        }
    }

    #[inline]
    fn slice_from(&self, start: uint) -> &[T] {
        self.slice(start, self.len())
    }

    #[inline]
    fn slice_to(&self, end: uint) -> &[T] {
        self.slice(0, end)
    }

    #[inline]
    fn split_at(&self, mid: uint) -> (&[T], &[T]) {
        (self[..mid], self[mid..])
    }

    #[inline]
    fn iter<'a>(&'a self) -> Iter<'a, T> {
        unsafe {
            let p = self.as_ptr();
            if mem::size_of::<T>() == 0 {
                Iter {ptr: p,
                      end: (p as uint + self.len()) as *const T,
                      marker: marker::ContravariantLifetime::<'a>}
            } else {
                Iter {ptr: p,
                      end: p.offset(self.len() as int),
                      marker: marker::ContravariantLifetime::<'a>}
            }
        }
    }

    #[inline]
    fn split<'a, P>(&'a self, pred: P) -> Split<'a, T, P> where P: FnMut(&T) -> bool {
        Split {
            v: self,
            pred: pred,
            finished: false
        }
    }

    #[inline]
    fn splitn<'a, P>(&'a self, n: uint, pred: P) -> SplitN<'a, T, P> where
        P: FnMut(&T) -> bool,
    {
        SplitN {
            inner: GenericSplitN {
                iter: self.split(pred),
                count: n,
                invert: false
            }
        }
    }

    #[inline]
    fn rsplitn<'a, P>(&'a self, n: uint, pred: P) -> RSplitN<'a, T, P> where
        P: FnMut(&T) -> bool,
    {
        RSplitN {
            inner: GenericSplitN {
                iter: self.split(pred),
                count: n,
                invert: true
            }
        }
    }

    #[inline]
    fn windows(&self, size: uint) -> Windows<T> {
        assert!(size != 0);
        Windows { v: self, size: size }
    }

    #[inline]
    fn chunks(&self, size: uint) -> Chunks<T> {
        assert!(size != 0);
        Chunks { v: self, size: size }
    }

    #[inline]
    fn get(&self, index: uint) -> Option<&T> {
        if index < self.len() { Some(&self[index]) } else { None }
    }

    #[inline]
    fn first(&self) -> Option<&T> {
        if self.len() == 0 { None } else { Some(&self[0]) }
    }

    #[inline]
    fn tail(&self) -> &[T] { self[1..] }

    #[inline]
    fn init(&self) -> &[T] {
        self[..self.len() - 1]
    }

    #[inline]
    fn last(&self) -> Option<&T> {
        if self.len() == 0 { None } else { Some(&self[self.len() - 1]) }
    }

    #[inline]
    unsafe fn get_unchecked(&self, index: uint) -> &T {
        transmute(self.repr().data.offset(index as int))
    }

    #[inline]
    fn as_ptr(&self) -> *const T {
        self.repr().data
    }

    #[unstable]
    fn binary_search_by<F>(&self, mut f: F) -> Result<uint, uint> where
        F: FnMut(&T) -> Ordering
    {
        let mut base : uint = 0;
        let mut lim : uint = self.len();

        while lim != 0 {
            let ix = base + (lim >> 1);
            match f(&self[ix]) {
                Equal => return Ok(ix),
                Less => {
                    base = ix + 1;
                    lim -= 1;
                }
                Greater => ()
            }
            lim >>= 1;
        }
        Err(base)
    }

    #[inline]
    fn len(&self) -> uint { self.repr().len }

    #[inline]
    fn get_mut(&mut self, index: uint) -> Option<&mut T> {
        if index < self.len() { Some(&mut self[index]) } else { None }
    }

    #[inline]
    fn as_mut_slice(&mut self) -> &mut [T] { self }

    fn slice_mut(&mut self, start: uint, end: uint) -> &mut [T] {
        ops::SliceMut::slice_or_fail_mut(self, &start, &end)
    }

    #[inline]
    fn slice_from_mut(&mut self, start: uint) -> &mut [T] {
        ops::SliceMut::slice_from_or_fail_mut(self, &start)
    }

    #[inline]
    fn slice_to_mut(&mut self, end: uint) -> &mut [T] {
        ops::SliceMut::slice_to_or_fail_mut(self, &end)
    }

    #[inline]
    fn split_at_mut(&mut self, mid: uint) -> (&mut [T], &mut [T]) {
        unsafe {
            let self2: &mut [T] = mem::transmute_copy(&self);

            (ops::SliceMut::slice_to_or_fail_mut(self, &mid),
             ops::SliceMut::slice_from_or_fail_mut(self2, &mid))
        }
    }

    #[inline]
    fn iter_mut<'a>(&'a mut self) -> IterMut<'a, T> {
        unsafe {
            let p = self.as_mut_ptr();
            if mem::size_of::<T>() == 0 {
                IterMut {ptr: p,
                         end: (p as uint + self.len()) as *mut T,
                         marker: marker::ContravariantLifetime::<'a>}
            } else {
                IterMut {ptr: p,
                         end: p.offset(self.len() as int),
                         marker: marker::ContravariantLifetime::<'a>}
            }
        }
    }

    #[inline]
    fn last_mut(&mut self) -> Option<&mut T> {
        let len = self.len();
        if len == 0 { return None; }
        Some(&mut self[len - 1])
    }

    #[inline]
    fn first_mut(&mut self) -> Option<&mut T> {
        if self.len() == 0 { None } else { Some(&mut self[0]) }
    }

    #[inline]
    fn tail_mut(&mut self) -> &mut [T] {
        self.slice_from_mut(1)
    }

    #[inline]
    fn init_mut(&mut self) -> &mut [T] {
        let len = self.len();
        self.slice_to_mut(len-1)
    }

    #[inline]
    fn split_mut<'a, P>(&'a mut self, pred: P) -> SplitMut<'a, T, P> where P: FnMut(&T) -> bool {
        SplitMut { v: self, pred: pred, finished: false }
    }

    #[inline]
    fn splitn_mut<'a, P>(&'a mut self, n: uint, pred: P) -> SplitNMut<'a, T, P> where
        P: FnMut(&T) -> bool
    {
        SplitNMut {
            inner: GenericSplitN {
                iter: self.split_mut(pred),
                count: n,
                invert: false
            }
        }
    }

    #[inline]
    fn rsplitn_mut<'a, P>(&'a mut self, n: uint, pred: P) -> RSplitNMut<'a, T, P> where
        P: FnMut(&T) -> bool,
    {
        RSplitNMut {
            inner: GenericSplitN {
                iter: self.split_mut(pred),
                count: n,
                invert: true
            }
        }
   }

    #[inline]
    fn chunks_mut(&mut self, chunk_size: uint) -> ChunksMut<T> {
        assert!(chunk_size > 0);
        ChunksMut { v: self, chunk_size: chunk_size }
    }

    fn swap(&mut self, a: uint, b: uint) {
        unsafe {
            // Can't take two mutable loans from one vector, so instead just cast
            // them to their raw pointers to do the swap
            let pa: *mut T = &mut self[a];
            let pb: *mut T = &mut self[b];
            ptr::swap(pa, pb);
        }
    }

    fn reverse(&mut self) {
        let mut i: uint = 0;
        let ln = self.len();
        while i < ln / 2 {
            // Unsafe swap to avoid the bounds check in safe swap.
            unsafe {
                let pa: *mut T = self.get_unchecked_mut(i);
                let pb: *mut T = self.get_unchecked_mut(ln - i - 1);
                ptr::swap(pa, pb);
            }
            i += 1;
        }
    }

    #[inline]
    unsafe fn get_unchecked_mut(&mut self, index: uint) -> &mut T {
        transmute((self.repr().data as *mut T).offset(index as int))
    }

    #[inline]
    fn as_mut_ptr(&mut self) -> *mut T {
        self.repr().data as *mut T
    }

    #[inline]
    fn position_elem(&self, x: &T) -> Option<uint> where T: PartialEq {
        self.iter().position(|y| *x == *y)
    }

    #[inline]
    fn rposition_elem(&self, t: &T) -> Option<uint> where T: PartialEq {
        self.iter().rposition(|x| *x == *t)
    }

    #[inline]
    fn contains(&self, x: &T) -> bool where T: PartialEq {
        self.iter().any(|elt| *x == *elt)
    }

    #[inline]
    fn starts_with(&self, needle: &[T]) -> bool where T: PartialEq {
        let n = needle.len();
        self.len() >= n && needle == self[..n]
    }

    #[inline]
    fn ends_with(&self, needle: &[T]) -> bool where T: PartialEq {
        let (m, n) = (self.len(), needle.len());
        m >= n && needle == self[m-n..]
    }

    #[unstable]
    fn binary_search(&self, x: &T) -> Result<uint, uint> where T: Ord {
        self.binary_search_by(|p| p.cmp(x))
    }

    #[experimental]
    fn next_permutation(&mut self) -> bool where T: Ord {
        // These cases only have 1 permutation each, so we can't do anything.
        if self.len() < 2 { return false; }

        // Step 1: Identify the longest, rightmost weakly decreasing part of the vector
        let mut i = self.len() - 1;
        while i > 0 && self[i-1] >= self[i] {
            i -= 1;
        }

        // If that is the entire vector, this is the last-ordered permutation.
        if i == 0 {
            return false;
        }

        // Step 2: Find the rightmost element larger than the pivot (i-1)
        let mut j = self.len() - 1;
        while j >= i && self[j] <= self[i-1]  {
            j -= 1;
        }

        // Step 3: Swap that element with the pivot
        self.swap(j, i-1);

        // Step 4: Reverse the (previously) weakly decreasing part
        self.slice_from_mut(i).reverse();

        true
    }

    #[experimental]
    fn prev_permutation(&mut self) -> bool where T: Ord {
        // These cases only have 1 permutation each, so we can't do anything.
        if self.len() < 2 { return false; }

        // Step 1: Identify the longest, rightmost weakly increasing part of the vector
        let mut i = self.len() - 1;
        while i > 0 && self[i-1] <= self[i] {
            i -= 1;
        }

        // If that is the entire vector, this is the first-ordered permutation.
        if i == 0 {
            return false;
        }

        // Step 2: Reverse the weakly increasing part
        self.slice_from_mut(i).reverse();

        // Step 3: Find the rightmost element equal to or bigger than the pivot (i-1)
        let mut j = self.len() - 1;
        while j >= i && self[j-1] < self[i-1]  {
            j -= 1;
        }

        // Step 4: Swap that element with the pivot
        self.swap(i-1, j);

        true
    }

    #[inline]
    fn clone_from_slice(&mut self, src: &[T]) -> uint where T: Clone {
        let min = cmp::min(self.len(), src.len());
        let dst = self.slice_to_mut(min);
        let src = src.slice_to(min);
        for i in range(0, min) {
            dst[i].clone_from(&src[i]);
        }
        min
    }
}

// NOTE(stage0) remove impl after a snapshot
#[cfg(stage0)]
impl<T> ops::Index<uint, T> for [T] {
    fn index(&self, &index: &uint) -> &T {
        assert!(index < self.len());

        unsafe { mem::transmute(self.repr().data.offset(index as int)) }
    }
}

#[cfg(not(stage0))]  // NOTE(stage0) remove cfg after a snapshot
impl<T> ops::Index<uint> for [T] {
    type Output = T;

    fn index(&self, &index: &uint) -> &T {
        assert!(index < self.len());

        unsafe { mem::transmute(self.repr().data.offset(index as int)) }
    }
}

// NOTE(stage0) remove impl after a snapshot
#[cfg(stage0)]
impl<T> ops::IndexMut<uint, T> for [T] {
    fn index_mut(&mut self, &index: &uint) -> &mut T {
        assert!(index < self.len());

        unsafe { mem::transmute(self.repr().data.offset(index as int)) }
    }
}

#[cfg(not(stage0))]  // NOTE(stage0) remove cfg after a snapshot
impl<T> ops::IndexMut<uint> for [T] {
    type Output = T;

    fn index_mut(&mut self, &index: &uint) -> &mut T {
        assert!(index < self.len());

        unsafe { mem::transmute(self.repr().data.offset(index as int)) }
    }
}

impl<T> ops::Slice<uint, [T]> for [T] {
    #[inline]
    fn as_slice_<'a>(&'a self) -> &'a [T] {
        self
    }

    #[inline]
    fn slice_from_or_fail<'a>(&'a self, start: &uint) -> &'a [T] {
        self.slice_or_fail(start, &self.len())
    }

    #[inline]
    fn slice_to_or_fail<'a>(&'a self, end: &uint) -> &'a [T] {
        self.slice_or_fail(&0, end)
    }
    #[inline]
    fn slice_or_fail<'a>(&'a self, start: &uint, end: &uint) -> &'a [T] {
        assert!(*start <= *end);
        assert!(*end <= self.len());
        unsafe {
            transmute(RawSlice {
                    data: self.as_ptr().offset(*start as int),
                    len: (*end - *start)
                })
        }
    }
}

impl<T> ops::SliceMut<uint, [T]> for [T] {
    #[inline]
    fn as_mut_slice_<'a>(&'a mut self) -> &'a mut [T] {
        self
    }

    #[inline]
    fn slice_from_or_fail_mut<'a>(&'a mut self, start: &uint) -> &'a mut [T] {
        let len = &self.len();
        self.slice_or_fail_mut(start, len)
    }

    #[inline]
    fn slice_to_or_fail_mut<'a>(&'a mut self, end: &uint) -> &'a mut [T] {
        self.slice_or_fail_mut(&0, end)
    }
    #[inline]
    fn slice_or_fail_mut<'a>(&'a mut self, start: &uint, end: &uint) -> &'a mut [T] {
        assert!(*start <= *end);
        assert!(*end <= self.len());
        unsafe {
            transmute(RawSlice {
                    data: self.as_ptr().offset(*start as int),
                    len: (*end - *start)
                })
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// Common traits
////////////////////////////////////////////////////////////////////////////////

/// Data that is viewable as a slice.
#[experimental = "will be replaced by slice syntax"]
pub trait AsSlice<T> for Sized? {
    /// Work with `self` as a slice.
    fn as_slice<'a>(&'a self) -> &'a [T];
}

#[experimental = "trait is experimental"]
impl<T> AsSlice<T> for [T] {
    #[inline(always)]
    fn as_slice<'a>(&'a self) -> &'a [T] { self }
}

#[experimental = "trait is experimental"]
impl<'a, T, Sized? U: AsSlice<T>> AsSlice<T> for &'a U {
    #[inline(always)]
    fn as_slice(&self) -> &[T] { AsSlice::as_slice(*self) }
}

#[experimental = "trait is experimental"]
impl<'a, T, Sized? U: AsSlice<T>> AsSlice<T> for &'a mut U {
    #[inline(always)]
    fn as_slice(&self) -> &[T] { AsSlice::as_slice(*self) }
}

#[stable]
impl<'a, T> Default for &'a [T] {
    #[stable]
    fn default() -> &'a [T] { &[] }
}

//
// Iterators
//

// The shared definition of the `Iter` and `IterMut` iterators
macro_rules! iterator {
    (struct $name:ident -> $ptr:ty, $elem:ty) => {
        #[experimental = "needs review"]
        impl<'a, T> Iterator for $name<'a, T> {
            type Item = $elem;

            #[inline]
            fn next(&mut self) -> Option<$elem> {
                // could be implemented with slices, but this avoids bounds checks
                unsafe {
                    if self.ptr == self.end {
                        None
                    } else {
                        if mem::size_of::<T>() == 0 {
                            // purposefully don't use 'ptr.offset' because for
                            // vectors with 0-size elements this would return the
                            // same pointer.
                            self.ptr = transmute(self.ptr as uint + 1);

                            // Use a non-null pointer value
                            Some(transmute(1u))
                        } else {
                            let old = self.ptr;
                            self.ptr = self.ptr.offset(1);

                            Some(transmute(old))
                        }
                    }
                }
            }

            #[inline]
            fn size_hint(&self) -> (uint, Option<uint>) {
                let diff = (self.end as uint) - (self.ptr as uint);
                let size = mem::size_of::<T>();
                let exact = diff / (if size == 0 {1} else {size});
                (exact, Some(exact))
            }
        }

        #[experimental = "needs review"]
        impl<'a, T> DoubleEndedIterator for $name<'a, T> {
            #[inline]
            fn next_back(&mut self) -> Option<$elem> {
                // could be implemented with slices, but this avoids bounds checks
                unsafe {
                    if self.end == self.ptr {
                        None
                    } else {
                        if mem::size_of::<T>() == 0 {
                            // See above for why 'ptr.offset' isn't used
                            self.end = transmute(self.end as uint - 1);

                            // Use a non-null pointer value
                            Some(transmute(1u))
                        } else {
                            self.end = self.end.offset(-1);

                            Some(transmute(self.end))
                        }
                    }
                }
            }
        }
    }
}

macro_rules! make_slice {
    ($t: ty -> $result: ty: $start: expr, $end: expr) => {{
        let diff = $end as uint - $start as uint;
        let len = if mem::size_of::<T>() == 0 {
            diff
        } else {
            diff / mem::size_of::<$t>()
        };
        unsafe {
            transmute::<_, $result>(RawSlice { data: $start as *const T, len: len })
        }
    }}
}

/// Immutable slice iterator
#[stable]
pub struct Iter<'a, T: 'a> {
    ptr: *const T,
    end: *const T,
    marker: marker::ContravariantLifetime<'a>
}

#[experimental]
impl<'a, T> ops::Slice<uint, [T]> for Iter<'a, T> {
    fn as_slice_(&self) -> &[T] {
        self.as_slice()
    }
    fn slice_from_or_fail<'b>(&'b self, from: &uint) -> &'b [T] {
        use ops::Slice;
        self.as_slice().slice_from_or_fail(from)
    }
    fn slice_to_or_fail<'b>(&'b self, to: &uint) -> &'b [T] {
        use ops::Slice;
        self.as_slice().slice_to_or_fail(to)
    }
    fn slice_or_fail<'b>(&'b self, from: &uint, to: &uint) -> &'b [T] {
        use ops::Slice;
        self.as_slice().slice_or_fail(from, to)
    }
}

impl<'a, T> Iter<'a, T> {
    /// View the underlying data as a subslice of the original data.
    ///
    /// This has the same lifetime as the original slice, and so the
    /// iterator can continue to be used while this exists.
    #[experimental]
    pub fn as_slice(&self) -> &'a [T] {
        make_slice!(T -> &'a [T]: self.ptr, self.end)
    }
}

impl<'a,T> Copy for Iter<'a,T> {}

iterator!{struct Iter -> *const T, &'a T}

#[experimental = "needs review"]
impl<'a, T> ExactSizeIterator for Iter<'a, T> {}

#[stable]
impl<'a, T> Clone for Iter<'a, T> {
    fn clone(&self) -> Iter<'a, T> { *self }
}

#[experimental = "needs review"]
impl<'a, T> RandomAccessIterator for Iter<'a, T> {
    #[inline]
    fn indexable(&self) -> uint {
        let (exact, _) = self.size_hint();
        exact
    }

    #[inline]
    fn idx(&mut self, index: uint) -> Option<&'a T> {
        unsafe {
            if index < self.indexable() {
                if mem::size_of::<T>() == 0 {
                    // Use a non-null pointer value
                    Some(transmute(1u))
                } else {
                    Some(transmute(self.ptr.offset(index as int)))
                }
            } else {
                None
            }
        }
    }
}

/// Mutable slice iterator.
#[stable]
pub struct IterMut<'a, T: 'a> {
    ptr: *mut T,
    end: *mut T,
    marker: marker::ContravariantLifetime<'a>,
}

#[experimental]
impl<'a, T> ops::Slice<uint, [T]> for IterMut<'a, T> {
    fn as_slice_<'b>(&'b self) -> &'b [T] {
        make_slice!(T -> &'b [T]: self.ptr, self.end)
    }
    fn slice_from_or_fail<'b>(&'b self, from: &uint) -> &'b [T] {
        use ops::Slice;
        self.as_slice_().slice_from_or_fail(from)
    }
    fn slice_to_or_fail<'b>(&'b self, to: &uint) -> &'b [T] {
        use ops::Slice;
        self.as_slice_().slice_to_or_fail(to)
    }
    fn slice_or_fail<'b>(&'b self, from: &uint, to: &uint) -> &'b [T] {
        use ops::Slice;
        self.as_slice_().slice_or_fail(from, to)
    }
}

#[experimental]
impl<'a, T> ops::SliceMut<uint, [T]> for IterMut<'a, T> {
    fn as_mut_slice_<'b>(&'b mut self) -> &'b mut [T] {
        make_slice!(T -> &'b mut [T]: self.ptr, self.end)
    }
    fn slice_from_or_fail_mut<'b>(&'b mut self, from: &uint) -> &'b mut [T] {
        use ops::SliceMut;
        self.as_mut_slice_().slice_from_or_fail_mut(from)
    }
    fn slice_to_or_fail_mut<'b>(&'b mut self, to: &uint) -> &'b mut [T] {
        use ops::SliceMut;
        self.as_mut_slice_().slice_to_or_fail_mut(to)
    }
    fn slice_or_fail_mut<'b>(&'b mut self, from: &uint, to: &uint) -> &'b mut [T] {
        use ops::SliceMut;
        self.as_mut_slice_().slice_or_fail_mut(from, to)
    }
}

impl<'a, T> IterMut<'a, T> {
    /// View the underlying data as a subslice of the original data.
    ///
    /// To avoid creating `&mut` references that alias, this is forced
    /// to consume the iterator. Consider using the `Slice` and
    /// `SliceMut` implementations for obtaining slices with more
    /// restricted lifetimes that do not consume the iterator.
    #[experimental]
    pub fn into_slice(self) -> &'a mut [T] {
        make_slice!(T -> &'a mut [T]: self.ptr, self.end)
    }
}

iterator!{struct IterMut -> *mut T, &'a mut T}

#[experimental = "needs review"]
impl<'a, T> ExactSizeIterator for IterMut<'a, T> {}

/// An internal abstraction over the splitting iterators, so that
/// splitn, splitn_mut etc can be implemented once.
trait SplitIter: DoubleEndedIterator {
    /// Mark the underlying iterator as complete, extracting the remaining
    /// portion of the slice.
    fn finish(&mut self) -> Option< <Self as Iterator>::Item>;
}

/// An iterator over subslices separated by elements that match a predicate
/// function.
#[stable]
pub struct Split<'a, T:'a, P> where P: FnMut(&T) -> bool {
    v: &'a [T],
    pred: P,
    finished: bool
}

// FIXME(#19839) Remove in favor of `#[derive(Clone)]`
#[stable]
impl<'a, T, P> Clone for Split<'a, T, P> where P: Clone + FnMut(&T) -> bool {
    fn clone(&self) -> Split<'a, T, P> {
        Split {
            v: self.v,
            pred: self.pred.clone(),
            finished: self.finished,
        }
    }
}

#[experimental = "needs review"]
impl<'a, T, P> Iterator for Split<'a, T, P> where P: FnMut(&T) -> bool {
    type Item = &'a [T];

    #[inline]
    fn next(&mut self) -> Option<&'a [T]> {
        if self.finished { return None; }

        match self.v.iter().position(|x| (self.pred)(x)) {
            None => self.finish(),
            Some(idx) => {
                let ret = Some(self.v[..idx]);
                self.v = self.v[idx + 1..];
                ret
            }
        }
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        if self.finished {
            (0, Some(0))
        } else {
            (1, Some(self.v.len() + 1))
        }
    }
}

#[experimental = "needs review"]
impl<'a, T, P> DoubleEndedIterator for Split<'a, T, P> where P: FnMut(&T) -> bool {
    #[inline]
    fn next_back(&mut self) -> Option<&'a [T]> {
        if self.finished { return None; }

        match self.v.iter().rposition(|x| (self.pred)(x)) {
            None => self.finish(),
            Some(idx) => {
                let ret = Some(self.v[idx + 1..]);
                self.v = self.v[..idx];
                ret
            }
        }
    }
}

impl<'a, T, P> SplitIter for Split<'a, T, P> where P: FnMut(&T) -> bool {
    #[inline]
    fn finish(&mut self) -> Option<&'a [T]> {
        if self.finished { None } else { self.finished = true; Some(self.v) }
    }
}

/// An iterator over the subslices of the vector which are separated
/// by elements that match `pred`.
#[stable]
pub struct SplitMut<'a, T:'a, P> where P: FnMut(&T) -> bool {
    v: &'a mut [T],
    pred: P,
    finished: bool
}

impl<'a, T, P> SplitIter for SplitMut<'a, T, P> where P: FnMut(&T) -> bool {
    #[inline]
    fn finish(&mut self) -> Option<&'a mut [T]> {
        if self.finished {
            None
        } else {
            self.finished = true;
            Some(mem::replace(&mut self.v, &mut []))
        }
    }
}

#[experimental = "needs review"]
impl<'a, T, P> Iterator for SplitMut<'a, T, P> where P: FnMut(&T) -> bool {
    type Item = &'a mut [T];

    #[inline]
    fn next(&mut self) -> Option<&'a mut [T]> {
        if self.finished { return None; }

        let idx_opt = { // work around borrowck limitations
            let pred = &mut self.pred;
            self.v.iter().position(|x| (*pred)(x))
        };
        match idx_opt {
            None => self.finish(),
            Some(idx) => {
                let tmp = mem::replace(&mut self.v, &mut []);
                let (head, tail) = tmp.split_at_mut(idx);
                self.v = tail.slice_from_mut(1);
                Some(head)
            }
        }
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        if self.finished {
            (0, Some(0))
        } else {
            // if the predicate doesn't match anything, we yield one slice
            // if it matches every element, we yield len+1 empty slices.
            (1, Some(self.v.len() + 1))
        }
    }
}

#[experimental = "needs review"]
impl<'a, T, P> DoubleEndedIterator for SplitMut<'a, T, P> where
    P: FnMut(&T) -> bool,
{
    #[inline]
    fn next_back(&mut self) -> Option<&'a mut [T]> {
        if self.finished { return None; }

        let idx_opt = { // work around borrowck limitations
            let pred = &mut self.pred;
            self.v.iter().rposition(|x| (*pred)(x))
        };
        match idx_opt {
            None => self.finish(),
            Some(idx) => {
                let tmp = mem::replace(&mut self.v, &mut []);
                let (head, tail) = tmp.split_at_mut(idx);
                self.v = head;
                Some(tail.slice_from_mut(1))
            }
        }
    }
}

/// An private iterator over subslices separated by elements that
/// match a predicate function, splitting at most a fixed number of
/// times.
struct GenericSplitN<I> {
    iter: I,
    count: uint,
    invert: bool
}

#[experimental = "needs review"]
impl<T, I: SplitIter + Iterator<Item=T>> Iterator for GenericSplitN<I> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<T> {
        if self.count == 0 {
            self.iter.finish()
        } else {
            self.count -= 1;
            if self.invert { self.iter.next_back() } else { self.iter.next() }
        }
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        let (lower, upper_opt) = self.iter.size_hint();
        (lower, upper_opt.map(|upper| cmp::min(self.count + 1, upper)))
    }
}

/// An iterator over subslices separated by elements that match a predicate
/// function, limited to a given number of splits.
pub struct SplitN<'a, T: 'a, P> where P: FnMut(&T) -> bool {
    inner: GenericSplitN<Split<'a, T, P>>
}

/// An iterator over subslices separated by elements that match a
/// predicate function, limited to a given number of splits, starting
/// from the end of the slice.
pub struct RSplitN<'a, T: 'a, P> where P: FnMut(&T) -> bool {
    inner: GenericSplitN<Split<'a, T, P>>
}

/// An iterator over subslices separated by elements that match a predicate
/// function, limited to a given number of splits.
pub struct SplitNMut<'a, T: 'a, P> where P: FnMut(&T) -> bool {
    inner: GenericSplitN<SplitMut<'a, T, P>>
}

/// An iterator over subslices separated by elements that match a
/// predicate function, limited to a given number of splits, starting
/// from the end of the slice.
pub struct RSplitNMut<'a, T: 'a, P> where P: FnMut(&T) -> bool {
    inner: GenericSplitN<SplitMut<'a, T, P>>
}

macro_rules! forward_iterator {
    ($name:ident: $elem:ident, $iter_of:ty) => {
        impl<'a, $elem, P> Iterator for $name<'a, $elem, P> where
            P: FnMut(&T) -> bool
        {
            type Item = $iter_of;

            #[inline]
            fn next(&mut self) -> Option<$iter_of> {
                self.inner.next()
            }

            #[inline]
            fn size_hint(&self) -> (uint, Option<uint>) {
                self.inner.size_hint()
            }
        }
    }
}

forward_iterator! { SplitN: T, &'a [T] }
forward_iterator! { RSplitN: T, &'a [T] }
forward_iterator! { SplitNMut: T, &'a mut [T] }
forward_iterator! { RSplitNMut: T, &'a mut [T] }

/// An iterator over overlapping subslices of length `size`.
#[derive(Clone)]
#[experimental = "needs review"]
pub struct Windows<'a, T:'a> {
    v: &'a [T],
    size: uint
}

impl<'a, T> Iterator for Windows<'a, T> {
    type Item = &'a [T];

    #[inline]
    fn next(&mut self) -> Option<&'a [T]> {
        if self.size > self.v.len() {
            None
        } else {
            let ret = Some(self.v[..self.size]);
            self.v = self.v[1..];
            ret
        }
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        if self.size > self.v.len() {
            (0, Some(0))
        } else {
            let x = self.v.len() - self.size;
            (x.saturating_add(1), x.checked_add(1u))
        }
    }
}

/// An iterator over a slice in (non-overlapping) chunks (`size` elements at a
/// time).
///
/// When the slice len is not evenly divided by the chunk size, the last slice
/// of the iteration will be the remainder.
#[derive(Clone)]
#[experimental = "needs review"]
pub struct Chunks<'a, T:'a> {
    v: &'a [T],
    size: uint
}

#[experimental = "needs review"]
impl<'a, T> Iterator for Chunks<'a, T> {
    type Item = &'a [T];

    #[inline]
    fn next(&mut self) -> Option<&'a [T]> {
        if self.v.len() == 0 {
            None
        } else {
            let chunksz = cmp::min(self.v.len(), self.size);
            let (fst, snd) = self.v.split_at(chunksz);
            self.v = snd;
            Some(fst)
        }
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        if self.v.len() == 0 {
            (0, Some(0))
        } else {
            let n = self.v.len() / self.size;
            let rem = self.v.len() % self.size;
            let n = if rem > 0 { n+1 } else { n };
            (n, Some(n))
        }
    }
}

#[experimental = "needs review"]
impl<'a, T> DoubleEndedIterator for Chunks<'a, T> {
    #[inline]
    fn next_back(&mut self) -> Option<&'a [T]> {
        if self.v.len() == 0 {
            None
        } else {
            let remainder = self.v.len() % self.size;
            let chunksz = if remainder != 0 { remainder } else { self.size };
            let (fst, snd) = self.v.split_at(self.v.len() - chunksz);
            self.v = fst;
            Some(snd)
        }
    }
}

#[experimental = "needs review"]
impl<'a, T> RandomAccessIterator for Chunks<'a, T> {
    #[inline]
    fn indexable(&self) -> uint {
        self.v.len()/self.size + if self.v.len() % self.size != 0 { 1 } else { 0 }
    }

    #[inline]
    fn idx(&mut self, index: uint) -> Option<&'a [T]> {
        if index < self.indexable() {
            let lo = index * self.size;
            let mut hi = lo + self.size;
            if hi < lo || hi > self.v.len() { hi = self.v.len(); }

            Some(self.v[lo..hi])
        } else {
            None
        }
    }
}

/// An iterator over a slice in (non-overlapping) mutable chunks (`size`
/// elements at a time). When the slice len is not evenly divided by the chunk
/// size, the last slice of the iteration will be the remainder.
#[experimental = "needs review"]
pub struct ChunksMut<'a, T:'a> {
    v: &'a mut [T],
    chunk_size: uint
}

#[experimental = "needs review"]
impl<'a, T> Iterator for ChunksMut<'a, T> {
    type Item = &'a mut [T];

    #[inline]
    fn next(&mut self) -> Option<&'a mut [T]> {
        if self.v.len() == 0 {
            None
        } else {
            let sz = cmp::min(self.v.len(), self.chunk_size);
            let tmp = mem::replace(&mut self.v, &mut []);
            let (head, tail) = tmp.split_at_mut(sz);
            self.v = tail;
            Some(head)
        }
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        if self.v.len() == 0 {
            (0, Some(0))
        } else {
            let n = self.v.len() / self.chunk_size;
            let rem = self.v.len() % self.chunk_size;
            let n = if rem > 0 { n + 1 } else { n };
            (n, Some(n))
        }
    }
}

#[experimental = "needs review"]
impl<'a, T> DoubleEndedIterator for ChunksMut<'a, T> {
    #[inline]
    fn next_back(&mut self) -> Option<&'a mut [T]> {
        if self.v.len() == 0 {
            None
        } else {
            let remainder = self.v.len() % self.chunk_size;
            let sz = if remainder != 0 { remainder } else { self.chunk_size };
            let tmp = mem::replace(&mut self.v, &mut []);
            let tmp_len = tmp.len();
            let (head, tail) = tmp.split_at_mut(tmp_len - sz);
            self.v = head;
            Some(tail)
        }
    }
}


//
// Free functions
//

/// Converts a pointer to A into a slice of length 1 (without copying).
#[unstable]
pub fn ref_slice<'a, A>(s: &'a A) -> &'a [A] {
    unsafe {
        transmute(RawSlice { data: s, len: 1 })
    }
}

/// Converts a pointer to A into a slice of length 1 (without copying).
#[unstable]
pub fn mut_ref_slice<'a, A>(s: &'a mut A) -> &'a mut [A] {
    unsafe {
        let ptr: *const A = transmute(s);
        transmute(RawSlice { data: ptr, len: 1 })
    }
}

/// Forms a slice from a pointer and a length.
///
/// The pointer given is actually a reference to the base of the slice. This
/// reference is used to give a concrete lifetime to tie the returned slice to.
/// Typically this should indicate that the slice is valid for as long as the
/// pointer itself is valid.
///
/// The `len` argument is the number of **elements**, not the number of bytes.
///
/// This function is unsafe as there is no guarantee that the given pointer is
/// valid for `len` elements, nor whether the lifetime provided is a suitable
/// lifetime for the returned slice.
///
/// # Example
///
/// ```rust
/// use std::slice;
///
/// // manifest a slice out of thin air!
/// let ptr = 0x1234 as *const uint;
/// let amt = 10;
/// unsafe {
///     let slice = slice::from_raw_buf(&ptr, amt);
/// }
/// ```
#[inline]
#[unstable = "should be renamed to from_raw_parts"]
pub unsafe fn from_raw_buf<'a, T>(p: &'a *const T, len: uint) -> &'a [T] {
    transmute(RawSlice { data: *p, len: len })
}

/// Performs the same functionality as `from_raw_buf`, except that a mutable
/// slice is returned.
///
/// This function is unsafe for the same reasons as `from_raw_buf`, as well as
/// not being able to provide a non-aliasing guarantee of the returned mutable
/// slice.
#[inline]
#[unstable = "jshould be renamed to from_raw_parts_mut"]
pub unsafe fn from_raw_mut_buf<'a, T>(p: &'a *mut T, len: uint) -> &'a mut [T] {
    transmute(RawSlice { data: *p as *const T, len: len })
}

//
// Submodules
//

/// Operations on `[u8]`.
#[experimental = "needs review"]
pub mod bytes {
    use kinds::Sized;
    use ptr;
    use slice::SliceExt;

    /// A trait for operations on mutable `[u8]`s.
    pub trait MutableByteVector for Sized? {
        /// Sets all bytes of the receiver to the given value.
        fn set_memory(&mut self, value: u8);
    }

    impl MutableByteVector for [u8] {
        #[inline]
        #[allow(experimental)]
        fn set_memory(&mut self, value: u8) {
            unsafe { ptr::set_memory(self.as_mut_ptr(), value, self.len()) };
        }
    }

    /// Copies data from `src` to `dst`
    ///
    /// Panics if the length of `dst` is less than the length of `src`.
    #[inline]
    pub fn copy_memory(dst: &mut [u8], src: &[u8]) {
        let len_src = src.len();
        assert!(dst.len() >= len_src);
        // `dst` is unaliasable, so we know statically it doesn't overlap
        // with `src`.
        unsafe {
            ptr::copy_nonoverlapping_memory(dst.as_mut_ptr(),
                                            src.as_ptr(),
                                            len_src);
        }
    }
}



//
// Boilerplate traits
//

#[stable]
impl<A, B> PartialEq<[B]> for [A] where A: PartialEq<B> {
    fn eq(&self, other: &[B]) -> bool {
        self.len() == other.len() &&
            order::eq(self.iter(), other.iter())
    }
    fn ne(&self, other: &[B]) -> bool {
        self.len() != other.len() ||
            order::ne(self.iter(), other.iter())
    }
}

#[stable]
impl<T: Eq> Eq for [T] {}

#[stable]
impl<T: Ord> Ord for [T] {
    fn cmp(&self, other: &[T]) -> Ordering {
        order::cmp(self.iter(), other.iter())
    }
}

#[stable]
impl<T: PartialOrd> PartialOrd for [T] {
    #[inline]
    fn partial_cmp(&self, other: &[T]) -> Option<Ordering> {
        order::partial_cmp(self.iter(), other.iter())
    }
    #[inline]
    fn lt(&self, other: &[T]) -> bool {
        order::lt(self.iter(), other.iter())
    }
    #[inline]
    fn le(&self, other: &[T]) -> bool {
        order::le(self.iter(), other.iter())
    }
    #[inline]
    fn ge(&self, other: &[T]) -> bool {
        order::ge(self.iter(), other.iter())
    }
    #[inline]
    fn gt(&self, other: &[T]) -> bool {
        order::gt(self.iter(), other.iter())
    }
}

/// Extension methods for slices containing integers.
#[experimental]
pub trait IntSliceExt<U, S> for Sized? {
    /// Converts the slice to an immutable slice of unsigned integers with the same width.
    fn as_unsigned<'a>(&'a self) -> &'a [U];
    /// Converts the slice to an immutable slice of signed integers with the same width.
    fn as_signed<'a>(&'a self) -> &'a [S];

    /// Converts the slice to a mutable slice of unsigned integers with the same width.
    fn as_unsigned_mut<'a>(&'a mut self) -> &'a mut [U];
    /// Converts the slice to a mutable slice of signed integers with the same width.
    fn as_signed_mut<'a>(&'a mut self) -> &'a mut [S];
}

macro_rules! impl_int_slice {
    ($u:ty, $s:ty, $t:ty) => {
        #[experimental]
        impl IntSliceExt<$u, $s> for [$t] {
            #[inline]
            fn as_unsigned(&self) -> &[$u] { unsafe { transmute(self) } }
            #[inline]
            fn as_signed(&self) -> &[$s] { unsafe { transmute(self) } }
            #[inline]
            fn as_unsigned_mut(&mut self) -> &mut [$u] { unsafe { transmute(self) } }
            #[inline]
            fn as_signed_mut(&mut self) -> &mut [$s] { unsafe { transmute(self) } }
        }
    }
}

macro_rules! impl_int_slices {
    ($u:ty, $s:ty) => {
        impl_int_slice! { $u, $s, $u }
        impl_int_slice! { $u, $s, $s }
    }
}

impl_int_slices! { u8,   i8  }
impl_int_slices! { u16,  i16 }
impl_int_slices! { u32,  i32 }
impl_int_slices! { u64,  i64 }
impl_int_slices! { uint, int }
