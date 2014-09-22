// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! A growable list type, written `Vec<T>` but pronounced 'vector.'
//!
//! Vectors have `O(1)` indexing, push (to the end) and pop (from the end).

use core::prelude::*;

use alloc::heap::{EMPTY, allocate, reallocate, deallocate};
use core::cmp::max;
use core::default::Default;
use core::fmt;
use core::mem;
use core::num;
use core::ops;
use core::ptr;
use core::raw::Slice as RawSlice;
use core::uint;

use {Mutable, MutableSeq};
use slice::{MutableOrdSlice, MutableSliceAllocating, CloneableVector};
use slice::{Items, MutItems};

/// An owned, growable vector.
///
/// # Examples
///
/// ```
/// let mut vec = Vec::new();
/// vec.push(1i);
/// vec.push(2i);
///
/// assert_eq!(vec.len(), 2);
/// assert_eq!(vec[0], 1);
///
/// assert_eq!(vec.pop(), Some(2));
/// assert_eq!(vec.len(), 1);
///
/// *vec.get_mut(0) = 7i;
/// assert_eq!(vec[0], 7);
///
/// vec.push_all([1, 2, 3]);
///
/// for x in vec.iter() {
///     println!("{}", x);
/// }
/// assert_eq!(vec, vec![7i, 1, 2, 3]);
/// ```
///
/// The `vec!` macro is provided to make initialization more convenient:
///
/// ```
/// let mut vec = vec![1i, 2i, 3i];
/// vec.push(4);
/// assert_eq!(vec, vec![1, 2, 3, 4]);
/// ```
///
/// Use a `Vec` as an efficient stack:
///
/// ```
/// let mut stack = Vec::new();
///
/// stack.push(1i);
/// stack.push(2i);
/// stack.push(3i);
///
/// loop {
///     let top = match stack.pop() {
///         None => break, // empty
///         Some(x) => x,
///     };
///     // Prints 3, 2, 1
///     println!("{}", top);
/// }
/// ```
///
/// # Capacity and reallocation
///
/// The capacity of a vector is the amount of space allocated for any future
/// elements that will be added onto the vector. This is not to be confused
/// with the *length* of a vector, which specifies the number of actual
/// elements within the vector. If a vector's length exceeds its capacity,
/// its capacity will automatically be increased, but its elements will
/// have to be reallocated.
///
/// For example, a vector with capacity 10 and length 0 would be an empty
/// vector with space for 10 more elements. Pushing 10 or fewer elements onto
/// the vector will not change its capacity or cause reallocation to occur.
/// However, if the vector's length is increased to 11, it will have to
/// reallocate, which can be slow. For this reason, it is recommended
/// to use `Vec::with_capacity` whenever possible to specify how big the vector
/// is expected to get.
#[unsafe_no_drop_flag]
#[stable]
pub struct Vec<T> {
    len: uint,
    cap: uint,
    ptr: *mut T
}

impl<T> Vec<T> {
    /// Constructs a new, empty `Vec`.
    ///
    /// The vector will not allocate until elements are pushed onto it.
    ///
    /// # Example
    ///
    /// ```
    /// let mut vec: Vec<int> = Vec::new();
    /// ```
    #[inline]
    #[stable]
    pub fn new() -> Vec<T> {
        // We want ptr to never be NULL so instead we set it to some arbitrary
        // non-null value which is fine since we never call deallocate on the ptr
        // if cap is 0. The reason for this is because the pointer of a slice
        // being NULL would break the null pointer optimization for enums.
        Vec { len: 0, cap: 0, ptr: EMPTY as *mut T }
    }

    /// Constructs a new, empty `Vec` with the specified capacity.
    ///
    /// The vector will be able to hold exactly `capacity` elements without
    /// reallocating. If `capacity` is 0, the vector will not allocate.
    ///
    /// It is important to note that this function does not specify the
    /// *length* of the returned vector, but only the *capacity*. (For an
    /// explanation of the difference between length and capacity, see
    /// the main `Vec` docs above, 'Capacity and reallocation'.) To create
    /// a vector of a given length, use `Vec::from_elem` or `Vec::from_fn`.
    ///
    /// # Example
    ///
    /// ```
    /// let mut vec: Vec<int> = Vec::with_capacity(10);
    ///
    /// // The vector contains no items, even though it has capacity for more
    /// assert_eq!(vec.len(), 0);
    ///
    /// // These are all done without reallocating...
    /// for i in range(0i, 10) {
    ///     vec.push(i);
    /// }
    ///
    /// // ...but this may make the vector reallocate
    /// vec.push(11);
    /// ```
    #[inline]
    #[stable]
    pub fn with_capacity(capacity: uint) -> Vec<T> {
        if mem::size_of::<T>() == 0 {
            Vec { len: 0, cap: uint::MAX, ptr: EMPTY as *mut T }
        } else if capacity == 0 {
            Vec::new()
        } else {
            let size = capacity.checked_mul(&mem::size_of::<T>())
                               .expect("capacity overflow");
            let ptr = unsafe { allocate(size, mem::min_align_of::<T>()) };
            Vec { len: 0, cap: capacity, ptr: ptr as *mut T }
        }
    }

    /// Creates and initializes a `Vec`.
    ///
    /// Creates a `Vec` of size `length` and initializes the elements to the
    /// value returned by the closure `op`.
    ///
    /// # Example
    ///
    /// ```
    /// let vec = Vec::from_fn(3, |idx| idx * 2);
    /// assert_eq!(vec, vec![0, 2, 4]);
    /// ```
    #[inline]
    #[unstable = "the naming is uncertain as well as this migrating to unboxed \
                  closures in the future"]
    pub fn from_fn(length: uint, op: |uint| -> T) -> Vec<T> {
        unsafe {
            let mut xs = Vec::with_capacity(length);
            while xs.len < length {
                let len = xs.len;
                ptr::write(xs.as_mut_slice().unsafe_mut(len), op(len));
                xs.len += 1;
            }
            xs
        }
    }

    /// Creates a `Vec<T>` directly from the raw constituents.
    ///
    /// This is highly unsafe:
    ///
    /// - if `ptr` is null, then `length` and `capacity` should be 0
    /// - `ptr` must point to an allocation of size `capacity`
    /// - there must be `length` valid instances of type `T` at the
    ///   beginning of that allocation
    /// - `ptr` must be allocated by the default `Vec` allocator
    ///
    /// # Example
    ///
    /// ```
    /// use std::ptr;
    /// use std::mem;
    ///
    /// fn main() {
    ///     let mut v = vec![1i, 2, 3];
    ///
    ///     // Pull out the various important pieces of information about `v`
    ///     let p = v.as_mut_ptr();
    ///     let len = v.len();
    ///     let cap = v.capacity();
    ///
    ///     unsafe {
    ///         // Cast `v` into the void: no destructor run, so we are in
    ///         // complete control of the allocation to which `p` points.
    ///         mem::forget(v);
    ///
    ///         // Overwrite memory with 4, 5, 6
    ///         for i in range(0, len as int) {
    ///             ptr::write(p.offset(i), 4 + i);
    ///         }
    ///
    ///         // Put everything back together into a Vec
    ///         let rebuilt = Vec::from_raw_parts(len, cap, p);
    ///         assert_eq!(rebuilt, vec![4i, 5i, 6i]);
    ///     }
    /// }
    /// ```
    #[experimental]
    pub unsafe fn from_raw_parts(length: uint, capacity: uint,
                                 ptr: *mut T) -> Vec<T> {
        Vec { len: length, cap: capacity, ptr: ptr }
    }

    /// Consumes the `Vec`, partitioning it based on a predicate.
    ///
    /// Partitions the `Vec` into two `Vec`s `(A,B)`, where all elements of `A`
    /// satisfy `f` and all elements of `B` do not. The order of elements is
    /// preserved.
    ///
    /// # Example
    ///
    /// ```
    /// let vec = vec![1i, 2i, 3i, 4i];
    /// let (even, odd) = vec.partition(|&n| n % 2 == 0);
    /// assert_eq!(even, vec![2, 4]);
    /// assert_eq!(odd, vec![1, 3]);
    /// ```
    #[inline]
    #[experimental]
    pub fn partition(self, f: |&T| -> bool) -> (Vec<T>, Vec<T>) {
        let mut lefts  = Vec::new();
        let mut rights = Vec::new();

        for elt in self.into_iter() {
            if f(&elt) {
                lefts.push(elt);
            } else {
                rights.push(elt);
            }
        }

        (lefts, rights)
    }
}

impl<T: Clone> Vec<T> {
    /// Deprecated, call `extend` instead.
    #[inline]
    #[deprecated = "this function has been deprecated in favor of extend()"]
    pub fn append(mut self, second: &[T]) -> Vec<T> {
        self.push_all(second);
        self
    }

    /// Deprecated, call `to_vec()` instead
    #[inline]
    #[deprecated = "this function has been deprecated in favor of to_vec()"]
    pub fn from_slice(values: &[T]) -> Vec<T> { values.to_vec() }

    /// Constructs a `Vec` with copies of a value.
    ///
    /// Creates a `Vec` with `length` copies of `value`.
    ///
    /// # Example
    /// ```
    /// let vec = Vec::from_elem(3, "hi");
    /// println!("{}", vec); // prints [hi, hi, hi]
    /// ```
    #[inline]
    #[unstable = "this functionality may become more generic over all collections"]
    pub fn from_elem(length: uint, value: T) -> Vec<T> {
        unsafe {
            let mut xs = Vec::with_capacity(length);
            while xs.len < length {
                let len = xs.len;
                ptr::write(xs.as_mut_slice().unsafe_mut(len),
                           value.clone());
                xs.len += 1;
            }
            xs
        }
    }

    /// Appends all elements in a slice to the `Vec`.
    ///
    /// Iterates over the slice `other`, clones each element, and then appends
    /// it to this `Vec`. The `other` vector is traversed in-order.
    ///
    /// # Example
    ///
    /// ```
    /// let mut vec = vec![1i];
    /// vec.push_all([2i, 3, 4]);
    /// assert_eq!(vec, vec![1, 2, 3, 4]);
    /// ```
    #[inline]
    #[experimental]
    pub fn push_all(&mut self, other: &[T]) {
        self.reserve_additional(other.len());

        for i in range(0, other.len()) {
            let len = self.len();

            // Unsafe code so this can be optimised to a memcpy (or something similarly
            // fast) when T is Copy. LLVM is easily confused, so any extra operations
            // during the loop can prevent this optimisation.
            unsafe {
                ptr::write(
                    self.as_mut_slice().unsafe_mut(len),
                    other.unsafe_get(i).clone());
                self.set_len(len + 1);
            }
        }
    }

    /// Grows the `Vec` in-place.
    ///
    /// Adds `n` copies of `value` to the `Vec`.
    ///
    /// # Example
    ///
    /// ```
    /// let mut vec = vec!["hello"];
    /// vec.grow(2, "world");
    /// assert_eq!(vec, vec!["hello", "world", "world"]);
    /// ```
    #[stable]
    pub fn grow(&mut self, n: uint, value: T) {
        self.reserve_additional(n);
        let mut i: uint = 0u;

        while i < n {
            self.push(value.clone());
            i += 1u;
        }
    }

    /// Sets the value of a vector element at a given index, growing the vector
    /// as needed.
    ///
    /// Sets the element at position `index` to `value`. If `index` is past the
    /// end of the vector, expands the vector by replicating `initval` to fill
    /// the intervening space.
    ///
    /// # Example
    ///
    /// ```
    /// # #![allow(deprecated)]
    /// let mut vec = vec!["a", "b", "c"];
    /// vec.grow_set(1, &("fill"), "d");
    /// vec.grow_set(4, &("fill"), "e");
    /// assert_eq!(vec, vec!["a", "d", "c", "fill", "e"]);
    /// ```
    #[deprecated = "call .grow() and .push() manually instead"]
    pub fn grow_set(&mut self, index: uint, initval: &T, value: T) {
        let l = self.len();
        if index >= l {
            self.grow(index - l + 1u, initval.clone());
        }
        *self.get_mut(index) = value;
    }

    /// Partitions a vector based on a predicate.
    ///
    /// Clones the elements of the vector, partitioning them into two `Vec`s
    /// `(a, b)`, where all elements of `a` satisfy `f` and all elements of `b`
    /// do not. The order of elements is preserved.
    ///
    /// # Example
    ///
    /// ```
    /// let vec = vec![1i, 2, 3, 4];
    /// let (even, odd) = vec.partitioned(|&n| n % 2 == 0);
    /// assert_eq!(even, vec![2i, 4]);
    /// assert_eq!(odd, vec![1i, 3]);
    /// ```
    #[experimental]
    pub fn partitioned(&self, f: |&T| -> bool) -> (Vec<T>, Vec<T>) {
        let mut lefts = Vec::new();
        let mut rights = Vec::new();

        for elt in self.iter() {
            if f(elt) {
                lefts.push(elt.clone());
            } else {
                rights.push(elt.clone());
            }
        }

        (lefts, rights)
    }
}

#[unstable]
impl<T:Clone> Clone for Vec<T> {
    fn clone(&self) -> Vec<T> { self.as_slice().to_vec() }

    fn clone_from(&mut self, other: &Vec<T>) {
        // drop anything in self that will not be overwritten
        if self.len() > other.len() {
            self.truncate(other.len())
        }

        // reuse the contained values' allocations/resources.
        for (place, thing) in self.iter_mut().zip(other.iter()) {
            place.clone_from(thing)
        }

        // self.len <= other.len due to the truncate above, so the
        // slice here is always in-bounds.
        let slice = other.slice_from(self.len());
        self.push_all(slice);
    }
}

#[experimental = "waiting on Index stability"]
impl<T> Index<uint,T> for Vec<T> {
    #[inline]
    #[allow(deprecated)] // allow use of get
    fn index<'a>(&'a self, index: &uint) -> &'a T {
        self.get(*index)
    }
}

// FIXME(#12825) Indexing will always try IndexMut first and that causes issues.
/*impl<T> IndexMut<uint,T> for Vec<T> {
    #[inline]
    fn index_mut<'a>(&'a mut self, index: &uint) -> &'a mut T {
        self.get_mut(*index)
    }
}*/

impl<T> ops::Slice<uint, [T]> for Vec<T> {
    #[inline]
    fn as_slice_<'a>(&'a self) -> &'a [T] {
        self.as_slice()
    }

    #[inline]
    fn slice_from_<'a>(&'a self, start: &uint) -> &'a [T] {
        self.as_slice().slice_from_(start)
    }

    #[inline]
    fn slice_to_<'a>(&'a self, end: &uint) -> &'a [T] {
        self.as_slice().slice_to_(end)
    }
    #[inline]
    fn slice_<'a>(&'a self, start: &uint, end: &uint) -> &'a [T] {
        self.as_slice().slice_(start, end)
    }
}

impl<T> ops::SliceMut<uint, [T]> for Vec<T> {
    #[inline]
    fn as_mut_slice_<'a>(&'a mut self) -> &'a mut [T] {
        self.as_mut_slice()
    }

    #[inline]
    fn slice_from_mut_<'a>(&'a mut self, start: &uint) -> &'a mut [T] {
        self.as_mut_slice().slice_from_mut_(start)
    }

    #[inline]
    fn slice_to_mut_<'a>(&'a mut self, end: &uint) -> &'a mut [T] {
        self.as_mut_slice().slice_to_mut_(end)
    }
    #[inline]
    fn slice_mut_<'a>(&'a mut self, start: &uint, end: &uint) -> &'a mut [T] {
        self.as_mut_slice().slice_mut_(start, end)
    }
}

#[experimental = "waiting on FromIterator stability"]
impl<T> FromIterator<T> for Vec<T> {
    #[inline]
    fn from_iter<I:Iterator<T>>(mut iterator: I) -> Vec<T> {
        let (lower, _) = iterator.size_hint();
        let mut vector = Vec::with_capacity(lower);
        for element in iterator {
            vector.push(element)
        }
        vector
    }
}

#[experimental = "waiting on Extendable stability"]
impl<T> Extendable<T> for Vec<T> {
    #[inline]
    fn extend<I: Iterator<T>>(&mut self, mut iterator: I) {
        let (lower, _) = iterator.size_hint();
        self.reserve_additional(lower);
        for element in iterator {
            self.push(element)
        }
    }
}

#[unstable = "waiting on PartialEq stability"]
impl<T: PartialEq> PartialEq for Vec<T> {
    #[inline]
    fn eq(&self, other: &Vec<T>) -> bool {
        self.as_slice() == other.as_slice()
    }
}

#[unstable = "waiting on PartialOrd stability"]
impl<T: PartialOrd> PartialOrd for Vec<T> {
    #[inline]
    fn partial_cmp(&self, other: &Vec<T>) -> Option<Ordering> {
        self.as_slice().partial_cmp(&other.as_slice())
    }
}

#[unstable = "waiting on Eq stability"]
impl<T: Eq> Eq for Vec<T> {}

#[experimental]
impl<T: PartialEq, V: Slice<T>> Equiv<V> for Vec<T> {
    #[inline]
    fn equiv(&self, other: &V) -> bool { self.as_slice() == other.as_slice() }
}

#[unstable = "waiting on Ord stability"]
impl<T: Ord> Ord for Vec<T> {
    #[inline]
    fn cmp(&self, other: &Vec<T>) -> Ordering {
        self.as_slice().cmp(&other.as_slice())
    }
}

#[experimental = "waiting on Collection stability"]
impl<T> Collection for Vec<T> {
    #[inline]
    #[stable]
    fn len(&self) -> uint {
        self.len
    }
}

impl<T: Clone> CloneableVector<T> for Vec<T> {
    #[deprecated = "call .clone() instead"]
    fn to_vec(&self) -> Vec<T> { self.clone() }
    #[deprecated = "move the vector instead"]
    fn into_vec(self) -> Vec<T> { self }
}

// FIXME: #13996: need a way to mark the return value as `noalias`
#[inline(never)]
unsafe fn alloc_or_realloc<T>(ptr: *mut T, size: uint, old_size: uint) -> *mut T {
    if old_size == 0 {
        allocate(size, mem::min_align_of::<T>()) as *mut T
    } else {
        reallocate(ptr as *mut u8, size,
                   mem::min_align_of::<T>(), old_size) as *mut T
    }
}

#[inline]
unsafe fn dealloc<T>(ptr: *mut T, len: uint) {
    if mem::size_of::<T>() != 0 {
        deallocate(ptr as *mut u8,
                   len * mem::size_of::<T>(),
                   mem::min_align_of::<T>())
    }
}

impl<T> Vec<T> {
    /// Returns the number of elements the vector can hold without
    /// reallocating.
    ///
    /// # Example
    ///
    /// ```
    /// let vec: Vec<int> = Vec::with_capacity(10);
    /// assert_eq!(vec.capacity(), 10);
    /// ```
    #[inline]
    #[stable]
    pub fn capacity(&self) -> uint {
        self.cap
    }

     /// Reserves capacity for at least `n` additional elements in the given
     /// vector.
     ///
     /// # Failure
     ///
     /// Fails if the new capacity overflows `uint`.
     ///
     /// # Example
     ///
     /// ```
     /// let mut vec: Vec<int> = vec![1i];
     /// vec.reserve_additional(10);
     /// assert!(vec.capacity() >= 11);
     /// ```
    pub fn reserve_additional(&mut self, extra: uint) {
        if self.cap - self.len < extra {
            match self.len.checked_add(&extra) {
                None => fail!("Vec::reserve_additional: `uint` overflow"),
                Some(new_cap) => self.reserve(new_cap)
            }
        }
    }

    /// Reserves capacity for at least `n` elements in the given vector.
    ///
    /// This function will over-allocate in order to amortize the allocation
    /// costs in scenarios where the caller may need to repeatedly reserve
    /// additional space.
    ///
    /// If the capacity for `self` is already equal to or greater than the
    /// requested capacity, then no action is taken.
    ///
    /// # Example
    ///
    /// ```
    /// let mut vec = vec![1i, 2, 3];
    /// vec.reserve(10);
    /// assert!(vec.capacity() >= 10);
    /// ```
    pub fn reserve(&mut self, capacity: uint) {
        if capacity > self.cap {
            self.reserve_exact(num::next_power_of_two(capacity))
        }
    }

    /// Reserves capacity for exactly `capacity` elements in the given vector.
    ///
    /// If the capacity for `self` is already equal to or greater than the
    /// requested capacity, then no action is taken.
    ///
    /// # Example
    ///
    /// ```
    /// let mut vec: Vec<int> = Vec::with_capacity(10);
    /// vec.reserve_exact(11);
    /// assert_eq!(vec.capacity(), 11);
    /// ```
    pub fn reserve_exact(&mut self, capacity: uint) {
        if mem::size_of::<T>() == 0 { return }

        if capacity > self.cap {
            let size = capacity.checked_mul(&mem::size_of::<T>())
                               .expect("capacity overflow");
            unsafe {
                self.ptr = alloc_or_realloc(self.ptr, size,
                                            self.cap * mem::size_of::<T>());
            }
            self.cap = capacity;
        }
    }

    /// Shrinks the capacity of the vector as much as possible.
    ///
    /// # Example
    ///
    /// ```
    /// let mut vec = vec![1i, 2, 3];
    /// vec.shrink_to_fit();
    /// ```
    #[stable]
    pub fn shrink_to_fit(&mut self) {
        if mem::size_of::<T>() == 0 { return }

        if self.len == 0 {
            if self.cap != 0 {
                unsafe {
                    dealloc(self.ptr, self.cap)
                }
                self.cap = 0;
            }
        } else {
            unsafe {
                // Overflow check is unnecessary as the vector is already at
                // least this large.
                self.ptr = reallocate(self.ptr as *mut u8,
                                      self.len * mem::size_of::<T>(),
                                      mem::min_align_of::<T>(),
                                      self.cap * mem::size_of::<T>()) as *mut T;
            }
            self.cap = self.len;
        }
    }

    /// Deprecated, call `push` instead
    #[inline]
    #[deprecated = "call .push() instead"]
    pub fn append_one(mut self, x: T) -> Vec<T> {
        self.push(x);
        self
    }

    /// Shorten a vector, dropping excess elements.
    ///
    /// If `len` is greater than the vector's current length, this has no
    /// effect.
    ///
    /// # Example
    ///
    /// ```
    /// let mut vec = vec![1i, 2, 3, 4];
    /// vec.truncate(2);
    /// assert_eq!(vec, vec![1, 2]);
    /// ```
    #[unstable = "waiting on failure semantics"]
    pub fn truncate(&mut self, len: uint) {
        unsafe {
            // drop any extra elements
            while len < self.len {
                // decrement len before the read(), so a failure on Drop doesn't
                // re-drop the just-failed value.
                self.len -= 1;
                ptr::read(self.as_slice().unsafe_get(self.len));
            }
        }
    }

    /// Returns a mutable slice of the elements of `self`.
    ///
    /// # Example
    ///
    /// ```
    /// fn foo(slice: &mut [int]) {}
    ///
    /// let mut vec = vec![1i, 2];
    /// foo(vec.as_mut_slice());
    /// ```
    #[inline]
    #[stable]
    pub fn as_mut_slice<'a>(&'a mut self) -> &'a mut [T] {
        unsafe {
            mem::transmute(RawSlice {
                data: self.as_mut_ptr() as *const T,
                len: self.len,
            })
        }
    }

    /// Deprecated: use `into_iter`.
    #[deprecated = "use into_iter"]
    pub fn move_iter(self) -> MoveItems<T> {
        self.into_iter()
    }

    /// Creates a consuming iterator, that is, one that moves each
    /// value out of the vector (from start to end). The vector cannot
    /// be used after calling this.
    ///
    /// # Example
    ///
    /// ```
    /// let v = vec!["a".to_string(), "b".to_string()];
    /// for s in v.into_iter() {
    ///     // s has type String, not &String
    ///     println!("{}", s);
    /// }
    /// ```
    #[inline]
    pub fn into_iter(self) -> MoveItems<T> {
        unsafe {
            let iter = mem::transmute(self.as_slice().iter());
            let ptr = self.ptr;
            let cap = self.cap;
            mem::forget(self);
            MoveItems { allocation: ptr, cap: cap, iter: iter }
        }
    }

    /// Sets the length of a vector.
    ///
    /// This will explicitly set the size of the vector, without actually
    /// modifying its buffers, so it is up to the caller to ensure that the
    /// vector is actually the specified size.
    ///
    /// # Example
    ///
    /// ```
    /// let mut v = vec![1u, 2, 3, 4];
    /// unsafe {
    ///     v.set_len(1);
    /// }
    /// ```
    #[inline]
    #[stable]
    pub unsafe fn set_len(&mut self, len: uint) {
        self.len = len;
    }

    /// Returns a reference to the value at index `index`.
    ///
    /// # Failure
    ///
    /// Fails if `index` is out of bounds
    ///
    /// # Example
    ///
    /// ```
    /// #![allow(deprecated)]
    ///
    /// let vec = vec![1i, 2, 3];
    /// assert!(vec.get(1) == &2);
    /// ```
    #[deprecated="prefer using indexing, e.g., vec[0]"]
    #[inline]
    pub fn get<'a>(&'a self, index: uint) -> &'a T {
        &self.as_slice()[index]
    }

    /// Returns a mutable reference to the value at index `index`.
    ///
    /// # Failure
    ///
    /// Fails if `index` is out of bounds
    ///
    /// # Example
    ///
    /// ```
    /// let mut vec = vec![1i, 2, 3];
    /// *vec.get_mut(1) = 4;
    /// assert_eq!(vec, vec![1i, 4, 3]);
    /// ```
    #[inline]
    #[unstable = "this is likely to be moved to actual indexing"]
    pub fn get_mut<'a>(&'a mut self, index: uint) -> &'a mut T {
        &mut self.as_mut_slice()[index]
    }

    /// Returns an iterator over references to the elements of the vector in
    /// order.
    ///
    /// # Example
    ///
    /// ```
    /// let vec = vec![1i, 2, 3];
    /// for num in vec.iter() {
    ///     println!("{}", *num);
    /// }
    /// ```
    #[inline]
    pub fn iter<'a>(&'a self) -> Items<'a,T> {
        self.as_slice().iter()
    }

    /// Deprecated: use `iter_mut`.
    #[deprecated = "use iter_mut"]
    pub fn mut_iter<'a>(&'a mut self) -> MutItems<'a,T> {
        self.iter_mut()
    }

    /// Returns an iterator over mutable references to the elements of the
    /// vector in order.
    ///
    /// # Example
    ///
    /// ```
    /// let mut vec = vec![1i, 2, 3];
    /// for num in vec.iter_mut() {
    ///     *num = 0;
    /// }
    /// ```
    #[inline]
    pub fn iter_mut<'a>(&'a mut self) -> MutItems<'a,T> {
        self.as_mut_slice().iter_mut()
    }

    /// Sorts the vector, in place, using `compare` to compare elements.
    ///
    /// This sort is `O(n log n)` worst-case and stable, but allocates
    /// approximately `2 * n`, where `n` is the length of `self`.
    ///
    /// # Example
    ///
    /// ```
    /// let mut v = vec![5i, 4, 1, 3, 2];
    /// v.sort_by(|a, b| a.cmp(b));
    /// assert_eq!(v, vec![1i, 2, 3, 4, 5]);
    ///
    /// // reverse sorting
    /// v.sort_by(|a, b| b.cmp(a));
    /// assert_eq!(v, vec![5i, 4, 3, 2, 1]);
    /// ```
    #[inline]
    pub fn sort_by(&mut self, compare: |&T, &T| -> Ordering) {
        self.as_mut_slice().sort_by(compare)
    }

    /// Returns a slice of self spanning the interval [`start`, `end`).
    ///
    /// # Failure
    ///
    /// Fails when the slice (or part of it) is outside the bounds of self, or when
    /// `start` > `end`.
    ///
    /// # Example
    ///
    /// ```
    /// let vec = vec![1i, 2, 3, 4];
    /// assert!(vec.slice(0, 2) == [1, 2]);
    /// ```
    #[inline]
    pub fn slice<'a>(&'a self, start: uint, end: uint) -> &'a [T] {
        self.as_slice().slice(start, end)
    }

    /// Returns a slice containing all but the first element of the vector.
    ///
    /// # Failure
    ///
    /// Fails when the vector is empty.
    ///
    /// # Example
    ///
    /// ```
    /// let vec = vec![1i, 2, 3];
    /// assert!(vec.tail() == [2, 3]);
    /// ```
    #[inline]
    pub fn tail<'a>(&'a self) -> &'a [T] {
        self.as_slice().tail()
    }

    /// Returns all but the first `n' elements of a vector.
    ///
    /// # Failure
    ///
    /// Fails when there are fewer than `n` elements in the vector.
    ///
    /// # Example
    ///
    /// ```
    /// #![allow(deprecated)]
    /// let vec = vec![1i, 2, 3, 4];
    /// assert!(vec.tailn(2) == [3, 4]);
    /// ```
    #[inline]
    #[deprecated = "use slice_from"]
    pub fn tailn<'a>(&'a self, n: uint) -> &'a [T] {
        self.as_slice().slice_from(n)
    }

    /// Returns a reference to the last element of a vector, or `None` if it is
    /// empty.
    ///
    /// # Example
    ///
    /// ```
    /// let vec = vec![1i, 2, 3];
    /// assert!(vec.last() == Some(&3));
    /// ```
    #[inline]
    pub fn last<'a>(&'a self) -> Option<&'a T> {
        self.as_slice().last()
    }

    /// Deprecated: use `last_mut`.
    #[deprecated = "use last_mut"]
    pub fn mut_last<'a>(&'a mut self) -> Option<&'a mut T> {
        self.last_mut()
    }

    /// Returns a mutable reference to the last element of a vector, or `None`
    /// if it is empty.
    ///
    /// # Example
    ///
    /// ```
    /// let mut vec = vec![1i, 2, 3];
    /// *vec.last_mut().unwrap() = 4;
    /// assert_eq!(vec, vec![1i, 2, 4]);
    /// ```
    #[inline]
    pub fn last_mut<'a>(&'a mut self) -> Option<&'a mut T> {
        self.as_mut_slice().last_mut()
    }

    /// Removes an element from anywhere in the vector and return it, replacing
    /// it with the last element. This does not preserve ordering, but is O(1).
    ///
    /// Returns `None` if `index` is out of bounds.
    ///
    /// # Example
    /// ```
    /// let mut v = vec!["foo".to_string(), "bar".to_string(),
    ///                  "baz".to_string(), "qux".to_string()];
    ///
    /// assert_eq!(v.swap_remove(1), Some("bar".to_string()));
    /// assert_eq!(v, vec!["foo".to_string(), "qux".to_string(), "baz".to_string()]);
    ///
    /// assert_eq!(v.swap_remove(0), Some("foo".to_string()));
    /// assert_eq!(v, vec!["baz".to_string(), "qux".to_string()]);
    ///
    /// assert_eq!(v.swap_remove(2), None);
    /// ```
    #[inline]
    #[unstable = "the naming of this function may be altered"]
    pub fn swap_remove(&mut self, index: uint) -> Option<T> {
        let length = self.len();
        if length > 0 && index < length - 1 {
            self.as_mut_slice().swap(index, length - 1);
        } else if index >= length {
            return None
        }
        self.pop()
    }

    /// Prepends an element to the vector.
    ///
    /// # Warning
    ///
    /// This is an O(n) operation as it requires copying every element in the
    /// vector.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut vec = vec![1i, 2, 3];
    /// vec.unshift(4);
    /// assert_eq!(vec, vec![4, 1, 2, 3]);
    /// ```
    #[inline]
    #[deprecated = "use insert(0, ...)"]
    pub fn unshift(&mut self, element: T) {
        self.insert(0, element)
    }

    /// Removes the first element from a vector and returns it, or `None` if
    /// the vector is empty.
    ///
    /// # Warning
    ///
    /// This is an O(n) operation as it requires copying every element in the
    /// vector.
    ///
    /// # Example
    ///
    /// ```
    /// #![allow(deprecated)]
    /// let mut vec = vec![1i, 2, 3];
    /// assert!(vec.shift() == Some(1));
    /// assert_eq!(vec, vec![2, 3]);
    /// ```
    #[inline]
    #[deprecated = "use remove(0)"]
    pub fn shift(&mut self) -> Option<T> {
        self.remove(0)
    }

    /// Inserts an element at position `index` within the vector, shifting all
    /// elements after position `i` one position to the right.
    ///
    /// # Failure
    ///
    /// Fails if `index` is not between `0` and the vector's length (both
    /// bounds inclusive).
    ///
    /// # Example
    ///
    /// ```
    /// let mut vec = vec![1i, 2, 3];
    /// vec.insert(1, 4);
    /// assert_eq!(vec, vec![1, 4, 2, 3]);
    /// vec.insert(4, 5);
    /// assert_eq!(vec, vec![1, 4, 2, 3, 5]);
    /// ```
    #[unstable = "failure semantics need settling"]
    pub fn insert(&mut self, index: uint, element: T) {
        let len = self.len();
        assert!(index <= len);
        // space for the new element
        self.reserve(len + 1);

        unsafe { // infallible
            // The spot to put the new value
            {
                let p = self.as_mut_ptr().offset(index as int);
                // Shift everything over to make space. (Duplicating the
                // `index`th element into two consecutive places.)
                ptr::copy_memory(p.offset(1), &*p, len - index);
                // Write it in, overwriting the first copy of the `index`th
                // element.
                ptr::write(&mut *p, element);
            }
            self.set_len(len + 1);
        }
    }

    /// Removes and returns the element at position `index` within the vector,
    /// shifting all elements after position `index` one position to the left.
    /// Returns `None` if `i` is out of bounds.
    ///
    /// # Example
    ///
    /// ```
    /// let mut v = vec![1i, 2, 3];
    /// assert_eq!(v.remove(1), Some(2));
    /// assert_eq!(v, vec![1, 3]);
    ///
    /// assert_eq!(v.remove(4), None);
    /// // v is unchanged:
    /// assert_eq!(v, vec![1, 3]);
    /// ```
    #[unstable = "failure semantics need settling"]
    pub fn remove(&mut self, index: uint) -> Option<T> {
        let len = self.len();
        if index < len {
            unsafe { // infallible
                let ret;
                {
                    // the place we are taking from.
                    let ptr = self.as_mut_ptr().offset(index as int);
                    // copy it out, unsafely having a copy of the value on
                    // the stack and in the vector at the same time.
                    ret = Some(ptr::read(ptr as *const T));

                    // Shift everything down to fill in that spot.
                    ptr::copy_memory(ptr, &*ptr.offset(1), len - index - 1);
                }
                self.set_len(len - 1);
                ret
            }
        } else {
            None
        }
    }

    /// Takes ownership of the vector `other`, moving all elements into
    /// the current vector. This does not copy any elements, and it is
    /// illegal to use the `other` vector after calling this method
    /// (because it is moved here).
    ///
    /// # Example
    ///
    /// ```
    /// # #![allow(deprecated)]
    /// let mut vec = vec![box 1i];
    /// vec.push_all_move(vec![box 2, box 3, box 4]);
    /// assert_eq!(vec, vec![box 1, box 2, box 3, box 4]);
    /// ```
    #[inline]
    #[deprecated = "use .extend(other.into_iter())"]
    pub fn push_all_move(&mut self, other: Vec<T>) {
        self.extend(other.into_iter());
    }

    /// Deprecated: use `slice_mut`.
    #[deprecated = "use slice_mut"]
    pub fn mut_slice<'a>(&'a mut self, start: uint, end: uint)
                         -> &'a mut [T] {
        self.slice_mut(start, end)
    }

    /// Returns a mutable slice of `self` between `start` and `end`.
    ///
    /// # Failure
    ///
    /// Fails when `start` or `end` point outside the bounds of `self`, or when
    /// `start` > `end`.
    ///
    /// # Example
    ///
    /// ```
    /// let mut vec = vec![1i, 2, 3, 4];
    /// assert!(vec.slice_mut(0, 2) == [1, 2]);
    /// ```
    #[inline]
    pub fn slice_mut<'a>(&'a mut self, start: uint, end: uint)
                         -> &'a mut [T] {
        self.as_mut_slice().slice_mut(start, end)
    }

    /// Deprecated: use "slice_from_mut".
    #[deprecated = "use slice_from_mut"]
    pub fn mut_slice_from<'a>(&'a mut self, start: uint) -> &'a mut [T] {
        self.slice_from_mut(start)
    }

    /// Returns a mutable slice of `self` from `start` to the end of the `Vec`.
    ///
    /// # Failure
    ///
    /// Fails when `start` points outside the bounds of self.
    ///
    /// # Example
    ///
    /// ```
    /// let mut vec = vec![1i, 2, 3, 4];
    /// assert!(vec.slice_from_mut(2) == [3, 4]);
    /// ```
    #[inline]
    pub fn slice_from_mut<'a>(&'a mut self, start: uint) -> &'a mut [T] {
        self.as_mut_slice().slice_from_mut(start)
    }

    /// Deprecated: use `slice_to_mut`.
    #[deprecated = "use slice_to_mut"]
    pub fn mut_slice_to<'a>(&'a mut self, end: uint) -> &'a mut [T] {
        self.slice_to_mut(end)
    }

    /// Returns a mutable slice of `self` from the start of the `Vec` to `end`.
    ///
    /// # Failure
    ///
    /// Fails when `end` points outside the bounds of self.
    ///
    /// # Example
    ///
    /// ```
    /// let mut vec = vec![1i, 2, 3, 4];
    /// assert!(vec.slice_to_mut(2) == [1, 2]);
    /// ```
    #[inline]
    pub fn slice_to_mut<'a>(&'a mut self, end: uint) -> &'a mut [T] {
        self.as_mut_slice().slice_to_mut(end)
    }

    /// Deprecated: use `split_at_mut`.
    #[deprecated = "use split_at_mut"]
    pub fn mut_split_at<'a>(&'a mut self, mid: uint) -> (&'a mut [T], &'a mut [T]) {
        self.split_at_mut(mid)
    }

    /// Returns a pair of mutable slices that divides the `Vec` at an index.
    ///
    /// The first will contain all indices from `[0, mid)` (excluding
    /// the index `mid` itself) and the second will contain all
    /// indices from `[mid, len)` (excluding the index `len` itself).
    ///
    /// # Failure
    ///
    /// Fails if `mid > len`.
    ///
    /// # Example
    ///
    /// ```
    /// let mut vec = vec![1i, 2, 3, 4, 5, 6];
    ///
    /// // scoped to restrict the lifetime of the borrows
    /// {
    ///    let (left, right) = vec.split_at_mut(0);
    ///    assert!(left == &mut []);
    ///    assert!(right == &mut [1, 2, 3, 4, 5, 6]);
    /// }
    ///
    /// {
    ///     let (left, right) = vec.split_at_mut(2);
    ///     assert!(left == &mut [1, 2]);
    ///     assert!(right == &mut [3, 4, 5, 6]);
    /// }
    ///
    /// {
    ///     let (left, right) = vec.split_at_mut(6);
    ///     assert!(left == &mut [1, 2, 3, 4, 5, 6]);
    ///     assert!(right == &mut []);
    /// }
    /// ```
    #[inline]
    pub fn split_at_mut<'a>(&'a mut self, mid: uint) -> (&'a mut [T], &'a mut [T]) {
        self.as_mut_slice().split_at_mut(mid)
    }

    /// Reverses the order of elements in a vector, in place.
    ///
    /// # Example
    ///
    /// ```
    /// let mut v = vec![1i, 2, 3];
    /// v.reverse();
    /// assert_eq!(v, vec![3i, 2, 1]);
    /// ```
    #[inline]
    pub fn reverse(&mut self) {
        self.as_mut_slice().reverse()
    }

    /// Returns a slice of `self` from `start` to the end of the vec.
    ///
    /// # Failure
    ///
    /// Fails when `start` points outside the bounds of self.
    ///
    /// # Example
    ///
    /// ```
    /// let vec = vec![1i, 2, 3];
    /// assert!(vec.slice_from(1) == [2, 3]);
    /// ```
    #[inline]
    pub fn slice_from<'a>(&'a self, start: uint) -> &'a [T] {
        self.as_slice().slice_from(start)
    }

    /// Returns a slice of self from the start of the vec to `end`.
    ///
    /// # Failure
    ///
    /// Fails when `end` points outside the bounds of self.
    ///
    /// # Example
    ///
    /// ```
    /// let vec = vec![1i, 2, 3, 4];
    /// assert!(vec.slice_to(2) == [1, 2]);
    /// ```
    #[inline]
    pub fn slice_to<'a>(&'a self, end: uint) -> &'a [T] {
        self.as_slice().slice_to(end)
    }

    /// Returns a slice containing all but the last element of the vector.
    ///
    /// # Failure
    ///
    /// Fails if the vector is empty
    ///
    /// # Example
    ///
    /// ```
    /// let vec = vec![1i, 2, 3];
    /// assert!(vec.init() == [1, 2]);
    /// ```
    #[inline]
    pub fn init<'a>(&'a self) -> &'a [T] {
        self.slice(0, self.len() - 1)
    }


    /// Returns an unsafe pointer to the vector's buffer.
    ///
    /// The caller must ensure that the vector outlives the pointer this
    /// function returns, or else it will end up pointing to garbage.
    ///
    /// Modifying the vector may cause its buffer to be reallocated, which
    /// would also make any pointers to it invalid.
    ///
    /// # Example
    ///
    /// ```
    /// let v = vec![1i, 2, 3];
    /// let p = v.as_ptr();
    /// unsafe {
    ///     // Examine each element manually
    ///     assert_eq!(*p, 1i);
    ///     assert_eq!(*p.offset(1), 2i);
    ///     assert_eq!(*p.offset(2), 3i);
    /// }
    /// ```
    #[inline]
    pub fn as_ptr(&self) -> *const T {
        self.ptr as *const T
    }

    /// Returns a mutable unsafe pointer to the vector's buffer.
    ///
    /// The caller must ensure that the vector outlives the pointer this
    /// function returns, or else it will end up pointing to garbage.
    ///
    /// Modifying the vector may cause its buffer to be reallocated, which
    /// would also make any pointers to it invalid.
    ///
    /// # Example
    ///
    /// ```
    /// use std::ptr;
    ///
    /// let mut v = vec![1i, 2, 3];
    /// let p = v.as_mut_ptr();
    /// unsafe {
    ///     ptr::write(p, 9i);
    ///     ptr::write(p.offset(2), 5i);
    /// }
    /// assert_eq!(v, vec![9i, 2, 5]);
    /// ```
    #[inline]
    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.ptr
    }

    /// Retains only the elements specified by the predicate.
    ///
    /// In other words, remove all elements `e` such that `f(&e)` returns false.
    /// This method operates in place and preserves the order of the retained elements.
    ///
    /// # Example
    ///
    /// ```
    /// let mut vec = vec![1i, 2, 3, 4];
    /// vec.retain(|x| x%2 == 0);
    /// assert_eq!(vec, vec![2, 4]);
    /// ```
    #[unstable = "the closure argument may become an unboxed closure"]
    pub fn retain(&mut self, f: |&T| -> bool) {
        let len = self.len();
        let mut del = 0u;
        {
            let v = self.as_mut_slice();

            for i in range(0u, len) {
                if !f(&v[i]) {
                    del += 1;
                } else if del > 0 {
                    v.swap(i-del, i);
                }
            }
        }
        if del > 0 {
            self.truncate(len - del);
        }
    }

    /// Expands a vector in place, initializing the new elements to the result of a function.
    ///
    /// The vector is grown by `n` elements. The i-th new element are initialized to the value
    /// returned by `f(i)` where `i` is in the range [0, n).
    ///
    /// # Example
    ///
    /// ```
    /// let mut vec = vec![0u, 1];
    /// vec.grow_fn(3, |i| i);
    /// assert_eq!(vec, vec![0, 1, 0, 1, 2]);
    /// ```
    #[unstable = "this function may be renamed or change to unboxed closures"]
    pub fn grow_fn(&mut self, n: uint, f: |uint| -> T) {
        self.reserve_additional(n);
        for i in range(0u, n) {
            self.push(f(i));
        }
    }
}

impl<T:Ord> Vec<T> {
    /// Sorts the vector in place.
    ///
    /// This sort is `O(n log n)` worst-case and stable, but allocates
    /// approximately `2 * n`, where `n` is the length of `self`.
    ///
    /// # Example
    ///
    /// ```
    /// let mut vec = vec![3i, 1, 2];
    /// vec.sort();
    /// assert_eq!(vec, vec![1, 2, 3]);
    /// ```
    pub fn sort(&mut self) {
        self.as_mut_slice().sort()
    }
}

#[experimental = "waiting on Mutable stability"]
impl<T> Mutable for Vec<T> {
    #[inline]
    #[stable]
    fn clear(&mut self) {
        self.truncate(0)
    }
}

impl<T: PartialEq> Vec<T> {
    /// Returns true if a vector contains an element equal to the given value.
    ///
    /// # Example
    ///
    /// ```
    /// let vec = vec![1i, 2, 3];
    /// assert!(vec.contains(&1));
    /// ```
    #[inline]
    pub fn contains(&self, x: &T) -> bool {
        self.as_slice().contains(x)
    }

    /// Removes consecutive repeated elements in the vector.
    ///
    /// If the vector is sorted, this removes all duplicates.
    ///
    /// # Example
    ///
    /// ```
    /// let mut vec = vec![1i, 2, 2, 3, 2];
    /// vec.dedup();
    /// assert_eq!(vec, vec![1i, 2, 3, 2]);
    /// ```
    #[unstable = "this function may be renamed"]
    pub fn dedup(&mut self) {
        unsafe {
            // Although we have a mutable reference to `self`, we cannot make
            // *arbitrary* changes. The `PartialEq` comparisons could fail, so we
            // must ensure that the vector is in a valid state at all time.
            //
            // The way that we handle this is by using swaps; we iterate
            // over all the elements, swapping as we go so that at the end
            // the elements we wish to keep are in the front, and those we
            // wish to reject are at the back. We can then truncate the
            // vector. This operation is still O(n).
            //
            // Example: We start in this state, where `r` represents "next
            // read" and `w` represents "next_write`.
            //
            //           r
            //     +---+---+---+---+---+---+
            //     | 0 | 1 | 1 | 2 | 3 | 3 |
            //     +---+---+---+---+---+---+
            //           w
            //
            // Comparing self[r] against self[w-1], this is not a duplicate, so
            // we swap self[r] and self[w] (no effect as r==w) and then increment both
            // r and w, leaving us with:
            //
            //               r
            //     +---+---+---+---+---+---+
            //     | 0 | 1 | 1 | 2 | 3 | 3 |
            //     +---+---+---+---+---+---+
            //               w
            //
            // Comparing self[r] against self[w-1], this value is a duplicate,
            // so we increment `r` but leave everything else unchanged:
            //
            //                   r
            //     +---+---+---+---+---+---+
            //     | 0 | 1 | 1 | 2 | 3 | 3 |
            //     +---+---+---+---+---+---+
            //               w
            //
            // Comparing self[r] against self[w-1], this is not a duplicate,
            // so swap self[r] and self[w] and advance r and w:
            //
            //                       r
            //     +---+---+---+---+---+---+
            //     | 0 | 1 | 2 | 1 | 3 | 3 |
            //     +---+---+---+---+---+---+
            //                   w
            //
            // Not a duplicate, repeat:
            //
            //                           r
            //     +---+---+---+---+---+---+
            //     | 0 | 1 | 2 | 3 | 1 | 3 |
            //     +---+---+---+---+---+---+
            //                       w
            //
            // Duplicate, advance r. End of vec. Truncate to w.

            let ln = self.len();
            if ln < 1 { return; }

            // Avoid bounds checks by using unsafe pointers.
            let p = self.as_mut_slice().as_mut_ptr();
            let mut r = 1;
            let mut w = 1;

            while r < ln {
                let p_r = p.offset(r as int);
                let p_wm1 = p.offset((w - 1) as int);
                if *p_r != *p_wm1 {
                    if r != w {
                        let p_w = p_wm1.offset(1);
                        mem::swap(&mut *p_r, &mut *p_w);
                    }
                    w += 1;
                }
                r += 1;
            }

            self.truncate(w);
        }
    }
}

impl<T> Slice<T> for Vec<T> {
    /// Returns a slice into `self`.
    ///
    /// # Example
    ///
    /// ```
    /// fn foo(slice: &[int]) {}
    ///
    /// let vec = vec![1i, 2];
    /// foo(vec.as_slice());
    /// ```
    #[inline]
    #[stable]
    fn as_slice<'a>(&'a self) -> &'a [T] {
        unsafe { mem::transmute(RawSlice { data: self.as_ptr(), len: self.len }) }
    }
}

impl<T: Clone, V: Slice<T>> Add<V, Vec<T>> for Vec<T> {
    #[inline]
    fn add(&self, rhs: &V) -> Vec<T> {
        let mut res = Vec::with_capacity(self.len() + rhs.as_slice().len());
        res.push_all(self.as_slice());
        res.push_all(rhs.as_slice());
        res
    }
}

#[unsafe_destructor]
impl<T> Drop for Vec<T> {
    fn drop(&mut self) {
        // This is (and should always remain) a no-op if the fields are
        // zeroed (when moving out, because of #[unsafe_no_drop_flag]).
        if self.cap != 0 {
            unsafe {
                for x in self.as_mut_slice().iter() {
                    ptr::read(x);
                }
                dealloc(self.ptr, self.cap)
            }
        }
    }
}

#[stable]
impl<T> Default for Vec<T> {
    fn default() -> Vec<T> {
        Vec::new()
    }
}

#[experimental = "waiting on Show stability"]
impl<T:fmt::Show> fmt::Show for Vec<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.as_slice().fmt(f)
    }
}

#[experimental = "waiting on MutableSeq stability"]
impl<T> MutableSeq<T> for Vec<T> {
    /// Appends an element to the back of a collection.
    ///
    /// # Failure
    ///
    /// Fails if the number of elements in the vector overflows a `uint`.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut vec = vec!(1i, 2);
    /// vec.push(3);
    /// assert_eq!(vec, vec!(1, 2, 3));
    /// ```
    #[inline]
    #[stable]
    fn push(&mut self, value: T) {
        if mem::size_of::<T>() == 0 {
            // zero-size types consume no memory, so we can't rely on the address space running out
            self.len = self.len.checked_add(&1).expect("length overflow");
            unsafe { mem::forget(value); }
            return
        }
        if self.len == self.cap {
            let old_size = self.cap * mem::size_of::<T>();
            let size = max(old_size, 2 * mem::size_of::<T>()) * 2;
            if old_size > size { fail!("capacity overflow") }
            unsafe {
                self.ptr = alloc_or_realloc(self.ptr, size,
                                            self.cap * mem::size_of::<T>());
            }
            self.cap = max(self.cap, 2) * 2;
        }

        unsafe {
            let end = (self.ptr as *const T).offset(self.len as int) as *mut T;
            ptr::write(&mut *end, value);
            self.len += 1;
        }
    }

    #[inline]
    #[stable]
    fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            None
        } else {
            unsafe {
                self.len -= 1;
                Some(ptr::read(self.as_slice().unsafe_get(self.len())))
            }
        }
    }

}

/// An iterator that moves out of a vector.
pub struct MoveItems<T> {
    allocation: *mut T, // the block of memory allocated for the vector
    cap: uint, // the capacity of the vector
    iter: Items<'static, T>
}

impl<T> MoveItems<T> {
    #[inline]
    /// Drops all items that have not yet been moved and returns the empty vector.
    pub fn unwrap(mut self) -> Vec<T> {
        unsafe {
            for _x in self { }
            let MoveItems { allocation, cap, iter: _iter } = self;
            mem::forget(self);
            Vec { ptr: allocation, cap: cap, len: 0 }
        }
    }
}

impl<T> Iterator<T> for MoveItems<T> {
    #[inline]
    fn next<'a>(&'a mut self) -> Option<T> {
        unsafe {
            // Unsafely transmute from Items<'static, T> to Items<'a,
            // T> because otherwise the type checker requires that T
            // be bounded by 'static.
            let iter: &mut Items<'a, T> = mem::transmute(&mut self.iter);
            iter.next().map(|x| ptr::read(x))
        }
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        self.iter.size_hint()
    }
}

impl<T> DoubleEndedIterator<T> for MoveItems<T> {
    #[inline]
    fn next_back<'a>(&'a mut self) -> Option<T> {
        unsafe {
            // Unsafely transmute from Items<'static, T> to Items<'a,
            // T> because otherwise the type checker requires that T
            // be bounded by 'static.
            let iter: &mut Items<'a, T> = mem::transmute(&mut self.iter);
            iter.next_back().map(|x| ptr::read(x))
        }
    }
}

impl<T> ExactSize<T> for MoveItems<T> {}

#[unsafe_destructor]
impl<T> Drop for MoveItems<T> {
    fn drop(&mut self) {
        // destroy the remaining elements
        if self.cap != 0 {
            for _x in *self {}
            unsafe {
                dealloc(self.allocation, self.cap);
            }
        }
    }
}

/// Converts an iterator of pairs into a pair of vectors.
///
/// Returns a tuple containing two vectors where the i-th element of the first
/// vector contains the first element of the i-th tuple of the input iterator,
/// and the i-th element of the second vector contains the second element
/// of the i-th tuple of the input iterator.
#[unstable = "this functionality may become more generic over time"]
pub fn unzip<T, U, V: Iterator<(T, U)>>(mut iter: V) -> (Vec<T>, Vec<U>) {
    let (lo, _) = iter.size_hint();
    let mut ts = Vec::with_capacity(lo);
    let mut us = Vec::with_capacity(lo);
    for (t, u) in iter {
        ts.push(t);
        us.push(u);
    }
    (ts, us)
}

/// Unsafe vector operations.
#[unstable]
pub mod raw {
    use super::Vec;
    use core::ptr;

    /// Constructs a vector from an unsafe pointer to a buffer.
    ///
    /// The elements of the buffer are copied into the vector without cloning,
    /// as if `ptr::read()` were called on them.
    #[inline]
    #[unstable]
    pub unsafe fn from_buf<T>(ptr: *const T, elts: uint) -> Vec<T> {
        let mut dst = Vec::with_capacity(elts);
        dst.set_len(elts);
        ptr::copy_nonoverlapping_memory(dst.as_mut_ptr(), ptr, elts);
        dst
    }
}

/// An owned, partially type-converted vector.
///
/// This struct takes two type parameters `T` and `U` which must be of the
/// same, non-zero size having the same minimal alignment.
///
/// No allocations are performed by usage, only a deallocation happens in the
/// destructor which should only run when unwinding.
///
/// It can be used to convert a vector of `T`s into a vector of `U`s, by
/// converting the individual elements one-by-one.
///
/// You may call the `push` method as often as you get a `Some(t)` from `pop`.
/// After pushing the same number of `U`s as you got `T`s, you can `unwrap` the
/// vector.
///
/// # Example
///
/// ```ignore
/// let pv = PartialVec::from_vec(vec![0u32, 1]);
/// assert_eq!(pv.pop(), Some(0));
/// assert_eq!(pv.pop(), Some(1));
/// assert_eq!(pv.pop(), None);
/// pv.push(2u32);
/// pv.push(3);
/// assert_eq!(pv.into_vec().as_slice(), &[2, 3]);
/// ```
//
// Upheld invariants:
//
// (a) `vec` isn't modified except when the `PartialVec` goes out of scope, the
//     only thing it is used for is keeping the memory which the `PartialVec`
//     uses for the inplace conversion.
//
// (b) `start_u` points to the start of the vector.
//
// (c) `end_u` points to one element beyond the vector.
//
// (d) `start_u` <= `end_u` <= `start_t` <= `end_t`.
//
// (e) From `start_u` (incl.) to `end_u` (excl.) there are sequential instances
//     of type `U`.
//
// (f) From `start_t` (incl.) to `end_t` (excl.) there are sequential instances
//     of type `T`.
//
// (g) The size of `T` and `U` is equal and non-zero.
//
// (h) The `min_align_of` of `T` and `U` is equal.

struct PartialVec<T,U> {
    vec: Vec<T>,

    start_u: *mut U,
    end_u: *mut U,
    start_t: *mut T,
    end_t: *mut T,
}

impl<T,U> PartialVec<T,U> {
    /// Creates a `PartialVec` from a `Vec`.
    ///
    /// # Failure
    ///
    /// Fails if `T` and `U` have differing sizes, are zero-sized or have
    /// differing minimal alignments.
    fn from_vec(mut vec: Vec<T>) -> PartialVec<T,U> {
        // FIXME: Assert statically that the types `T` and `U` have the same
        // size.
        //
        // These asserts make sure (g) and (h) are satisfied.
        assert!(mem::size_of::<T>() != 0);
        assert!(mem::size_of::<U>() != 0);
        assert!(mem::size_of::<T>() == mem::size_of::<U>());
        assert!(mem::min_align_of::<T>() == mem::min_align_of::<U>());

        let start = vec.as_mut_ptr();

        // This `as int` cast is safe, because the size of the elements of the
        // vector is not 0, and:
        //
        // 1) If the size of the elements in the vector is 1, the `int` may
        //    overflow, but it has the correct bit pattern so that the
        //    `.offset()` function will work.
        //
        //    Example:
        //        Address space 0x0-0xF.
        //        `u8` array at: 0x1.
        //        Size of `u8` array: 0x8.
        //        Calculated `offset`: -0x8.
        //        After `array.offset(offset)`: 0x9.
        //        (0x1 + 0x8 = 0x1 - 0x8)
        //
        // 2) If the size of the elements in the vector is >1, the `uint` ->
        //    `int` conversion can't overflow.
        let offset = vec.len() as int;

        let start_u = start as *mut U;
        let end_u = start as *mut U;
        let start_t = start;

        // This points inside the vector, as the vector has length `offset`.
        let end_t = unsafe { start_t.offset(offset) };

        // (b) is satisfied, `start_u` points to the start of `vec`.
        //
        // (c) is also satisfied, `end_t` points to the end of `vec`.
        //
        // `start_u == end_u == start_t <= end_t`, so also `start_u <= end_u <=
        // start_t <= end_t`, thus (b).
        //
        // As `start_u == end_u`, it is represented correctly that there are no
        // instances of `U` in `vec`, thus (e) is satisfied.
        //
        // At start, there are only elements of type `T` in `vec`, so (f) is
        // satisfied, as `start_t` points to the start of `vec` and `end_t` to
        // the end of it.

        PartialVec {
            // (a) is satisfied, `vec` isn't modified in the function.
            vec: vec,
            start_u: start_u,
            end_u: end_u,
            start_t: start_t,
            end_t: end_t,
        }
    }

    /// Pops a `T` from the `PartialVec`.
    ///
    /// Removes the next `T` from the vector and returns it as `Some(T)`, or
    /// `None` if there are none left.
    fn pop(&mut self) -> Option<T> {
        // The `if` ensures that there are more `T`s in `vec`.
        if self.start_t < self.end_t {
            let result;
            unsafe {
                // (f) is satisfied before, so in this if branch there actually
                // is a `T` at `start_t`.  After shifting the pointer by one,
                // (f) is again satisfied.
                result = ptr::read(self.start_t as *const T);
                self.start_t = self.start_t.offset(1);
            }
            Some(result)
        } else {
            None
        }
    }

    /// Pushes a new `U` to the `PartialVec`.
    ///
    /// # Failure
    ///
    /// Fails if not enough `T`s were popped to have enough space for the new
    /// `U`.
    fn push(&mut self, value: U) {
        // The assert assures that still `end_u <= start_t` (d) after
        // the function.
        assert!(self.end_u as *const () < self.start_t as *const (),
            "writing more elements to PartialVec than reading from it")
        unsafe {
            // (e) is satisfied before, and after writing one `U`
            // to `end_u` and shifting it by one, it's again
            // satisfied.
            ptr::write(self.end_u, value);
            self.end_u = self.end_u.offset(1);
        }
    }

    /// Unwraps the new `Vec` of `U`s after having pushed enough `U`s and
    /// popped all `T`s.
    ///
    /// # Failure
    ///
    /// Fails if not all `T`s were popped, also fails if not the same amount of
    /// `U`s was pushed before calling `unwrap`.
    fn into_vec(mut self) -> Vec<U> {
        // If `self.end_u == self.end_t`, we know from (e) that there are no
        // more `T`s in `vec`, we also know that the whole length of `vec` is
        // now used by `U`s, thus we can just interpret `vec` as a vector of
        // `U` safely.

        assert!(self.end_u as *const () == self.end_t as *const (),
            "trying to unwrap a PartialVec before completing the writes to it");

        // Extract `vec` and prevent the destructor of `PartialVec` from
        // running. Note that none of the function calls can fail, thus no
        // resources can be leaked (as the `vec` member of `PartialVec` is the
        // only one which holds allocations -- and it is returned from this
        // function.
        unsafe {
            let vec_len = self.vec.len();
            let vec_cap = self.vec.capacity();
            let vec_ptr = self.vec.as_mut_ptr() as *mut U;
            mem::forget(self);
            Vec::from_raw_parts(vec_len, vec_cap, vec_ptr)
        }
    }
}

#[unsafe_destructor]
impl<T,U> Drop for PartialVec<T,U> {
    fn drop(&mut self) {
        unsafe {
            // As per (a) `vec` hasn't been modified until now. As it has a
            // length currently, this would run destructors of `T`s which might
            // not be there. So at first, set `vec`s length to `0`. This must
            // be done at first to remain memory-safe as the destructors of `U`
            // or `T` might cause unwinding where `vec`s destructor would be
            // executed.
            self.vec.set_len(0);

            // As per (e) and (f) we have instances of `U`s and `T`s in `vec`.
            // Destruct them.
            while self.start_u < self.end_u {
                let _ = ptr::read(self.start_u as *const U); // Run a `U` destructor.
                self.start_u = self.start_u.offset(1);
            }
            while self.start_t < self.end_t {
                let _ = ptr::read(self.start_t as *const T); // Run a `T` destructor.
                self.start_t = self.start_t.offset(1);
            }
            // After this destructor ran, the destructor of `vec` will run,
            // deallocating the underlying memory.
        }
    }
}

impl<T> Vec<T> {
    /// Converts a `Vec<T>` to a `Vec<U>` where `T` and `U` have the same
    /// non-zero size and the same minimal alignment.
    ///
    /// # Failure
    ///
    /// Fails if `T` and `U` have differing sizes, are zero-sized or have
    /// differing minimal alignments.
    ///
    /// # Example
    ///
    /// ```
    /// let v = vec![0u, 1, 2];
    /// let w = v.map_in_place(|i| i + 3);
    /// assert_eq!(w.as_slice(), [3, 4, 5].as_slice());
    ///
    /// #[deriving(PartialEq, Show)]
    /// struct Newtype(u8);
    /// let bytes = vec![0x11, 0x22];
    /// let newtyped_bytes = bytes.map_in_place(|x| Newtype(x));
    /// assert_eq!(newtyped_bytes.as_slice(), [Newtype(0x11), Newtype(0x22)].as_slice());
    /// ```
    pub fn map_in_place<U>(self, f: |T| -> U) -> Vec<U> {
        let mut pv = PartialVec::from_vec(self);
        loop {
            let maybe_t = pv.pop();
            match maybe_t {
                Some(t) => pv.push(f(t)),
                None => return pv.into_vec(),
            };
        }
    }
}


#[cfg(test)]
mod tests {
    extern crate test;

    use std::prelude::*;
    use std::mem::size_of;
    use test::Bencher;
    use super::{unzip, raw, Vec};

    use MutableSeq;

    #[test]
    fn test_small_vec_struct() {
        assert!(size_of::<Vec<u8>>() == size_of::<uint>() * 3);
    }

    #[test]
    fn test_double_drop() {
        struct TwoVec<T> {
            x: Vec<T>,
            y: Vec<T>
        }

        struct DropCounter<'a> {
            count: &'a mut int
        }

        #[unsafe_destructor]
        impl<'a> Drop for DropCounter<'a> {
            fn drop(&mut self) {
                *self.count += 1;
            }
        }

        let (mut count_x, mut count_y) = (0, 0);
        {
            let mut tv = TwoVec {
                x: Vec::new(),
                y: Vec::new()
            };
            tv.x.push(DropCounter {count: &mut count_x});
            tv.y.push(DropCounter {count: &mut count_y});

            // If Vec had a drop flag, here is where it would be zeroed.
            // Instead, it should rely on its internal state to prevent
            // doing anything significant when dropped multiple times.
            drop(tv.x);

            // Here tv goes out of scope, tv.y should be dropped, but not tv.x.
        }

        assert_eq!(count_x, 1);
        assert_eq!(count_y, 1);
    }

    #[test]
    fn test_reserve_additional() {
        let mut v = Vec::new();
        assert_eq!(v.capacity(), 0);

        v.reserve_additional(2);
        assert!(v.capacity() >= 2);

        for i in range(0i, 16) {
            v.push(i);
        }

        assert!(v.capacity() >= 16);
        v.reserve_additional(16);
        assert!(v.capacity() >= 32);

        v.push(16);

        v.reserve_additional(16);
        assert!(v.capacity() >= 33)
    }

    #[test]
    fn test_extend() {
        let mut v = Vec::new();
        let mut w = Vec::new();

        v.extend(range(0i, 3));
        for i in range(0i, 3) { w.push(i) }

        assert_eq!(v, w);

        v.extend(range(3i, 10));
        for i in range(3i, 10) { w.push(i) }

        assert_eq!(v, w);
    }

    #[test]
    fn test_mut_slice_from() {
        let mut values = Vec::from_slice([1u8,2,3,4,5]);
        {
            let slice = values.slice_from_mut(2);
            assert!(slice == [3, 4, 5]);
            for p in slice.iter_mut() {
                *p += 2;
            }
        }

        assert!(values.as_slice() == [1, 2, 5, 6, 7]);
    }

    #[test]
    fn test_mut_slice_to() {
        let mut values = Vec::from_slice([1u8,2,3,4,5]);
        {
            let slice = values.slice_to_mut(2);
            assert!(slice == [1, 2]);
            for p in slice.iter_mut() {
                *p += 1;
            }
        }

        assert!(values.as_slice() == [2, 3, 3, 4, 5]);
    }

    #[test]
    fn test_mut_split_at() {
        let mut values = Vec::from_slice([1u8,2,3,4,5]);
        {
            let (left, right) = values.split_at_mut(2);
            assert!(left.slice(0, left.len()) == [1, 2]);
            for p in left.iter_mut() {
                *p += 1;
            }

            assert!(right.slice(0, right.len()) == [3, 4, 5]);
            for p in right.iter_mut() {
                *p += 2;
            }
        }

        assert!(values == Vec::from_slice([2u8, 3, 5, 6, 7]));
    }

    #[test]
    fn test_clone() {
        let v: Vec<int> = vec!();
        let w = vec!(1i, 2, 3);

        assert_eq!(v, v.clone());

        let z = w.clone();
        assert_eq!(w, z);
        // they should be disjoint in memory.
        assert!(w.as_ptr() != z.as_ptr())
    }

    #[test]
    fn test_clone_from() {
        let mut v = vec!();
        let three = vec!(box 1i, box 2, box 3);
        let two = vec!(box 4i, box 5);
        // zero, long
        v.clone_from(&three);
        assert_eq!(v, three);

        // equal
        v.clone_from(&three);
        assert_eq!(v, three);

        // long, short
        v.clone_from(&two);
        assert_eq!(v, two);

        // short, long
        v.clone_from(&three);
        assert_eq!(v, three)
    }

    #[test]
    fn test_grow_fn() {
        let mut v = Vec::from_slice([0u, 1]);
        v.grow_fn(3, |i| i);
        assert!(v == Vec::from_slice([0u, 1, 0, 1, 2]));
    }

    #[test]
    fn test_retain() {
        let mut vec = Vec::from_slice([1u, 2, 3, 4]);
        vec.retain(|x| x%2 == 0);
        assert!(vec == Vec::from_slice([2u, 4]));
    }

    #[test]
    fn zero_sized_values() {
        let mut v = Vec::new();
        assert_eq!(v.len(), 0);
        v.push(());
        assert_eq!(v.len(), 1);
        v.push(());
        assert_eq!(v.len(), 2);
        assert_eq!(v.pop(), Some(()));
        assert_eq!(v.pop(), Some(()));
        assert_eq!(v.pop(), None);

        assert_eq!(v.iter().count(), 0);
        v.push(());
        assert_eq!(v.iter().count(), 1);
        v.push(());
        assert_eq!(v.iter().count(), 2);

        for &() in v.iter() {}

        assert_eq!(v.iter_mut().count(), 2);
        v.push(());
        assert_eq!(v.iter_mut().count(), 3);
        v.push(());
        assert_eq!(v.iter_mut().count(), 4);

        for &() in v.iter_mut() {}
        unsafe { v.set_len(0); }
        assert_eq!(v.iter_mut().count(), 0);
    }

    #[test]
    fn test_partition() {
        assert_eq!(vec![].partition(|x: &int| *x < 3), (vec![], vec![]));
        assert_eq!(vec![1i, 2, 3].partition(|x: &int| *x < 4), (vec![1, 2, 3], vec![]));
        assert_eq!(vec![1i, 2, 3].partition(|x: &int| *x < 2), (vec![1], vec![2, 3]));
        assert_eq!(vec![1i, 2, 3].partition(|x: &int| *x < 0), (vec![], vec![1, 2, 3]));
    }

    #[test]
    fn test_partitioned() {
        assert_eq!(vec![].partitioned(|x: &int| *x < 3), (vec![], vec![]))
        assert_eq!(vec![1i, 2, 3].partitioned(|x: &int| *x < 4), (vec![1, 2, 3], vec![]));
        assert_eq!(vec![1i, 2, 3].partitioned(|x: &int| *x < 2), (vec![1], vec![2, 3]));
        assert_eq!(vec![1i, 2, 3].partitioned(|x: &int| *x < 0), (vec![], vec![1, 2, 3]));
    }

    #[test]
    fn test_zip_unzip() {
        let z1 = vec![(1i, 4i), (2, 5), (3, 6)];

        let (left, right) = unzip(z1.iter().map(|&x| x));

        let (left, right) = (left.as_slice(), right.as_slice());
        assert_eq!((1, 4), (left[0], right[0]));
        assert_eq!((2, 5), (left[1], right[1]));
        assert_eq!((3, 6), (left[2], right[2]));
    }

    #[test]
    fn test_unsafe_ptrs() {
        unsafe {
            // Test on-stack copy-from-buf.
            let a = [1i, 2, 3];
            let ptr = a.as_ptr();
            let b = raw::from_buf(ptr, 3u);
            assert_eq!(b, vec![1, 2, 3]);

            // Test on-heap copy-from-buf.
            let c = vec![1i, 2, 3, 4, 5];
            let ptr = c.as_ptr();
            let d = raw::from_buf(ptr, 5u);
            assert_eq!(d, vec![1, 2, 3, 4, 5]);
        }
    }

    #[test]
    fn test_vec_truncate_drop() {
        static mut drops: uint = 0;
        struct Elem(int);
        impl Drop for Elem {
            fn drop(&mut self) {
                unsafe { drops += 1; }
            }
        }

        let mut v = vec![Elem(1), Elem(2), Elem(3), Elem(4), Elem(5)];
        assert_eq!(unsafe { drops }, 0);
        v.truncate(3);
        assert_eq!(unsafe { drops }, 2);
        v.truncate(0);
        assert_eq!(unsafe { drops }, 5);
    }

    #[test]
    #[should_fail]
    fn test_vec_truncate_fail() {
        struct BadElem(int);
        impl Drop for BadElem {
            fn drop(&mut self) {
                let BadElem(ref mut x) = *self;
                if *x == 0xbadbeef {
                    fail!("BadElem failure: 0xbadbeef")
                }
            }
        }

        let mut v = vec![BadElem(1), BadElem(2), BadElem(0xbadbeef), BadElem(4)];
        v.truncate(0);
    }

    #[test]
    fn test_index() {
        let vec = vec!(1i, 2, 3);
        assert!(vec[1] == 2);
    }

    #[test]
    #[should_fail]
    fn test_index_out_of_bounds() {
        let vec = vec!(1i, 2, 3);
        let _ = vec[3];
    }

    // NOTE uncomment after snapshot
    /*
    #[test]
    #[should_fail]
    fn test_slice_out_of_bounds_1() {
        let x: Vec<int> = vec![1, 2, 3, 4, 5];
        x[-1..];
    }

    #[test]
    #[should_fail]
    fn test_slice_out_of_bounds_2() {
        let x: Vec<int> = vec![1, 2, 3, 4, 5];
        x[..6];
    }

    #[test]
    #[should_fail]
    fn test_slice_out_of_bounds_3() {
        let x: Vec<int> = vec![1, 2, 3, 4, 5];
        x[-1..4];
    }

    #[test]
    #[should_fail]
    fn test_slice_out_of_bounds_4() {
        let x: Vec<int> = vec![1, 2, 3, 4, 5];
        x[1..6];
    }

    #[test]
    #[should_fail]
    fn test_slice_out_of_bounds_5() {
        let x: Vec<int> = vec![1, 2, 3, 4, 5];
        x[3..2];
    }
    */

    #[test]
    fn test_swap_remove_empty() {
        let mut vec: Vec<uint> = vec!();
        assert_eq!(vec.swap_remove(0), None);
    }

    #[test]
    fn test_move_iter_unwrap() {
        let mut vec: Vec<uint> = Vec::with_capacity(7);
        vec.push(1);
        vec.push(2);
        let ptr = vec.as_ptr();
        vec = vec.into_iter().unwrap();
        assert_eq!(vec.as_ptr(), ptr);
        assert_eq!(vec.capacity(), 7);
        assert_eq!(vec.len(), 0);
    }

    #[test]
    #[should_fail]
    fn test_map_inp_lace_incompatible_types_fail() {
        let v = vec![0u, 1, 2];
        v.map_in_place(|_| ());
    }

    #[test]
    fn test_map_in_place() {
        let v = vec![0u, 1, 2];
        assert_eq!(v.map_in_place(|i: uint| i as int - 1).as_slice(), [-1i, 0, 1].as_slice());
    }

    #[bench]
    fn bench_new(b: &mut Bencher) {
        b.iter(|| {
            let v: Vec<uint> = Vec::new();
            assert_eq!(v.len(), 0);
            assert_eq!(v.capacity(), 0);
        })
    }

    fn do_bench_with_capacity(b: &mut Bencher, src_len: uint) {
        b.bytes = src_len as u64;

        b.iter(|| {
            let v: Vec<uint> = Vec::with_capacity(src_len);
            assert_eq!(v.len(), 0);
            assert_eq!(v.capacity(), src_len);
        })
    }

    #[bench]
    fn bench_with_capacity_0000(b: &mut Bencher) {
        do_bench_with_capacity(b, 0)
    }

    #[bench]
    fn bench_with_capacity_0010(b: &mut Bencher) {
        do_bench_with_capacity(b, 10)
    }

    #[bench]
    fn bench_with_capacity_0100(b: &mut Bencher) {
        do_bench_with_capacity(b, 100)
    }

    #[bench]
    fn bench_with_capacity_1000(b: &mut Bencher) {
        do_bench_with_capacity(b, 1000)
    }

    fn do_bench_from_fn(b: &mut Bencher, src_len: uint) {
        b.bytes = src_len as u64;

        b.iter(|| {
            let dst = Vec::from_fn(src_len, |i| i);
            assert_eq!(dst.len(), src_len);
            assert!(dst.iter().enumerate().all(|(i, x)| i == *x));
        })
    }

    #[bench]
    fn bench_from_fn_0000(b: &mut Bencher) {
        do_bench_from_fn(b, 0)
    }

    #[bench]
    fn bench_from_fn_0010(b: &mut Bencher) {
        do_bench_from_fn(b, 10)
    }

    #[bench]
    fn bench_from_fn_0100(b: &mut Bencher) {
        do_bench_from_fn(b, 100)
    }

    #[bench]
    fn bench_from_fn_1000(b: &mut Bencher) {
        do_bench_from_fn(b, 1000)
    }

    fn do_bench_from_elem(b: &mut Bencher, src_len: uint) {
        b.bytes = src_len as u64;

        b.iter(|| {
            let dst: Vec<uint> = Vec::from_elem(src_len, 5);
            assert_eq!(dst.len(), src_len);
            assert!(dst.iter().all(|x| *x == 5));
        })
    }

    #[bench]
    fn bench_from_elem_0000(b: &mut Bencher) {
        do_bench_from_elem(b, 0)
    }

    #[bench]
    fn bench_from_elem_0010(b: &mut Bencher) {
        do_bench_from_elem(b, 10)
    }

    #[bench]
    fn bench_from_elem_0100(b: &mut Bencher) {
        do_bench_from_elem(b, 100)
    }

    #[bench]
    fn bench_from_elem_1000(b: &mut Bencher) {
        do_bench_from_elem(b, 1000)
    }

    fn do_bench_from_slice(b: &mut Bencher, src_len: uint) {
        let src: Vec<uint> = FromIterator::from_iter(range(0, src_len));

        b.bytes = src_len as u64;

        b.iter(|| {
            let dst = Vec::from_slice(src.clone().as_slice());
            assert_eq!(dst.len(), src_len);
            assert!(dst.iter().enumerate().all(|(i, x)| i == *x));
        });
    }

    #[bench]
    fn bench_from_slice_0000(b: &mut Bencher) {
        do_bench_from_slice(b, 0)
    }

    #[bench]
    fn bench_from_slice_0010(b: &mut Bencher) {
        do_bench_from_slice(b, 10)
    }

    #[bench]
    fn bench_from_slice_0100(b: &mut Bencher) {
        do_bench_from_slice(b, 100)
    }

    #[bench]
    fn bench_from_slice_1000(b: &mut Bencher) {
        do_bench_from_slice(b, 1000)
    }

    fn do_bench_from_iter(b: &mut Bencher, src_len: uint) {
        let src: Vec<uint> = FromIterator::from_iter(range(0, src_len));

        b.bytes = src_len as u64;

        b.iter(|| {
            let dst: Vec<uint> = FromIterator::from_iter(src.clone().into_iter());
            assert_eq!(dst.len(), src_len);
            assert!(dst.iter().enumerate().all(|(i, x)| i == *x));
        });
    }

    #[bench]
    fn bench_from_iter_0000(b: &mut Bencher) {
        do_bench_from_iter(b, 0)
    }

    #[bench]
    fn bench_from_iter_0010(b: &mut Bencher) {
        do_bench_from_iter(b, 10)
    }

    #[bench]
    fn bench_from_iter_0100(b: &mut Bencher) {
        do_bench_from_iter(b, 100)
    }

    #[bench]
    fn bench_from_iter_1000(b: &mut Bencher) {
        do_bench_from_iter(b, 1000)
    }

    fn do_bench_extend(b: &mut Bencher, dst_len: uint, src_len: uint) {
        let dst: Vec<uint> = FromIterator::from_iter(range(0, dst_len));
        let src: Vec<uint> = FromIterator::from_iter(range(dst_len, dst_len + src_len));

        b.bytes = src_len as u64;

        b.iter(|| {
            let mut dst = dst.clone();
            dst.extend(src.clone().into_iter());
            assert_eq!(dst.len(), dst_len + src_len);
            assert!(dst.iter().enumerate().all(|(i, x)| i == *x));
        });
    }

    #[bench]
    fn bench_extend_0000_0000(b: &mut Bencher) {
        do_bench_extend(b, 0, 0)
    }

    #[bench]
    fn bench_extend_0000_0010(b: &mut Bencher) {
        do_bench_extend(b, 0, 10)
    }

    #[bench]
    fn bench_extend_0000_0100(b: &mut Bencher) {
        do_bench_extend(b, 0, 100)
    }

    #[bench]
    fn bench_extend_0000_1000(b: &mut Bencher) {
        do_bench_extend(b, 0, 1000)
    }

    #[bench]
    fn bench_extend_0010_0010(b: &mut Bencher) {
        do_bench_extend(b, 10, 10)
    }

    #[bench]
    fn bench_extend_0100_0100(b: &mut Bencher) {
        do_bench_extend(b, 100, 100)
    }

    #[bench]
    fn bench_extend_1000_1000(b: &mut Bencher) {
        do_bench_extend(b, 1000, 1000)
    }

    fn do_bench_push_all(b: &mut Bencher, dst_len: uint, src_len: uint) {
        let dst: Vec<uint> = FromIterator::from_iter(range(0, dst_len));
        let src: Vec<uint> = FromIterator::from_iter(range(dst_len, dst_len + src_len));

        b.bytes = src_len as u64;

        b.iter(|| {
            let mut dst = dst.clone();
            dst.push_all(src.as_slice());
            assert_eq!(dst.len(), dst_len + src_len);
            assert!(dst.iter().enumerate().all(|(i, x)| i == *x));
        });
    }

    #[bench]
    fn bench_push_all_0000_0000(b: &mut Bencher) {
        do_bench_push_all(b, 0, 0)
    }

    #[bench]
    fn bench_push_all_0000_0010(b: &mut Bencher) {
        do_bench_push_all(b, 0, 10)
    }

    #[bench]
    fn bench_push_all_0000_0100(b: &mut Bencher) {
        do_bench_push_all(b, 0, 100)
    }

    #[bench]
    fn bench_push_all_0000_1000(b: &mut Bencher) {
        do_bench_push_all(b, 0, 1000)
    }

    #[bench]
    fn bench_push_all_0010_0010(b: &mut Bencher) {
        do_bench_push_all(b, 10, 10)
    }

    #[bench]
    fn bench_push_all_0100_0100(b: &mut Bencher) {
        do_bench_push_all(b, 100, 100)
    }

    #[bench]
    fn bench_push_all_1000_1000(b: &mut Bencher) {
        do_bench_push_all(b, 1000, 1000)
    }

    fn do_bench_push_all_move(b: &mut Bencher, dst_len: uint, src_len: uint) {
        let dst: Vec<uint> = FromIterator::from_iter(range(0u, dst_len));
        let src: Vec<uint> = FromIterator::from_iter(range(dst_len, dst_len + src_len));

        b.bytes = src_len as u64;

        b.iter(|| {
            let mut dst = dst.clone();
            dst.push_all_move(src.clone());
            assert_eq!(dst.len(), dst_len + src_len);
            assert!(dst.iter().enumerate().all(|(i, x)| i == *x));
        });
    }

    #[bench]
    fn bench_push_all_move_0000_0000(b: &mut Bencher) {
        do_bench_push_all_move(b, 0, 0)
    }

    #[bench]
    fn bench_push_all_move_0000_0010(b: &mut Bencher) {
        do_bench_push_all_move(b, 0, 10)
    }

    #[bench]
    fn bench_push_all_move_0000_0100(b: &mut Bencher) {
        do_bench_push_all_move(b, 0, 100)
    }

    #[bench]
    fn bench_push_all_move_0000_1000(b: &mut Bencher) {
        do_bench_push_all_move(b, 0, 1000)
    }

    #[bench]
    fn bench_push_all_move_0010_0010(b: &mut Bencher) {
        do_bench_push_all_move(b, 10, 10)
    }

    #[bench]
    fn bench_push_all_move_0100_0100(b: &mut Bencher) {
        do_bench_push_all_move(b, 100, 100)
    }

    #[bench]
    fn bench_push_all_move_1000_1000(b: &mut Bencher) {
        do_bench_push_all_move(b, 1000, 1000)
    }

    fn do_bench_clone(b: &mut Bencher, src_len: uint) {
        let src: Vec<uint> = FromIterator::from_iter(range(0, src_len));

        b.bytes = src_len as u64;

        b.iter(|| {
            let dst = src.clone();
            assert_eq!(dst.len(), src_len);
            assert!(dst.iter().enumerate().all(|(i, x)| i == *x));
        });
    }

    #[bench]
    fn bench_clone_0000(b: &mut Bencher) {
        do_bench_clone(b, 0)
    }

    #[bench]
    fn bench_clone_0010(b: &mut Bencher) {
        do_bench_clone(b, 10)
    }

    #[bench]
    fn bench_clone_0100(b: &mut Bencher) {
        do_bench_clone(b, 100)
    }

    #[bench]
    fn bench_clone_1000(b: &mut Bencher) {
        do_bench_clone(b, 1000)
    }

    fn do_bench_clone_from(b: &mut Bencher, times: uint, dst_len: uint, src_len: uint) {
        let dst: Vec<uint> = FromIterator::from_iter(range(0, src_len));
        let src: Vec<uint> = FromIterator::from_iter(range(dst_len, dst_len + src_len));

        b.bytes = (times * src_len) as u64;

        b.iter(|| {
            let mut dst = dst.clone();

            for _ in range(0, times) {
                dst.clone_from(&src);

                assert_eq!(dst.len(), src_len);
                assert!(dst.iter().enumerate().all(|(i, x)| dst_len + i == *x));
            }
        });
    }

    #[bench]
    fn bench_clone_from_01_0000_0000(b: &mut Bencher) {
        do_bench_clone_from(b, 1, 0, 0)
    }

    #[bench]
    fn bench_clone_from_01_0000_0010(b: &mut Bencher) {
        do_bench_clone_from(b, 1, 0, 10)
    }

    #[bench]
    fn bench_clone_from_01_0000_0100(b: &mut Bencher) {
        do_bench_clone_from(b, 1, 0, 100)
    }

    #[bench]
    fn bench_clone_from_01_0000_1000(b: &mut Bencher) {
        do_bench_clone_from(b, 1, 0, 1000)
    }

    #[bench]
    fn bench_clone_from_01_0010_0010(b: &mut Bencher) {
        do_bench_clone_from(b, 1, 10, 10)
    }

    #[bench]
    fn bench_clone_from_01_0100_0100(b: &mut Bencher) {
        do_bench_clone_from(b, 1, 100, 100)
    }

    #[bench]
    fn bench_clone_from_01_1000_1000(b: &mut Bencher) {
        do_bench_clone_from(b, 1, 1000, 1000)
    }

    #[bench]
    fn bench_clone_from_01_0010_0100(b: &mut Bencher) {
        do_bench_clone_from(b, 1, 10, 100)
    }

    #[bench]
    fn bench_clone_from_01_0100_1000(b: &mut Bencher) {
        do_bench_clone_from(b, 1, 100, 1000)
    }

    #[bench]
    fn bench_clone_from_01_0010_0000(b: &mut Bencher) {
        do_bench_clone_from(b, 1, 10, 0)
    }

    #[bench]
    fn bench_clone_from_01_0100_0010(b: &mut Bencher) {
        do_bench_clone_from(b, 1, 100, 10)
    }

    #[bench]
    fn bench_clone_from_01_1000_0100(b: &mut Bencher) {
        do_bench_clone_from(b, 1, 1000, 100)
    }

    #[bench]
    fn bench_clone_from_10_0000_0000(b: &mut Bencher) {
        do_bench_clone_from(b, 10, 0, 0)
    }

    #[bench]
    fn bench_clone_from_10_0000_0010(b: &mut Bencher) {
        do_bench_clone_from(b, 10, 0, 10)
    }

    #[bench]
    fn bench_clone_from_10_0000_0100(b: &mut Bencher) {
        do_bench_clone_from(b, 10, 0, 100)
    }

    #[bench]
    fn bench_clone_from_10_0000_1000(b: &mut Bencher) {
        do_bench_clone_from(b, 10, 0, 1000)
    }

    #[bench]
    fn bench_clone_from_10_0010_0010(b: &mut Bencher) {
        do_bench_clone_from(b, 10, 10, 10)
    }

    #[bench]
    fn bench_clone_from_10_0100_0100(b: &mut Bencher) {
        do_bench_clone_from(b, 10, 100, 100)
    }

    #[bench]
    fn bench_clone_from_10_1000_1000(b: &mut Bencher) {
        do_bench_clone_from(b, 10, 1000, 1000)
    }

    #[bench]
    fn bench_clone_from_10_0010_0100(b: &mut Bencher) {
        do_bench_clone_from(b, 10, 10, 100)
    }

    #[bench]
    fn bench_clone_from_10_0100_1000(b: &mut Bencher) {
        do_bench_clone_from(b, 10, 100, 1000)
    }

    #[bench]
    fn bench_clone_from_10_0010_0000(b: &mut Bencher) {
        do_bench_clone_from(b, 10, 10, 0)
    }

    #[bench]
    fn bench_clone_from_10_0100_0010(b: &mut Bencher) {
        do_bench_clone_from(b, 10, 100, 10)
    }

    #[bench]
    fn bench_clone_from_10_1000_0100(b: &mut Bencher) {
        do_bench_clone_from(b, 10, 1000, 100)
    }
}
