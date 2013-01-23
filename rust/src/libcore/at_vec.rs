// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Managed vectors

// NB: transitionary, de-mode-ing.
#[forbid(deprecated_mode)];
#[forbid(deprecated_pattern)];

use cast::transmute;
use kinds::Copy;
use iter;
use libc;
use option::Option;
use ptr::addr_of;
use sys;
use uint;
use vec;

/// Code for dealing with @-vectors. This is pretty incomplete, and
/// contains a bunch of duplication from the code for ~-vectors.

#[abi = "cdecl"]
pub extern mod rustrt {
    #[legacy_exports];
    unsafe fn vec_reserve_shared_actual(++t: *sys::TypeDesc,
                                        ++v: **vec::raw::VecRepr,
                                        ++n: libc::size_t);
}

#[abi = "rust-intrinsic"]
pub extern mod rusti {
    #[legacy_exports];
    fn move_val_init<T>(dst: &mut T, -src: T);
}

/// Returns the number of elements the vector can hold without reallocating
#[inline(always)]
pub pure fn capacity<T>(v: @[const T]) -> uint {
    unsafe {
        let repr: **raw::VecRepr =
            ::cast::reinterpret_cast(&addr_of(&v));
        (**repr).unboxed.alloc / sys::size_of::<T>()
    }
}

/**
 * Builds a vector by calling a provided function with an argument
 * function that pushes an element to the back of a vector.
 * This version takes an initial size for the vector.
 *
 * # Arguments
 *
 * * size - An initial size of the vector to reserve
 * * builder - A function that will construct the vector. It recieves
 *             as an argument a function that will push an element
 *             onto the vector being constructed.
 */
#[inline(always)]
pub pure fn build_sized<A>(size: uint,
                           builder: &fn(push: pure fn(v: A))) -> @[A] {
    let mut vec: @[const A] = @[];
    unsafe { raw::reserve(&mut vec, size); }
    builder(|+x| unsafe { raw::push(&mut vec, x) });
    return unsafe { transmute(vec) };
}

/**
 * Builds a vector by calling a provided function with an argument
 * function that pushes an element to the back of a vector.
 *
 * # Arguments
 *
 * * builder - A function that will construct the vector. It recieves
 *             as an argument a function that will push an element
 *             onto the vector being constructed.
 */
#[inline(always)]
pub pure fn build<A>(builder: &fn(push: pure fn(v: A))) -> @[A] {
    build_sized(4, builder)
}

/**
 * Builds a vector by calling a provided function with an argument
 * function that pushes an element to the back of a vector.
 * This version takes an initial size for the vector.
 *
 * # Arguments
 *
 * * size - An option, maybe containing initial size of the vector to reserve
 * * builder - A function that will construct the vector. It recieves
 *             as an argument a function that will push an element
 *             onto the vector being constructed.
 */
#[inline(always)]
pub pure fn build_sized_opt<A>(size: Option<uint>,
                               builder: &fn(push: pure fn(v: A))) -> @[A] {
    build_sized(size.get_or_default(4), builder)
}

// Appending
#[inline(always)]
pub pure fn append<T: Copy>(lhs: @[T], rhs: &[const T]) -> @[T] {
    do build_sized(lhs.len() + rhs.len()) |push| {
        for vec::each(lhs) |x| { push(*x); }
        for uint::range(0, rhs.len()) |i| { push(rhs[i]); }
    }
}


/// Apply a function to each element of a vector and return the results
pub pure fn map<T, U>(v: &[T], f: &fn(x: &T) -> U) -> @[U] {
    do build_sized(v.len()) |push| {
        for vec::each(v) |elem| {
            push(f(elem));
        }
    }
}

/**
 * Creates and initializes an immutable vector.
 *
 * Creates an immutable vector of size `n_elts` and initializes the elements
 * to the value returned by the function `op`.
 */
pub pure fn from_fn<T>(n_elts: uint, op: iter::InitOp<T>) -> @[T] {
    do build_sized(n_elts) |push| {
        let mut i: uint = 0u;
        while i < n_elts { push(op(i)); i += 1u; }
    }
}

/**
 * Creates and initializes an immutable vector.
 *
 * Creates an immutable vector of size `n_elts` and initializes the elements
 * to the value `t`.
 */
pub pure fn from_elem<T: Copy>(n_elts: uint, t: T) -> @[T] {
    do build_sized(n_elts) |push| {
        let mut i: uint = 0u;
        while i < n_elts { push(copy t); i += 1u; }
    }
}

/**
 * Creates and initializes an immutable managed vector by moving all the
 * elements from an owned vector.
 */
pub fn from_owned<T>(v: ~[T]) -> @[T] {
    let mut av = @[];
    unsafe {
        raw::reserve(&mut av, v.len());
        do vec::consume(v) |_i, x| {
            raw::push(&mut av, x);
        }
        transmute(av)
    }
}

/**
 * Creates and initializes an immutable managed vector by copying all the
 * elements of a slice.
 */
pub fn from_slice<T:Copy>(v: &[T]) -> @[T] {
    from_fn(v.len(), |i| v[i])
}

#[cfg(notest)]
pub mod traits {
    use at_vec::append;
    use kinds::Copy;
    use ops::Add;

    pub impl<T: Copy> @[T] : Add<&[const T],@[T]> {
        #[inline(always)]
        pure fn add(&self, rhs: & &self/[const T]) -> @[T] {
            append(*self, (*rhs))
        }
    }
}

#[cfg(test)]
pub mod traits {}

pub mod raw {
    use at_vec::{capacity, rusti, rustrt};
    use cast::transmute;
    use libc;
    use ptr::addr_of;
    use ptr;
    use sys;
    use uint;
    use vec;

    pub type VecRepr = vec::raw::VecRepr;
    pub type SliceRepr = vec::raw::SliceRepr;

    /**
     * Sets the length of a vector
     *
     * This will explicitly set the size of the vector, without actually
     * modifing its buffers, so it is up to the caller to ensure that
     * the vector is actually the specified size.
     */
    #[inline(always)]
    pub unsafe fn set_len<T>(v: @[const T], new_len: uint) {
        let repr: **VecRepr = ::cast::reinterpret_cast(&addr_of(&v));
        (**repr).unboxed.fill = new_len * sys::size_of::<T>();
    }

    #[inline(always)]
    pub unsafe fn push<T>(v: &mut @[const T], initval: T) {
        let repr: **VecRepr = ::cast::reinterpret_cast(&v);
        let fill = (**repr).unboxed.fill;
        if (**repr).unboxed.alloc > fill {
            push_fast(v, initval);
        }
        else {
            push_slow(v, initval);
        }
    }

    #[inline(always)] // really pretty please
    pub unsafe fn push_fast<T>(v: &mut @[const T], initval: T) {
        let repr: **VecRepr = ::cast::reinterpret_cast(&v);
        let fill = (**repr).unboxed.fill;
        (**repr).unboxed.fill += sys::size_of::<T>();
        let p = addr_of(&((**repr).unboxed.data));
        let p = ptr::offset(p, fill) as *mut T;
        rusti::move_val_init(&mut(*p), move initval);
    }

    pub unsafe fn push_slow<T>(v: &mut @[const T], initval: T) {
        reserve_at_least(&mut *v, v.len() + 1u);
        push_fast(v, move initval);
    }

    /**
     * Reserves capacity for exactly `n` elements in the given vector.
     *
     * If the capacity for `v` is already equal to or greater than the
     * requested capacity, then no action is taken.
     *
     * # Arguments
     *
     * * v - A vector
     * * n - The number of elements to reserve space for
     */
    pub unsafe fn reserve<T>(v: &mut @[const T], n: uint) {
        // Only make the (slow) call into the runtime if we have to
        if capacity(*v) < n {
            let ptr: **VecRepr = transmute(v);
            rustrt::vec_reserve_shared_actual(sys::get_type_desc::<T>(),
                                              ptr, n as libc::size_t);
        }
    }

    /**
     * Reserves capacity for at least `n` elements in the given vector.
     *
     * This function will over-allocate in order to amortize the
     * allocation costs in scenarios where the caller may need to
     * repeatedly reserve additional space.
     *
     * If the capacity for `v` is already equal to or greater than the
     * requested capacity, then no action is taken.
     *
     * # Arguments
     *
     * * v - A vector
     * * n - The number of elements to reserve space for
     */
    pub unsafe fn reserve_at_least<T>(v: &mut @[const T], n: uint) {
        reserve(v, uint::next_power_of_two(n));
    }

}

#[test]
pub fn test() {
    // Some code that could use that, then:
    fn seq_range(lo: uint, hi: uint) -> @[uint] {
        do build |push| {
            for uint::range(lo, hi) |i| {
                push(i);
            }
        }
    }

    assert seq_range(10, 15) == @[10, 11, 12, 13, 14];
    assert from_fn(5, |x| x+1) == @[1, 2, 3, 4, 5];
    assert from_elem(5, 3.14) == @[3.14, 3.14, 3.14, 3.14, 3.14];
}

#[test]
pub fn append_test() {
    assert @[1,2,3] + @[4,5,6] == @[1,2,3,4,5,6];
}

#[test]
pub fn test_from_owned() {
    assert from_owned::<int>(~[]) == @[];
    assert from_owned(~[true]) == @[true];
    assert from_owned(~[1, 2, 3, 4, 5]) == @[1, 2, 3, 4, 5];
    assert from_owned(~[~"abc", ~"123"]) == @[~"abc", ~"123"];
    assert from_owned(~[~[42]]) == @[~[42]];
}

#[test]
pub fn test_from_slice() {
    assert from_slice::<int>([]) == @[];
    assert from_slice([true]) == @[true];
    assert from_slice([1, 2, 3, 4, 5]) == @[1, 2, 3, 4, 5];
    assert from_slice([@"abc", @"123"]) == @[@"abc", @"123"];
    assert from_slice([@[42]]) == @[@[42]];
}

