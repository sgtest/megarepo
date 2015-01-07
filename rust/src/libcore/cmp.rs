// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Defines the `PartialOrd` and `PartialEq` comparison traits.
//!
//! This module defines both `PartialOrd` and `PartialEq` traits which are used by the
//! compiler to implement comparison operators. Rust programs may implement
//!`PartialOrd` to overload the `<`, `<=`, `>`, and `>=` operators, and may implement
//! `PartialEq` to overload the `==` and `!=` operators.
//!
//! For example, to define a type with a customized definition for the PartialEq
//! operators, you could do the following:
//!
//! ```rust
//! use core::num::SignedInt;
//!
//! // Our type.
//! struct SketchyNum {
//!     num : int
//! }
//!
//! // Our implementation of `PartialEq` to support `==` and `!=`.
//! impl PartialEq for SketchyNum {
//!     // Our custom eq allows numbers which are near each other to be equal! :D
//!     fn eq(&self, other: &SketchyNum) -> bool {
//!         (self.num - other.num).abs() < 5
//!     }
//! }
//!
//! // Now these binary operators will work when applied!
//! assert!(SketchyNum {num: 37} == SketchyNum {num: 34});
//! assert!(SketchyNum {num: 25} != SketchyNum {num: 57});
//! ```

#![stable]

use self::Ordering::*;

use marker::Sized;
use option::Option::{self, Some, None};

/// Trait for equality comparisons which are [partial equivalence relations](
/// http://en.wikipedia.org/wiki/Partial_equivalence_relation).
///
/// This trait allows for partial equality, for types that do not have a full
/// equivalence relation. For example, in floating point numbers `NaN != NaN`,
/// so floating point types implement `PartialEq` but not `Eq`.
///
/// Formally, the equality must be (for all `a`, `b` and `c`):
///
/// - symmetric: `a == b` implies `b == a`; and
/// - transitive: `a == b` and `b == c` implies `a == c`.
///
/// Note that these requirements mean that the trait itself must be
/// implemented symmetrically and transitively: if `T: PartialEq<U>`
/// and `U: PartialEq<V>` then `U: PartialEq<T>` and `T:
/// PartialEq<V>`.
///
/// PartialEq only requires the `eq` method to be implemented; `ne` is defined
/// in terms of it by default. Any manual implementation of `ne` *must* respect
/// the rule that `eq` is a strict inverse of `ne`; that is, `!(a == b)` if and
/// only if `a != b`.
#[lang="eq"]
#[stable]
#[old_orphan_check]
pub trait PartialEq<Rhs: ?Sized = Self> {
    /// This method tests for `self` and `other` values to be equal, and is used by `==`.
    #[stable]
    fn eq(&self, other: &Rhs) -> bool;

    /// This method tests for `!=`.
    #[inline]
    #[stable]
    fn ne(&self, other: &Rhs) -> bool { !self.eq(other) }
}

/// Trait for equality comparisons which are [equivalence relations](
/// https://en.wikipedia.org/wiki/Equivalence_relation).
///
/// This means, that in addition to `a == b` and `a != b` being strict
/// inverses, the equality must be (for all `a`, `b` and `c`):
///
/// - reflexive: `a == a`;
/// - symmetric: `a == b` implies `b == a`; and
/// - transitive: `a == b` and `b == c` implies `a == c`.
#[stable]
pub trait Eq: PartialEq<Self> {
    // FIXME #13101: this method is used solely by #[deriving] to
    // assert that every component of a type implements #[deriving]
    // itself, the current deriving infrastructure means doing this
    // assertion without using a method on this trait is nearly
    // impossible.
    //
    // This should never be implemented by hand.
    #[doc(hidden)]
    #[inline(always)]
    fn assert_receiver_is_total_eq(&self) {}
}

/// An ordering is, e.g, a result of a comparison between two values.
#[derive(Clone, Copy, PartialEq, Show)]
#[stable]
pub enum Ordering {
    /// An ordering where a compared value is less [than another].
    #[stable]
    Less = -1i,
    /// An ordering where a compared value is equal [to another].
    #[stable]
    Equal = 0i,
    /// An ordering where a compared value is greater [than another].
    #[stable]
    Greater = 1i,
}

impl Ordering {
    /// Reverse the `Ordering`, so that `Less` becomes `Greater` and
    /// vice versa.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::cmp::Ordering::{Less, Equal, Greater};
    ///
    /// assert_eq!(Less.reverse(), Greater);
    /// assert_eq!(Equal.reverse(), Equal);
    /// assert_eq!(Greater.reverse(), Less);
    ///
    /// let mut data: &mut [_] = &mut [2u, 10, 5, 8];
    ///
    /// // sort the array from largest to smallest.
    /// data.sort_by(|a, b| a.cmp(b).reverse());
    ///
    /// let b: &mut [_] = &mut [10u, 8, 5, 2];
    /// assert!(data == b);
    /// ```
    #[inline]
    #[stable]
    pub fn reverse(self) -> Ordering {
        unsafe {
            // this compiles really nicely (to a single instruction);
            // an explicit match has a pile of branches and
            // comparisons.
            //
            // NB. it is safe because of the explicit discriminants
            // given above.
            ::mem::transmute::<_, Ordering>(-(self as i8))
        }
    }
}

/// Trait for types that form a [total order](
/// https://en.wikipedia.org/wiki/Total_order).
///
/// An order is a total order if it is (for all `a`, `b` and `c`):
///
/// - total and antisymmetric: exactly one of `a < b`, `a == b` or `a > b` is
///   true; and
/// - transitive, `a < b` and `b < c` implies `a < c`. The same must hold for
///   both `==` and `>`.
#[stable]
pub trait Ord: Eq + PartialOrd<Self> {
    /// This method returns an ordering between `self` and `other` values.
    ///
    /// By convention, `self.cmp(&other)` returns the ordering matching
    /// the expression `self <operator> other` if true.  For example:
    ///
    /// ```
    /// use std::cmp::Ordering::{Less, Equal, Greater};
    ///
    /// assert_eq!( 5u.cmp(&10), Less);     // because 5 < 10
    /// assert_eq!(10u.cmp(&5),  Greater);  // because 10 > 5
    /// assert_eq!( 5u.cmp(&5),  Equal);    // because 5 == 5
    /// ```
    #[stable]
    fn cmp(&self, other: &Self) -> Ordering;
}

#[stable]
impl Eq for Ordering {}

#[stable]
impl Ord for Ordering {
    #[inline]
    #[stable]
    fn cmp(&self, other: &Ordering) -> Ordering {
        (*self as int).cmp(&(*other as int))
    }
}

#[stable]
impl PartialOrd for Ordering {
    #[inline]
    #[stable]
    fn partial_cmp(&self, other: &Ordering) -> Option<Ordering> {
        (*self as int).partial_cmp(&(*other as int))
    }
}

/// Trait for values that can be compared for a sort-order.
///
/// The comparison must satisfy, for all `a`, `b` and `c`:
///
/// - antisymmetry: if `a < b` then `!(a > b)` and vice versa; and
/// - transitivity: `a < b` and `b < c` implies `a < c`. The same must hold for
///   both `==` and `>`.
///
/// Note that these requirements mean that the trait itself must be
/// implemented symmetrically and transitively: if `T: PartialOrd<U>`
/// and `U: PartialOrd<V>` then `U: PartialOrd<T>` and `T:
/// PartialOrd<V>`.
///
/// PartialOrd only requires implementation of the `partial_cmp` method,
/// with the others generated from default implementations.
///
/// However it remains possible to implement the others separately for types
/// which do not have a total order. For example, for floating point numbers,
/// `NaN < 0 == false` and `NaN >= 0 == false` (cf. IEEE 754-2008 section
/// 5.11).
#[lang="ord"]
#[stable]
pub trait PartialOrd<Rhs: ?Sized = Self>: PartialEq<Rhs> {
    /// This method returns an ordering between `self` and `other` values
    /// if one exists.
    #[stable]
    fn partial_cmp(&self, other: &Rhs) -> Option<Ordering>;

    /// This method tests less than (for `self` and `other`) and is used by the `<` operator.
    #[inline]
    #[stable]
    fn lt(&self, other: &Rhs) -> bool {
        match self.partial_cmp(other) {
            Some(Less) => true,
            _ => false,
        }
    }

    /// This method tests less than or equal to (`<=`).
    #[inline]
    #[stable]
    fn le(&self, other: &Rhs) -> bool {
        match self.partial_cmp(other) {
            Some(Less) | Some(Equal) => true,
            _ => false,
        }
    }

    /// This method tests greater than (`>`).
    #[inline]
    #[stable]
    fn gt(&self, other: &Rhs) -> bool {
        match self.partial_cmp(other) {
            Some(Greater) => true,
            _ => false,
        }
    }

    /// This method tests greater than or equal to (`>=`).
    #[inline]
    #[stable]
    fn ge(&self, other: &Rhs) -> bool {
        match self.partial_cmp(other) {
            Some(Greater) | Some(Equal) => true,
            _ => false,
        }
    }
}

/// Compare and return the minimum of two values.
#[inline]
#[stable]
pub fn min<T: Ord>(v1: T, v2: T) -> T {
    if v1 < v2 { v1 } else { v2 }
}

/// Compare and return the maximum of two values.
#[inline]
#[stable]
pub fn max<T: Ord>(v1: T, v2: T) -> T {
    if v1 > v2 { v1 } else { v2 }
}

/// Compare and return the minimum of two values if there is one.
///
/// Returns the first argument if the comparison determines them to be equal.
#[inline]
#[experimental]
pub fn partial_min<T: PartialOrd>(v1: T, v2: T) -> Option<T> {
    match v1.partial_cmp(&v2) {
        Some(Less) | Some(Equal) => Some(v1),
        Some(Greater) => Some(v2),
        None => None
    }
}

/// Compare and return the maximum of two values if there is one.
///
/// Returns the first argument if the comparison determines them to be equal.
#[inline]
#[experimental]
pub fn partial_max<T: PartialOrd>(v1: T, v2: T) -> Option<T> {
    match v1.partial_cmp(&v2) {
        Some(Less) => Some(v2),
        Some(Equal) | Some(Greater) => Some(v1),
        None => None
    }
}

// Implementation of PartialEq, Eq, PartialOrd and Ord for primitive types
mod impls {
    use cmp::{PartialOrd, Ord, PartialEq, Eq, Ordering};
    use cmp::Ordering::{Less, Greater, Equal};
    use marker::Sized;
    use option::Option;
    use option::Option::{Some, None};

    macro_rules! partial_eq_impl {
        ($($t:ty)*) => ($(
            #[stable]
            impl PartialEq for $t {
                #[inline]
                fn eq(&self, other: &$t) -> bool { (*self) == (*other) }
                #[inline]
                fn ne(&self, other: &$t) -> bool { (*self) != (*other) }
            }
        )*)
    }

    #[stable]
    impl PartialEq for () {
        #[inline]
        fn eq(&self, _other: &()) -> bool { true }
        #[inline]
        fn ne(&self, _other: &()) -> bool { false }
    }

    partial_eq_impl! {
        bool char uint u8 u16 u32 u64 int i8 i16 i32 i64 f32 f64
    }

    macro_rules! eq_impl {
        ($($t:ty)*) => ($(
            #[stable]
            impl Eq for $t {}
        )*)
    }

    eq_impl! { () bool char uint u8 u16 u32 u64 int i8 i16 i32 i64 }

    macro_rules! partial_ord_impl {
        ($($t:ty)*) => ($(
            #[stable]
            impl PartialOrd for $t {
                #[inline]
                fn partial_cmp(&self, other: &$t) -> Option<Ordering> {
                    match (self <= other, self >= other) {
                        (false, false) => None,
                        (false, true) => Some(Greater),
                        (true, false) => Some(Less),
                        (true, true) => Some(Equal),
                    }
                }
                #[inline]
                fn lt(&self, other: &$t) -> bool { (*self) < (*other) }
                #[inline]
                fn le(&self, other: &$t) -> bool { (*self) <= (*other) }
                #[inline]
                fn ge(&self, other: &$t) -> bool { (*self) >= (*other) }
                #[inline]
                fn gt(&self, other: &$t) -> bool { (*self) > (*other) }
            }
        )*)
    }

    #[stable]
    impl PartialOrd for () {
        #[inline]
        fn partial_cmp(&self, _: &()) -> Option<Ordering> {
            Some(Equal)
        }
    }

    #[stable]
    impl PartialOrd for bool {
        #[inline]
        fn partial_cmp(&self, other: &bool) -> Option<Ordering> {
            (*self as u8).partial_cmp(&(*other as u8))
        }
    }

    partial_ord_impl! { char uint u8 u16 u32 u64 int i8 i16 i32 i64 f32 f64 }

    macro_rules! ord_impl {
        ($($t:ty)*) => ($(
            #[stable]
            impl Ord for $t {
                #[inline]
                fn cmp(&self, other: &$t) -> Ordering {
                    if *self < *other { Less }
                    else if *self > *other { Greater }
                    else { Equal }
                }
            }
        )*)
    }

    #[stable]
    impl Ord for () {
        #[inline]
        fn cmp(&self, _other: &()) -> Ordering { Equal }
    }

    #[stable]
    impl Ord for bool {
        #[inline]
        fn cmp(&self, other: &bool) -> Ordering {
            (*self as u8).cmp(&(*other as u8))
        }
    }

    ord_impl! { char uint u8 u16 u32 u64 int i8 i16 i32 i64 }

    // & pointers

    #[stable]
    impl<'a, 'b, A: ?Sized, B: ?Sized> PartialEq<&'b B> for &'a A where A: PartialEq<B> {
        #[inline]
        fn eq(&self, other: & &'b B) -> bool { PartialEq::eq(*self, *other) }
        #[inline]
        fn ne(&self, other: & &'b B) -> bool { PartialEq::ne(*self, *other) }
    }
    #[stable]
    impl<'a, 'b, A: ?Sized, B: ?Sized> PartialOrd<&'b B> for &'a A where A: PartialOrd<B> {
        #[inline]
        fn partial_cmp(&self, other: &&'b B) -> Option<Ordering> {
            PartialOrd::partial_cmp(*self, *other)
        }
        #[inline]
        fn lt(&self, other: & &'b B) -> bool { PartialOrd::lt(*self, *other) }
        #[inline]
        fn le(&self, other: & &'b B) -> bool { PartialOrd::le(*self, *other) }
        #[inline]
        fn ge(&self, other: & &'b B) -> bool { PartialOrd::ge(*self, *other) }
        #[inline]
        fn gt(&self, other: & &'b B) -> bool { PartialOrd::gt(*self, *other) }
    }
    #[stable]
    impl<'a, A: ?Sized> Ord for &'a A where A: Ord {
        #[inline]
        fn cmp(&self, other: & &'a A) -> Ordering { Ord::cmp(*self, *other) }
    }
    #[stable]
    impl<'a, A: ?Sized> Eq for &'a A where A: Eq {}

    // &mut pointers

    #[stable]
    impl<'a, 'b, A: ?Sized, B: ?Sized> PartialEq<&'b mut B> for &'a mut A where A: PartialEq<B> {
        #[inline]
        fn eq(&self, other: &&'b mut B) -> bool { PartialEq::eq(*self, *other) }
        #[inline]
        fn ne(&self, other: &&'b mut B) -> bool { PartialEq::ne(*self, *other) }
    }
    #[stable]
    impl<'a, 'b, A: ?Sized, B: ?Sized> PartialOrd<&'b mut B> for &'a mut A where A: PartialOrd<B> {
        #[inline]
        fn partial_cmp(&self, other: &&'b mut B) -> Option<Ordering> {
            PartialOrd::partial_cmp(*self, *other)
        }
        #[inline]
        fn lt(&self, other: &&'b mut B) -> bool { PartialOrd::lt(*self, *other) }
        #[inline]
        fn le(&self, other: &&'b mut B) -> bool { PartialOrd::le(*self, *other) }
        #[inline]
        fn ge(&self, other: &&'b mut B) -> bool { PartialOrd::ge(*self, *other) }
        #[inline]
        fn gt(&self, other: &&'b mut B) -> bool { PartialOrd::gt(*self, *other) }
    }
    #[stable]
    impl<'a, A: ?Sized> Ord for &'a mut A where A: Ord {
        #[inline]
        fn cmp(&self, other: &&'a mut A) -> Ordering { Ord::cmp(*self, *other) }
    }
    #[stable]
    impl<'a, A: ?Sized> Eq for &'a mut A where A: Eq {}

    #[stable]
    impl<'a, 'b, A: ?Sized, B: ?Sized> PartialEq<&'b mut B> for &'a A where A: PartialEq<B> {
        #[inline]
        fn eq(&self, other: &&'b mut B) -> bool { PartialEq::eq(*self, *other) }
        #[inline]
        fn ne(&self, other: &&'b mut B) -> bool { PartialEq::ne(*self, *other) }
    }

    #[stable]
    impl<'a, 'b, A: ?Sized, B: ?Sized> PartialEq<&'b B> for &'a mut A where A: PartialEq<B> {
        #[inline]
        fn eq(&self, other: &&'b B) -> bool { PartialEq::eq(*self, *other) }
        #[inline]
        fn ne(&self, other: &&'b B) -> bool { PartialEq::ne(*self, *other) }
    }
}
