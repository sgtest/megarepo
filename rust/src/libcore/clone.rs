// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*! The `Clone` trait for types that cannot be 'implicitly copied'

In Rust, some simple types are "implicitly copyable" and when you
assign them or pass them as arguments, the receiver will get a copy,
leaving the original value in place. These types do not require
allocation to copy and do not have finalizers (i.e. they do not
contain owned boxes or implement `Drop`), so the compiler considers
them cheap and safe to copy. For other types copies must be made
explicitly, by convention implementing the `Clone` trait and calling
the `clone` method.

*/

#![unstable]

/// A common trait for cloning an object.
pub trait Clone {
    /// Returns a copy of the value.
    fn clone(&self) -> Self;

    /// Perform copy-assignment from `source`.
    ///
    /// `a.clone_from(&b)` is equivalent to `a = b.clone()` in functionality,
    /// but can be overridden to reuse the resources of `a` to avoid unnecessary
    /// allocations.
    #[inline(always)]
    #[experimental = "this function is mostly unused"]
    fn clone_from(&mut self, source: &Self) {
        *self = source.clone()
    }
}

impl<'a, T> Clone for &'a T {
    /// Return a shallow copy of the reference.
    #[inline]
    fn clone(&self) -> &'a T { *self }
}

impl<'a, T> Clone for &'a [T] {
    /// Return a shallow copy of the slice.
    #[inline]
    fn clone(&self) -> &'a [T] { *self }
}

impl<'a> Clone for &'a str {
    /// Return a shallow copy of the slice.
    #[inline]
    fn clone(&self) -> &'a str { *self }
}

macro_rules! clone_impl(
    ($t:ty) => {
        impl Clone for $t {
            /// Return a deep copy of the value.
            #[inline]
            fn clone(&self) -> $t { *self }
        }
    }
)

clone_impl!(int)
clone_impl!(i8)
clone_impl!(i16)
clone_impl!(i32)
clone_impl!(i64)

clone_impl!(uint)
clone_impl!(u8)
clone_impl!(u16)
clone_impl!(u32)
clone_impl!(u64)

clone_impl!(f32)
clone_impl!(f64)

clone_impl!(())
clone_impl!(bool)
clone_impl!(char)

macro_rules! extern_fn_clone(
    ($($A:ident),*) => (
        #[experimental = "this may not be sufficient for fns with region parameters"]
        impl<$($A,)* ReturnType> Clone for extern "Rust" fn($($A),*) -> ReturnType {
            /// Return a copy of a function pointer
            #[inline]
            fn clone(&self) -> extern "Rust" fn($($A),*) -> ReturnType { *self }
        }
    )
)

extern_fn_clone!()
extern_fn_clone!(A)
extern_fn_clone!(A, B)
extern_fn_clone!(A, B, C)
extern_fn_clone!(A, B, C, D)
extern_fn_clone!(A, B, C, D, E)
extern_fn_clone!(A, B, C, D, E, F)
extern_fn_clone!(A, B, C, D, E, F, G)
extern_fn_clone!(A, B, C, D, E, F, G, H)

