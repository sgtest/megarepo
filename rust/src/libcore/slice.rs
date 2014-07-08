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
use collections::Collection;
use cmp::{PartialEq, PartialOrd, Eq, Ord, Ordering, Less, Equal, Greater, Equiv};
use cmp;
use default::Default;
use iter::*;
use num::{CheckedAdd, Saturating, div_rem};
use option::{None, Option, Some};
use ptr;
use ptr::RawPtr;
use mem;
use mem::size_of;
use kinds::marker;
use raw::{Repr, Slice};

//
// Extension traits
//

/// Extension methods for vectors
pub trait ImmutableVector<'a, T> {
    /**
     * Returns a slice of self spanning the interval [`start`, `end`).
     *
     * Fails when the slice (or part of it) is outside the bounds of self,
     * or when `start` > `end`.
     */
    fn slice(&self, start: uint, end: uint) -> &'a [T];

    /**
     * Returns a slice of self from `start` to the end of the vec.
     *
     * Fails when `start` points outside the bounds of self.
     */
    fn slice_from(&self, start: uint) -> &'a [T];

    /**
     * Returns a slice of self from the start of the vec to `end`.
     *
     * Fails when `end` points outside the bounds of self.
     */
    fn slice_to(&self, end: uint) -> &'a [T];
    /// Returns an iterator over the vector
    fn iter(self) -> Items<'a, T>;
    /// Returns an iterator over the subslices of the vector which are
    /// separated by elements that match `pred`.  The matched element
    /// is not contained in the subslices.
    fn split(self, pred: |&T|: 'a -> bool) -> Splits<'a, T>;
    /// Returns an iterator over the subslices of the vector which are
    /// separated by elements that match `pred`, limited to splitting
    /// at most `n` times.  The matched element is not contained in
    /// the subslices.
    fn splitn(self, n: uint, pred: |&T|: 'a -> bool) -> SplitsN<'a, T>;
    /// Returns an iterator over the subslices of the vector which are
    /// separated by elements that match `pred` limited to splitting
    /// at most `n` times. This starts at the end of the vector and
    /// works backwards.  The matched element is not contained in the
    /// subslices.
    fn rsplitn(self,  n: uint, pred: |&T|: 'a -> bool) -> SplitsN<'a, T>;

    /**
     * Returns an iterator over all contiguous windows of length
     * `size`. The windows overlap. If the vector is shorter than
     * `size`, the iterator returns no values.
     *
     * # Failure
     *
     * Fails if `size` is 0.
     *
     * # Example
     *
     * Print the adjacent pairs of a vector (i.e. `[1,2]`, `[2,3]`,
     * `[3,4]`):
     *
     * ```rust
     * let v = &[1i, 2, 3, 4];
     * for win in v.windows(2) {
     *     println!("{}", win);
     * }
     * ```
     *
     */
    fn windows(self, size: uint) -> Windows<'a, T>;
    /**
     *
     * Returns an iterator over `size` elements of the vector at a
     * time. The chunks do not overlap. If `size` does not divide the
     * length of the vector, then the last chunk will not have length
     * `size`.
     *
     * # Failure
     *
     * Fails if `size` is 0.
     *
     * # Example
     *
     * Print the vector two elements at a time (i.e. `[1,2]`,
     * `[3,4]`, `[5]`):
     *
     * ```rust
     * let v = &[1i, 2, 3, 4, 5];
     * for win in v.chunks(2) {
     *     println!("{}", win);
     * }
     * ```
     *
     */
    fn chunks(self, size: uint) -> Chunks<'a, T>;

    /// Returns the element of a vector at the given index, or `None` if the
    /// index is out of bounds
    fn get(&self, index: uint) -> Option<&'a T>;
    /// Returns the first element of a vector, or `None` if it is empty
    fn head(&self) -> Option<&'a T>;
    /// Returns all but the first element of a vector
    fn tail(&self) -> &'a [T];
    /// Returns all but the first `n' elements of a vector
    fn tailn(&self, n: uint) -> &'a [T];
    /// Returns all but the last element of a vector
    fn init(&self) -> &'a [T];
    /// Returns all but the last `n' elements of a vector
    fn initn(&self, n: uint) -> &'a [T];
    /// Returns the last element of a vector, or `None` if it is empty.
    fn last(&self) -> Option<&'a T>;

    /// Returns a pointer to the element at the given index, without doing
    /// bounds checking.
    unsafe fn unsafe_ref(self, index: uint) -> &'a T;

    /**
     * Returns an unsafe pointer to the vector's buffer
     *
     * The caller must ensure that the vector outlives the pointer this
     * function returns, or else it will end up pointing to garbage.
     *
     * Modifying the vector may cause its buffer to be reallocated, which
     * would also make any pointers to it invalid.
     */
    fn as_ptr(&self) -> *const T;

    /**
     * Binary search a sorted vector with a comparator function.
     *
     * The comparator function should implement an order consistent
     * with the sort order of the underlying vector, returning an
     * order code that indicates whether its argument is `Less`,
     * `Equal` or `Greater` the desired target.
     *
     * Returns the index where the comparator returned `Equal`, or `None` if
     * not found.
     */
    fn bsearch(&self, f: |&T| -> Ordering) -> Option<uint>;

    /**
     * Returns an immutable reference to the first element in this slice
     * and adjusts the slice in place so that it no longer contains
     * that element. O(1).
     *
     * Equivalent to:
     *
     * ```ignore
     *     if self.len() == 0 { return None }
     *     let head = &self[0];
     *     *self = self.slice_from(1);
     *     Some(head)
     * ```
     *
     * Returns `None` if vector is empty
     */
    fn shift_ref(&mut self) -> Option<&'a T>;

    /**
     * Returns an immutable reference to the last element in this slice
     * and adjusts the slice in place so that it no longer contains
     * that element. O(1).
     *
     * Equivalent to:
     *
     * ```ignore
     *     if self.len() == 0 { return None; }
     *     let tail = &self[self.len() - 1];
     *     *self = self.slice_to(self.len() - 1);
     *     Some(tail)
     * ```
     *
     * Returns `None` if slice is empty.
     */
    fn pop_ref(&mut self) -> Option<&'a T>;
}

impl<'a,T> ImmutableVector<'a, T> for &'a [T] {
    #[inline]
    fn slice(&self, start: uint, end: uint) -> &'a [T] {
        assert!(start <= end);
        assert!(end <= self.len());
        unsafe {
            transmute(Slice {
                    data: self.as_ptr().offset(start as int),
                    len: (end - start)
                })
        }
    }

    #[inline]
    fn slice_from(&self, start: uint) -> &'a [T] {
        self.slice(start, self.len())
    }

    #[inline]
    fn slice_to(&self, end: uint) -> &'a [T] {
        self.slice(0, end)
    }

    #[inline]
    fn iter(self) -> Items<'a, T> {
        unsafe {
            let p = self.as_ptr();
            if mem::size_of::<T>() == 0 {
                Items{ptr: p,
                      end: (p as uint + self.len()) as *const T,
                      marker: marker::ContravariantLifetime::<'a>}
            } else {
                Items{ptr: p,
                      end: p.offset(self.len() as int),
                      marker: marker::ContravariantLifetime::<'a>}
            }
        }
    }

    #[inline]
    fn split(self, pred: |&T|: 'a -> bool) -> Splits<'a, T> {
        Splits {
            v: self,
            pred: pred,
            finished: false
        }
    }

    #[inline]
    fn splitn(self, n: uint, pred: |&T|: 'a -> bool) -> SplitsN<'a, T> {
        SplitsN {
            iter: self.split(pred),
            count: n,
            invert: false
        }
    }

    #[inline]
    fn rsplitn(self, n: uint, pred: |&T|: 'a -> bool) -> SplitsN<'a, T> {
        SplitsN {
            iter: self.split(pred),
            count: n,
            invert: true
        }
    }

    #[inline]
    fn windows(self, size: uint) -> Windows<'a, T> {
        assert!(size != 0);
        Windows { v: self, size: size }
    }

    #[inline]
    fn chunks(self, size: uint) -> Chunks<'a, T> {
        assert!(size != 0);
        Chunks { v: self, size: size }
    }

    #[inline]
    fn get(&self, index: uint) -> Option<&'a T> {
        if index < self.len() { Some(&self[index]) } else { None }
    }

    #[inline]
    fn head(&self) -> Option<&'a T> {
        if self.len() == 0 { None } else { Some(&self[0]) }
    }

    #[inline]
    fn tail(&self) -> &'a [T] { self.slice(1, self.len()) }

    #[inline]
    fn tailn(&self, n: uint) -> &'a [T] { self.slice(n, self.len()) }

    #[inline]
    fn init(&self) -> &'a [T] {
        self.slice(0, self.len() - 1)
    }

    #[inline]
    fn initn(&self, n: uint) -> &'a [T] {
        self.slice(0, self.len() - n)
    }

    #[inline]
    fn last(&self) -> Option<&'a T> {
            if self.len() == 0 { None } else { Some(&self[self.len() - 1]) }
    }

    #[inline]
    unsafe fn unsafe_ref(self, index: uint) -> &'a T {
        transmute(self.repr().data.offset(index as int))
    }

    #[inline]
    fn as_ptr(&self) -> *const T {
        self.repr().data
    }


    fn bsearch(&self, f: |&T| -> Ordering) -> Option<uint> {
        let mut base : uint = 0;
        let mut lim : uint = self.len();

        while lim != 0 {
            let ix = base + (lim >> 1);
            match f(&self[ix]) {
                Equal => return Some(ix),
                Less => {
                    base = ix + 1;
                    lim -= 1;
                }
                Greater => ()
            }
            lim >>= 1;
        }
        return None;
    }

    fn shift_ref(&mut self) -> Option<&'a T> {
        unsafe {
            let s: &mut Slice<T> = transmute(self);
            match raw::shift_ptr(s) {
                Some(p) => Some(&*p),
                None => None
            }
        }
    }

    fn pop_ref(&mut self) -> Option<&'a T> {
        unsafe {
            let s: &mut Slice<T> = transmute(self);
            match raw::pop_ptr(s) {
                Some(p) => Some(&*p),
                None => None
            }
        }
    }
}

/// Extension methods for vectors such that their elements are
/// mutable.
pub trait MutableVector<'a, T> {
    /// Returns a mutable reference to the element at the given index,
    /// or `None` if the index is out of bounds
    fn get_mut(self, index: uint) -> Option<&'a mut T>;
    /// Work with `self` as a mut slice.
    /// Primarily intended for getting a &mut [T] from a [T, ..N].
    fn as_mut_slice(self) -> &'a mut [T];

    /// Return a slice that points into another slice.
    fn mut_slice(self, start: uint, end: uint) -> &'a mut [T];

    /**
     * Returns a slice of self from `start` to the end of the vec.
     *
     * Fails when `start` points outside the bounds of self.
     */
    fn mut_slice_from(self, start: uint) -> &'a mut [T];

    /**
     * Returns a slice of self from the start of the vec to `end`.
     *
     * Fails when `end` points outside the bounds of self.
     */
    fn mut_slice_to(self, end: uint) -> &'a mut [T];

    /// Returns an iterator that allows modifying each value
    fn mut_iter(self) -> MutItems<'a, T>;

    /// Returns a mutable pointer to the last item in the vector.
    fn mut_last(self) -> Option<&'a mut T>;

    /// Returns an iterator over the mutable subslices of the vector
    /// which are separated by elements that match `pred`.  The
    /// matched element is not contained in the subslices.
    fn mut_split(self, pred: |&T|: 'a -> bool) -> MutSplits<'a, T>;

    /**
     * Returns an iterator over `size` elements of the vector at a time.
     * The chunks are mutable and do not overlap. If `size` does not divide the
     * length of the vector, then the last chunk will not have length
     * `size`.
     *
     * # Failure
     *
     * Fails if `size` is 0.
     */
    fn mut_chunks(self, chunk_size: uint) -> MutChunks<'a, T>;

    /**
     * Returns a mutable reference to the first element in this slice
     * and adjusts the slice in place so that it no longer contains
     * that element. O(1).
     *
     * Equivalent to:
     *
     * ```ignore
     *     if self.len() == 0 { return None; }
     *     let head = &mut self[0];
     *     *self = self.mut_slice_from(1);
     *     Some(head)
     * ```
     *
     * Returns `None` if slice is empty
     */
    fn mut_shift_ref(&mut self) -> Option<&'a mut T>;

    /**
     * Returns a mutable reference to the last element in this slice
     * and adjusts the slice in place so that it no longer contains
     * that element. O(1).
     *
     * Equivalent to:
     *
     * ```ignore
     *     if self.len() == 0 { return None; }
     *     let tail = &mut self[self.len() - 1];
     *     *self = self.mut_slice_to(self.len() - 1);
     *     Some(tail)
     * ```
     *
     * Returns `None` if slice is empty.
     */
    fn mut_pop_ref(&mut self) -> Option<&'a mut T>;

    /// Swaps two elements in a vector.
    ///
    /// Fails if `a` or `b` are out of bounds.
    ///
    /// # Arguments
    ///
    /// * a - The index of the first element
    /// * b - The index of the second element
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut v = ["a", "b", "c", "d"];
    /// v.swap(1, 3);
    /// assert!(v == ["a", "d", "c", "b"]);
    /// ```
    fn swap(self, a: uint, b: uint);


    /// Divides one `&mut` into two at an index.
    ///
    /// The first will contain all indices from `[0, mid)` (excluding
    /// the index `mid` itself) and the second will contain all
    /// indices from `[mid, len)` (excluding the index `len` itself).
    ///
    /// Fails if `mid > len`.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut v = [1i, 2, 3, 4, 5, 6];
    ///
    /// // scoped to restrict the lifetime of the borrows
    /// {
    ///    let (left, right) = v.mut_split_at(0);
    ///    assert!(left == &mut []);
    ///    assert!(right == &mut [1i, 2, 3, 4, 5, 6]);
    /// }
    ///
    /// {
    ///     let (left, right) = v.mut_split_at(2);
    ///     assert!(left == &mut [1i, 2]);
    ///     assert!(right == &mut [3i, 4, 5, 6]);
    /// }
    ///
    /// {
    ///     let (left, right) = v.mut_split_at(6);
    ///     assert!(left == &mut [1i, 2, 3, 4, 5, 6]);
    ///     assert!(right == &mut []);
    /// }
    /// ```
    fn mut_split_at(self, mid: uint) -> (&'a mut [T], &'a mut [T]);

    /// Reverse the order of elements in a vector, in place.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut v = [1i, 2, 3];
    /// v.reverse();
    /// assert!(v == [3i, 2, 1]);
    /// ```
    fn reverse(self);

    /// Returns an unsafe mutable pointer to the element in index
    unsafe fn unsafe_mut_ref(self, index: uint) -> &'a mut T;

    /// Return an unsafe mutable pointer to the vector's buffer.
    ///
    /// The caller must ensure that the vector outlives the pointer this
    /// function returns, or else it will end up pointing to garbage.
    ///
    /// Modifying the vector may cause its buffer to be reallocated, which
    /// would also make any pointers to it invalid.
    #[inline]
    fn as_mut_ptr(self) -> *mut T;

    /// Unsafely sets the element in index to the value.
    ///
    /// This performs no bounds checks, and it is undefined behaviour
    /// if `index` is larger than the length of `self`. However, it
    /// does run the destructor at `index`. It is equivalent to
    /// `self[index] = val`.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut v = ["foo".to_string(), "bar".to_string(), "baz".to_string()];
    ///
    /// unsafe {
    ///     // `"baz".to_string()` is deallocated.
    ///     v.unsafe_set(2, "qux".to_string());
    ///
    ///     // Out of bounds: could cause a crash, or overwriting
    ///     // other data, or something else.
    ///     // v.unsafe_set(10, "oops".to_string());
    /// }
    /// ```
    unsafe fn unsafe_set(self, index: uint, val: T);

    /// Unchecked vector index assignment.  Does not drop the
    /// old value and hence is only suitable when the vector
    /// is newly allocated.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut v = ["foo".to_string(), "bar".to_string()];
    ///
    /// // memory leak! `"bar".to_string()` is not deallocated.
    /// unsafe { v.init_elem(1, "baz".to_string()); }
    /// ```
    unsafe fn init_elem(self, i: uint, val: T);

    /// Copies raw bytes from `src` to `self`.
    ///
    /// This does not run destructors on the overwritten elements, and
    /// ignores move semantics. `self` and `src` must not
    /// overlap. Fails if `self` is shorter than `src`.
    unsafe fn copy_memory(self, src: &[T]);
}

impl<'a,T> MutableVector<'a, T> for &'a mut [T] {
    #[inline]
    fn get_mut(self, index: uint) -> Option<&'a mut T> {
        if index < self.len() { Some(&mut self[index]) } else { None }
    }

    #[inline]
    fn as_mut_slice(self) -> &'a mut [T] { self }

    fn mut_slice(self, start: uint, end: uint) -> &'a mut [T] {
        assert!(start <= end);
        assert!(end <= self.len());
        unsafe {
            transmute(Slice {
                    data: self.as_mut_ptr().offset(start as int) as *const T,
                    len: (end - start)
                })
        }
    }

    #[inline]
    fn mut_slice_from(self, start: uint) -> &'a mut [T] {
        let len = self.len();
        self.mut_slice(start, len)
    }

    #[inline]
    fn mut_slice_to(self, end: uint) -> &'a mut [T] {
        self.mut_slice(0, end)
    }

    #[inline]
    fn mut_split_at(self, mid: uint) -> (&'a mut [T], &'a mut [T]) {
        unsafe {
            let len = self.len();
            let self2: &'a mut [T] = mem::transmute_copy(&self);
            (self.mut_slice(0, mid), self2.mut_slice(mid, len))
        }
    }

    #[inline]
    fn mut_iter(self) -> MutItems<'a, T> {
        unsafe {
            let p = self.as_mut_ptr();
            if mem::size_of::<T>() == 0 {
                MutItems{ptr: p,
                         end: (p as uint + self.len()) as *mut T,
                         marker: marker::ContravariantLifetime::<'a>,
                         marker2: marker::NoCopy}
            } else {
                MutItems{ptr: p,
                         end: p.offset(self.len() as int),
                         marker: marker::ContravariantLifetime::<'a>,
                         marker2: marker::NoCopy}
            }
        }
    }

    #[inline]
    fn mut_last(self) -> Option<&'a mut T> {
        let len = self.len();
        if len == 0 { return None; }
        Some(&mut self[len - 1])
    }

    #[inline]
    fn mut_split(self, pred: |&T|: 'a -> bool) -> MutSplits<'a, T> {
        MutSplits { v: self, pred: pred, finished: false }
    }

    #[inline]
    fn mut_chunks(self, chunk_size: uint) -> MutChunks<'a, T> {
        assert!(chunk_size > 0);
        MutChunks { v: self, chunk_size: chunk_size }
    }

    fn mut_shift_ref(&mut self) -> Option<&'a mut T> {
        unsafe {
            let s: &mut Slice<T> = transmute(self);
            match raw::shift_ptr(s) {
                // FIXME #13933: this `&` -> `&mut` cast is a little
                // dubious
                Some(p) => Some(&mut *(p as *mut _)),
                None => None,
            }
        }
    }

    fn mut_pop_ref(&mut self) -> Option<&'a mut T> {
        unsafe {
            let s: &mut Slice<T> = transmute(self);
            match raw::pop_ptr(s) {
                // FIXME #13933: this `&` -> `&mut` cast is a little
                // dubious
                Some(p) => Some(&mut *(p as *mut _)),
                None => None,
            }
        }
    }

    fn swap(self, a: uint, b: uint) {
        unsafe {
            // Can't take two mutable loans from one vector, so instead just cast
            // them to their raw pointers to do the swap
            let pa: *mut T = &mut self[a];
            let pb: *mut T = &mut self[b];
            ptr::swap(pa, pb);
        }
    }

    fn reverse(self) {
        let mut i: uint = 0;
        let ln = self.len();
        while i < ln / 2 {
            self.swap(i, ln - i - 1);
            i += 1;
        }
    }

    #[inline]
    unsafe fn unsafe_mut_ref(self, index: uint) -> &'a mut T {
        transmute((self.repr().data as *mut T).offset(index as int))
    }

    #[inline]
    fn as_mut_ptr(self) -> *mut T {
        self.repr().data as *mut T
    }

    #[inline]
    unsafe fn unsafe_set(self, index: uint, val: T) {
        *self.unsafe_mut_ref(index) = val;
    }

    #[inline]
    unsafe fn init_elem(self, i: uint, val: T) {
        ptr::write(&mut (*self.as_mut_ptr().offset(i as int)), val);
    }

    #[inline]
    unsafe fn copy_memory(self, src: &[T]) {
        let len_src = src.len();
        assert!(self.len() >= len_src);
        ptr::copy_nonoverlapping_memory(self.as_mut_ptr(), src.as_ptr(), len_src)
    }
}

/// Extension methods for vectors contain `PartialEq` elements.
pub trait ImmutableEqVector<T:PartialEq> {
    /// Find the first index containing a matching value
    fn position_elem(&self, t: &T) -> Option<uint>;

    /// Find the last index containing a matching value
    fn rposition_elem(&self, t: &T) -> Option<uint>;

    /// Return true if a vector contains an element with the given value
    fn contains(&self, x: &T) -> bool;

    /// Returns true if `needle` is a prefix of the vector.
    fn starts_with(&self, needle: &[T]) -> bool;

    /// Returns true if `needle` is a suffix of the vector.
    fn ends_with(&self, needle: &[T]) -> bool;
}

impl<'a,T:PartialEq> ImmutableEqVector<T> for &'a [T] {
    #[inline]
    fn position_elem(&self, x: &T) -> Option<uint> {
        self.iter().position(|y| *x == *y)
    }

    #[inline]
    fn rposition_elem(&self, t: &T) -> Option<uint> {
        self.iter().rposition(|x| *x == *t)
    }

    #[inline]
    fn contains(&self, x: &T) -> bool {
        self.iter().any(|elt| *x == *elt)
    }

    #[inline]
    fn starts_with(&self, needle: &[T]) -> bool {
        let n = needle.len();
        self.len() >= n && needle == self.slice_to(n)
    }

    #[inline]
    fn ends_with(&self, needle: &[T]) -> bool {
        let (m, n) = (self.len(), needle.len());
        m >= n && needle == self.slice_from(m - n)
    }
}

/// Extension methods for vectors containing `Ord` elements.
pub trait ImmutableOrdVector<T: Ord> {
    /**
     * Binary search a sorted vector for a given element.
     *
     * Returns the index of the element or None if not found.
     */
    fn bsearch_elem(&self, x: &T) -> Option<uint>;
}

impl<'a, T: Ord> ImmutableOrdVector<T> for &'a [T] {
    fn bsearch_elem(&self, x: &T) -> Option<uint> {
        self.bsearch(|p| p.cmp(x))
    }
}

/// Trait for &[T] where T is Cloneable
pub trait MutableCloneableVector<T> {
    /// Copies as many elements from `src` as it can into `self` (the
    /// shorter of `self.len()` and `src.len()`). Returns the number
    /// of elements copied.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::slice::MutableCloneableVector;
    ///
    /// let mut dst = [0i, 0, 0];
    /// let src = [1i, 2];
    ///
    /// assert!(dst.copy_from(src) == 2);
    /// assert!(dst == [1, 2, 0]);
    ///
    /// let src2 = [3i, 4, 5, 6];
    /// assert!(dst.copy_from(src2) == 3);
    /// assert!(dst == [3i, 4, 5]);
    /// ```
    fn copy_from(self, &[T]) -> uint;
}

impl<'a, T:Clone> MutableCloneableVector<T> for &'a mut [T] {
    #[inline]
    fn copy_from(self, src: &[T]) -> uint {
        for (a, b) in self.mut_iter().zip(src.iter()) {
            a.clone_from(b);
        }
        cmp::min(self.len(), src.len())
    }
}




//
// Common traits
//

/// Any vector that can be represented as a slice.
pub trait Vector<T> {
    /// Work with `self` as a slice.
    fn as_slice<'a>(&'a self) -> &'a [T];
}

impl<'a,T> Vector<T> for &'a [T] {
    #[inline(always)]
    fn as_slice<'a>(&'a self) -> &'a [T] { *self }
}

impl<'a, T> Collection for &'a [T] {
    /// Returns the length of a vector
    #[inline]
    fn len(&self) -> uint {
        self.repr().len
    }
}

impl<'a, T> Default for &'a [T] {
    fn default() -> &'a [T] { &[] }
}




//
// Iterators
//

// The shared definition of the `Item` and `MutItems` iterators
macro_rules! iterator {
    (struct $name:ident -> $ptr:ty, $elem:ty) => {
        impl<'a, T> Iterator<$elem> for $name<'a, T> {
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

        impl<'a, T> DoubleEndedIterator<$elem> for $name<'a, T> {
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

/// Immutable slice iterator
pub struct Items<'a, T> {
    ptr: *const T,
    end: *const T,
    marker: marker::ContravariantLifetime<'a>
}

iterator!{struct Items -> *const T, &'a T}

impl<'a, T> ExactSize<&'a T> for Items<'a, T> {}

impl<'a, T> Clone for Items<'a, T> {
    fn clone(&self) -> Items<'a, T> { *self }
}

impl<'a, T> RandomAccessIterator<&'a T> for Items<'a, T> {
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

/// Mutable slice iterator
pub struct MutItems<'a, T> {
    ptr: *mut T,
    end: *mut T,
    marker: marker::ContravariantLifetime<'a>,
    marker2: marker::NoCopy
}

iterator!{struct MutItems -> *mut T, &'a mut T}

impl<'a, T> ExactSize<&'a mut T> for MutItems<'a, T> {}

/// An iterator over the slices of a vector separated by elements that
/// match a predicate function.
pub struct Splits<'a, T> {
    v: &'a [T],
    pred: |t: &T|: 'a -> bool,
    finished: bool
}

impl<'a, T> Iterator<&'a [T]> for Splits<'a, T> {
    #[inline]
    fn next(&mut self) -> Option<&'a [T]> {
        if self.finished { return None; }

        match self.v.iter().position(|x| (self.pred)(x)) {
            None => {
                self.finished = true;
                Some(self.v)
            }
            Some(idx) => {
                let ret = Some(self.v.slice(0, idx));
                self.v = self.v.slice(idx + 1, self.v.len());
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

impl<'a, T> DoubleEndedIterator<&'a [T]> for Splits<'a, T> {
    #[inline]
    fn next_back(&mut self) -> Option<&'a [T]> {
        if self.finished { return None; }

        match self.v.iter().rposition(|x| (self.pred)(x)) {
            None => {
                self.finished = true;
                Some(self.v)
            }
            Some(idx) => {
                let ret = Some(self.v.slice(idx + 1, self.v.len()));
                self.v = self.v.slice(0, idx);
                ret
            }
        }
    }
}

/// An iterator over the subslices of the vector which are separated
/// by elements that match `pred`.
pub struct MutSplits<'a, T> {
    v: &'a mut [T],
    pred: |t: &T|: 'a -> bool,
    finished: bool
}

impl<'a, T> Iterator<&'a mut [T]> for MutSplits<'a, T> {
    #[inline]
    fn next(&mut self) -> Option<&'a mut [T]> {
        if self.finished { return None; }

        let pred = &mut self.pred;
        match self.v.iter().position(|x| (*pred)(x)) {
            None => {
                self.finished = true;
                let tmp = mem::replace(&mut self.v, &mut []);
                let len = tmp.len();
                let (head, tail) = tmp.mut_split_at(len);
                self.v = tail;
                Some(head)
            }
            Some(idx) => {
                let tmp = mem::replace(&mut self.v, &mut []);
                let (head, tail) = tmp.mut_split_at(idx);
                self.v = tail.mut_slice_from(1);
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

impl<'a, T> DoubleEndedIterator<&'a mut [T]> for MutSplits<'a, T> {
    #[inline]
    fn next_back(&mut self) -> Option<&'a mut [T]> {
        if self.finished { return None; }

        let pred = &mut self.pred;
        match self.v.iter().rposition(|x| (*pred)(x)) {
            None => {
                self.finished = true;
                let tmp = mem::replace(&mut self.v, &mut []);
                Some(tmp)
            }
            Some(idx) => {
                let tmp = mem::replace(&mut self.v, &mut []);
                let (head, tail) = tmp.mut_split_at(idx);
                self.v = head;
                Some(tail.mut_slice_from(1))
            }
        }
    }
}

/// An iterator over the slices of a vector separated by elements that
/// match a predicate function, splitting at most a fixed number of times.
pub struct SplitsN<'a, T> {
    iter: Splits<'a, T>,
    count: uint,
    invert: bool
}

impl<'a, T> Iterator<&'a [T]> for SplitsN<'a, T> {
    #[inline]
    fn next(&mut self) -> Option<&'a [T]> {
        if self.count == 0 {
            if self.iter.finished {
                None
            } else {
                self.iter.finished = true;
                Some(self.iter.v)
            }
        } else {
            self.count -= 1;
            if self.invert { self.iter.next_back() } else { self.iter.next() }
        }
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        if self.iter.finished {
            (0, Some(0))
        } else {
            (1, Some(cmp::min(self.count, self.iter.v.len()) + 1))
        }
    }
}

/// An iterator over the (overlapping) slices of length `size` within
/// a vector.
#[deriving(Clone)]
pub struct Windows<'a, T> {
    v: &'a [T],
    size: uint
}

impl<'a, T> Iterator<&'a [T]> for Windows<'a, T> {
    #[inline]
    fn next(&mut self) -> Option<&'a [T]> {
        if self.size > self.v.len() {
            None
        } else {
            let ret = Some(self.v.slice(0, self.size));
            self.v = self.v.slice(1, self.v.len());
            ret
        }
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        if self.size > self.v.len() {
            (0, Some(0))
        } else {
            let x = self.v.len() - self.size;
            (x.saturating_add(1), x.checked_add(&1u))
        }
    }
}

/// An iterator over a vector in (non-overlapping) chunks (`size`
/// elements at a time).
///
/// When the vector len is not evenly divided by the chunk size,
/// the last slice of the iteration will be the remainder.
#[deriving(Clone)]
pub struct Chunks<'a, T> {
    v: &'a [T],
    size: uint
}

impl<'a, T> Iterator<&'a [T]> for Chunks<'a, T> {
    #[inline]
    fn next(&mut self) -> Option<&'a [T]> {
        if self.v.len() == 0 {
            None
        } else {
            let chunksz = cmp::min(self.v.len(), self.size);
            let (fst, snd) = (self.v.slice_to(chunksz),
                              self.v.slice_from(chunksz));
            self.v = snd;
            Some(fst)
        }
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        if self.v.len() == 0 {
            (0, Some(0))
        } else {
            let (n, rem) = div_rem(self.v.len(), self.size);
            let n = if rem > 0 { n+1 } else { n };
            (n, Some(n))
        }
    }
}

impl<'a, T> DoubleEndedIterator<&'a [T]> for Chunks<'a, T> {
    #[inline]
    fn next_back(&mut self) -> Option<&'a [T]> {
        if self.v.len() == 0 {
            None
        } else {
            let remainder = self.v.len() % self.size;
            let chunksz = if remainder != 0 { remainder } else { self.size };
            let (fst, snd) = (self.v.slice_to(self.v.len() - chunksz),
                              self.v.slice_from(self.v.len() - chunksz));
            self.v = fst;
            Some(snd)
        }
    }
}

impl<'a, T> RandomAccessIterator<&'a [T]> for Chunks<'a, T> {
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

            Some(self.v.slice(lo, hi))
        } else {
            None
        }
    }
}

/// An iterator over a vector in (non-overlapping) mutable chunks (`size`  elements at a time). When
/// the vector len is not evenly divided by the chunk size, the last slice of the iteration will be
/// the remainder.
pub struct MutChunks<'a, T> {
    v: &'a mut [T],
    chunk_size: uint
}

impl<'a, T> Iterator<&'a mut [T]> for MutChunks<'a, T> {
    #[inline]
    fn next(&mut self) -> Option<&'a mut [T]> {
        if self.v.len() == 0 {
            None
        } else {
            let sz = cmp::min(self.v.len(), self.chunk_size);
            let tmp = mem::replace(&mut self.v, &mut []);
            let (head, tail) = tmp.mut_split_at(sz);
            self.v = tail;
            Some(head)
        }
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        if self.v.len() == 0 {
            (0, Some(0))
        } else {
            let (n, rem) = div_rem(self.v.len(), self.chunk_size);
            let n = if rem > 0 { n + 1 } else { n };
            (n, Some(n))
        }
    }
}

impl<'a, T> DoubleEndedIterator<&'a mut [T]> for MutChunks<'a, T> {
    #[inline]
    fn next_back(&mut self) -> Option<&'a mut [T]> {
        if self.v.len() == 0 {
            None
        } else {
            let remainder = self.v.len() % self.chunk_size;
            let sz = if remainder != 0 { remainder } else { self.chunk_size };
            let tmp = mem::replace(&mut self.v, &mut []);
            let tmp_len = tmp.len();
            let (head, tail) = tmp.mut_split_at(tmp_len - sz);
            self.v = head;
            Some(tail)
        }
    }
}




//
// Free functions
//

/**
 * Converts a pointer to A into a slice of length 1 (without copying).
 */
pub fn ref_slice<'a, A>(s: &'a A) -> &'a [A] {
    unsafe {
        transmute(Slice { data: s, len: 1 })
    }
}

/**
 * Converts a pointer to A into a slice of length 1 (without copying).
 */
pub fn mut_ref_slice<'a, A>(s: &'a mut A) -> &'a mut [A] {
    unsafe {
        let ptr: *const A = transmute(s);
        transmute(Slice { data: ptr, len: 1 })
    }
}




//
// Submodules
//

/// Unsafe operations
pub mod raw {
    use mem::transmute;
    use ptr::RawPtr;
    use raw::Slice;
    use option::{None, Option, Some};

    /**
     * Form a slice from a pointer and length (as a number of units,
     * not bytes).
     */
    #[inline]
    pub unsafe fn buf_as_slice<T,U>(p: *const T, len: uint, f: |v: &[T]| -> U)
                               -> U {
        f(transmute(Slice {
            data: p,
            len: len
        }))
    }

    /**
     * Form a slice from a pointer and length (as a number of units,
     * not bytes).
     */
    #[inline]
    pub unsafe fn mut_buf_as_slice<T,
                                   U>(
                                   p: *mut T,
                                   len: uint,
                                   f: |v: &mut [T]| -> U)
                                   -> U {
        f(transmute(Slice {
            data: p as *const T,
            len: len
        }))
    }

    /**
     * Returns a pointer to first element in slice and adjusts
     * slice so it no longer contains that element. Returns None
     * if the slice is empty. O(1).
     */
     #[inline]
    pub unsafe fn shift_ptr<T>(slice: &mut Slice<T>) -> Option<*const T> {
        if slice.len == 0 { return None; }
        let head: *const T = slice.data;
        slice.data = slice.data.offset(1);
        slice.len -= 1;
        Some(head)
    }

    /**
     * Returns a pointer to last element in slice and adjusts
     * slice so it no longer contains that element. Returns None
     * if the slice is empty. O(1).
     */
     #[inline]
    pub unsafe fn pop_ptr<T>(slice: &mut Slice<T>) -> Option<*const T> {
        if slice.len == 0 { return None; }
        let tail: *const T = slice.data.offset((slice.len - 1) as int);
        slice.len -= 1;
        Some(tail)
    }
}

/// Operations on `[u8]`.
pub mod bytes {
    use collections::Collection;
    use ptr;
    use slice::MutableVector;

    /// A trait for operations on mutable `[u8]`s.
    pub trait MutableByteVector {
        /// Sets all bytes of the receiver to the given value.
        fn set_memory(self, value: u8);
    }

    impl<'a> MutableByteVector for &'a mut [u8] {
        #[inline]
        #[allow(experimental)]
        fn set_memory(self, value: u8) {
            unsafe { ptr::set_memory(self.as_mut_ptr(), value, self.len()) };
        }
    }

    /// Copies data from `src` to `dst`
    ///
    /// `src` and `dst` must not overlap. Fails if the length of `dst`
    /// is less than the length of `src`.
    #[inline]
    pub fn copy_memory(dst: &mut [u8], src: &[u8]) {
        // Bound checks are done at .copy_memory.
        unsafe { dst.copy_memory(src) }
    }
}




//
// Boilerplate traits
//

impl<'a,T:PartialEq> PartialEq for &'a [T] {
    fn eq(&self, other: & &'a [T]) -> bool {
        self.len() == other.len() &&
            order::eq(self.iter(), other.iter())
    }
    fn ne(&self, other: & &'a [T]) -> bool {
        self.len() != other.len() ||
            order::ne(self.iter(), other.iter())
    }
}

impl<'a,T:Eq> Eq for &'a [T] {}

impl<'a,T:PartialEq, V: Vector<T>> Equiv<V> for &'a [T] {
    #[inline]
    fn equiv(&self, other: &V) -> bool { self.as_slice() == other.as_slice() }
}

impl<'a,T:Ord> Ord for &'a [T] {
    fn cmp(&self, other: & &'a [T]) -> Ordering {
        order::cmp(self.iter(), other.iter())
    }
}

impl<'a, T: PartialOrd> PartialOrd for &'a [T] {
    #[inline]
    fn partial_cmp(&self, other: &&'a [T]) -> Option<Ordering> {
        order::partial_cmp(self.iter(), other.iter())
    }
    #[inline]
    fn lt(&self, other: & &'a [T]) -> bool {
        order::lt(self.iter(), other.iter())
    }
    #[inline]
    fn le(&self, other: & &'a [T]) -> bool {
        order::le(self.iter(), other.iter())
    }
    #[inline]
    fn ge(&self, other: & &'a [T]) -> bool {
        order::ge(self.iter(), other.iter())
    }
    #[inline]
    fn gt(&self, other: & &'a [T]) -> bool {
        order::gt(self.iter(), other.iter())
    }
}
