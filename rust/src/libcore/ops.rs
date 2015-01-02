// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Overloadable operators
//!
//! Implementing these traits allows you to get an effect similar to
//! overloading operators.
//!
//! The values for the right hand side of an operator are automatically
//! borrowed, so `a + b` is sugar for `a.add(&b)`.
//!
//! All of these traits are imported by the prelude, so they are available in
//! every Rust program.
//!
//! # Example
//!
//! This example creates a `Point` struct that implements `Add` and `Sub`, and then
//! demonstrates adding and subtracting two `Point`s.
//!
//! ```rust
//! use std::ops::{Add, Sub};
//!
//! #[deriving(Show)]
//! struct Point {
//!     x: int,
//!     y: int
//! }
//!
//! impl Add<Point, Point> for Point {
//!     fn add(self, other: Point) -> Point {
//!         Point {x: self.x + other.x, y: self.y + other.y}
//!     }
//! }
//!
//! impl Sub<Point, Point> for Point {
//!     fn sub(self, other: Point) -> Point {
//!         Point {x: self.x - other.x, y: self.y - other.y}
//!     }
//! }
//! fn main() {
//!     println!("{}", Point {x: 1, y: 0} + Point {x: 2, y: 3});
//!     println!("{}", Point {x: 1, y: 0} - Point {x: 2, y: 3});
//! }
//! ```
//!
//! See the documentation for each trait for a minimum implementation that prints
//! something to the screen.

use clone::Clone;
use iter::{Step, Iterator,DoubleEndedIterator,ExactSizeIterator};
use kinds::Sized;
use option::Option::{mod, Some, None};

/// The `Drop` trait is used to run some code when a value goes out of scope. This
/// is sometimes called a 'destructor'.
///
/// # Example
///
/// A trivial implementation of `Drop`. The `drop` method is called when `_x` goes
/// out of scope, and therefore `main` prints `Dropping!`.
///
/// ```rust
/// struct HasDrop;
///
/// impl Drop for HasDrop {
///     fn drop(&mut self) {
///         println!("Dropping!");
///     }
/// }
///
/// fn main() {
///     let _x = HasDrop;
/// }
/// ```
#[lang="drop"]
pub trait Drop {
    /// The `drop` method, called when the value goes out of scope.
    fn drop(&mut self);
}

/// The `Add` trait is used to specify the functionality of `+`.
///
/// # Example
///
/// A trivial implementation of `Add`. When `Foo + Foo` happens, it ends up
/// calling `add`, and therefore, `main` prints `Adding!`.
///
/// ```rust
/// use std::ops::Add;
///
/// #[deriving(Copy)]
/// struct Foo;
///
/// impl Add<Foo, Foo> for Foo {
///     fn add(self, _rhs: Foo) -> Foo {
///       println!("Adding!");
///       self
///   }
/// }
///
/// fn main() {
///   Foo + Foo;
/// }
/// ```
#[lang="add"]
pub trait Add<RHS, Result> {
    /// The method for the `+` operator
    fn add(self, rhs: RHS) -> Result;
}

macro_rules! add_impl {
    ($($t:ty)*) => ($(
        impl Add<$t, $t> for $t {
            #[inline]
            fn add(self, other: $t) -> $t { self + other }
        }
    )*)
}

add_impl! { uint u8 u16 u32 u64 int i8 i16 i32 i64 f32 f64 }

/// The `Sub` trait is used to specify the functionality of `-`.
///
/// # Example
///
/// A trivial implementation of `Sub`. When `Foo - Foo` happens, it ends up
/// calling `sub`, and therefore, `main` prints `Subtracting!`.
///
/// ```rust
/// use std::ops::Sub;
///
/// #[deriving(Copy)]
/// struct Foo;
///
/// impl Sub<Foo, Foo> for Foo {
///     fn sub(self, _rhs: Foo) -> Foo {
///         println!("Subtracting!");
///         self
///     }
/// }
///
/// fn main() {
///     Foo - Foo;
/// }
/// ```
#[lang="sub"]
pub trait Sub<RHS, Result> {
    /// The method for the `-` operator
    fn sub(self, rhs: RHS) -> Result;
}

macro_rules! sub_impl {
    ($($t:ty)*) => ($(
        impl Sub<$t, $t> for $t {
            #[inline]
            fn sub(self, other: $t) -> $t { self - other }
        }
    )*)
}

sub_impl! { uint u8 u16 u32 u64 int i8 i16 i32 i64 f32 f64 }

/// The `Mul` trait is used to specify the functionality of `*`.
///
/// # Example
///
/// A trivial implementation of `Mul`. When `Foo * Foo` happens, it ends up
/// calling `mul`, and therefore, `main` prints `Multiplying!`.
///
/// ```rust
/// use std::ops::Mul;
///
/// #[deriving(Copy)]
/// struct Foo;
///
/// impl Mul<Foo, Foo> for Foo {
///     fn mul(self, _rhs: Foo) -> Foo {
///         println!("Multiplying!");
///         self
///     }
/// }
///
/// fn main() {
///     Foo * Foo;
/// }
/// ```
#[lang="mul"]
pub trait Mul<RHS, Result> {
    /// The method for the `*` operator
    fn mul(self, rhs: RHS) -> Result;
}

macro_rules! mul_impl {
    ($($t:ty)*) => ($(
        impl Mul<$t, $t> for $t {
            #[inline]
            fn mul(self, other: $t) -> $t { self * other }
        }
    )*)
}

mul_impl! { uint u8 u16 u32 u64 int i8 i16 i32 i64 f32 f64 }

/// The `Div` trait is used to specify the functionality of `/`.
///
/// # Example
///
/// A trivial implementation of `Div`. When `Foo / Foo` happens, it ends up
/// calling `div`, and therefore, `main` prints `Dividing!`.
///
/// ```
/// use std::ops::Div;
///
/// #[deriving(Copy)]
/// struct Foo;
///
/// impl Div<Foo, Foo> for Foo {
///     fn div(self, _rhs: Foo) -> Foo {
///         println!("Dividing!");
///         self
///     }
/// }
///
/// fn main() {
///     Foo / Foo;
/// }
/// ```
#[lang="div"]
pub trait Div<RHS, Result> {
    /// The method for the `/` operator
    fn div(self, rhs: RHS) -> Result;
}

macro_rules! div_impl {
    ($($t:ty)*) => ($(
        impl Div<$t, $t> for $t {
            #[inline]
            fn div(self, other: $t) -> $t { self / other }
        }
    )*)
}

div_impl! { uint u8 u16 u32 u64 int i8 i16 i32 i64 f32 f64 }

/// The `Rem` trait is used to specify the functionality of `%`.
///
/// # Example
///
/// A trivial implementation of `Rem`. When `Foo % Foo` happens, it ends up
/// calling `rem`, and therefore, `main` prints `Remainder-ing!`.
///
/// ```
/// use std::ops::Rem;
///
/// #[deriving(Copy)]
/// struct Foo;
///
/// impl Rem<Foo, Foo> for Foo {
///     fn rem(self, _rhs: Foo) -> Foo {
///         println!("Remainder-ing!");
///         self
///     }
/// }
///
/// fn main() {
///     Foo % Foo;
/// }
/// ```
#[lang="rem"]
pub trait Rem<RHS, Result> {
    /// The method for the `%` operator
    fn rem(self, rhs: RHS) -> Result;
}

macro_rules! rem_impl {
    ($($t:ty)*) => ($(
        impl Rem<$t, $t> for $t {
            #[inline]
            fn rem(self, other: $t) -> $t { self % other }
        }
    )*)
}

macro_rules! rem_float_impl {
    ($t:ty, $fmod:ident) => {
        impl Rem<$t, $t> for $t {
            #[inline]
            fn rem(self, other: $t) -> $t {
                extern { fn $fmod(a: $t, b: $t) -> $t; }
                unsafe { $fmod(self, other) }
            }
        }
    }
}

rem_impl! { uint u8 u16 u32 u64 int i8 i16 i32 i64 }
rem_float_impl! { f32, fmodf }
rem_float_impl! { f64, fmod }

/// The `Neg` trait is used to specify the functionality of unary `-`.
///
/// # Example
///
/// A trivial implementation of `Neg`. When `-Foo` happens, it ends up calling
/// `neg`, and therefore, `main` prints `Negating!`.
///
/// ```
/// use std::ops::Neg;
///
/// struct Foo;
///
/// impl Copy for Foo {}
///
/// impl Neg<Foo> for Foo {
///     fn neg(self) -> Foo {
///         println!("Negating!");
///         self
///     }
/// }
///
/// fn main() {
///     -Foo;
/// }
/// ```
#[lang="neg"]
pub trait Neg<Result> {
    /// The method for the unary `-` operator
    fn neg(self) -> Result;
}

macro_rules! neg_impl {
    ($($t:ty)*) => ($(
        impl Neg<$t> for $t {
            #[inline]
            fn neg(self) -> $t { -self }
        }
    )*)
}

macro_rules! neg_uint_impl {
    ($t:ty, $t_signed:ty) => {
        impl Neg<$t> for $t {
            #[inline]
            fn neg(self) -> $t { -(self as $t_signed) as $t }
        }
    }
}

neg_impl! { int i8 i16 i32 i64 f32 f64 }

neg_uint_impl! { uint, int }
neg_uint_impl! { u8, i8 }
neg_uint_impl! { u16, i16 }
neg_uint_impl! { u32, i32 }
neg_uint_impl! { u64, i64 }


/// The `Not` trait is used to specify the functionality of unary `!`.
///
/// # Example
///
/// A trivial implementation of `Not`. When `!Foo` happens, it ends up calling
/// `not`, and therefore, `main` prints `Not-ing!`.
///
/// ```
/// use std::ops::Not;
///
/// struct Foo;
///
/// impl Copy for Foo {}
///
/// impl Not<Foo> for Foo {
///     fn not(self) -> Foo {
///         println!("Not-ing!");
///         self
///     }
/// }
///
/// fn main() {
///     !Foo;
/// }
/// ```
#[lang="not"]
pub trait Not<Result> {
    /// The method for the unary `!` operator
    fn not(self) -> Result;
}

macro_rules! not_impl {
    ($($t:ty)*) => ($(
        impl Not<$t> for $t {
            #[inline]
            fn not(self) -> $t { !self }
        }
    )*)
}

not_impl! { bool uint u8 u16 u32 u64 int i8 i16 i32 i64 }

/// The `BitAnd` trait is used to specify the functionality of `&`.
///
/// # Example
///
/// A trivial implementation of `BitAnd`. When `Foo & Foo` happens, it ends up
/// calling `bitand`, and therefore, `main` prints `Bitwise And-ing!`.
///
/// ```
/// use std::ops::BitAnd;
///
/// #[deriving(Copy)]
/// struct Foo;
///
/// impl BitAnd<Foo, Foo> for Foo {
///     fn bitand(self, _rhs: Foo) -> Foo {
///         println!("Bitwise And-ing!");
///         self
///     }
/// }
///
/// fn main() {
///     Foo & Foo;
/// }
/// ```
#[lang="bitand"]
pub trait BitAnd<RHS, Result> {
    /// The method for the `&` operator
    fn bitand(self, rhs: RHS) -> Result;
}

macro_rules! bitand_impl {
    ($($t:ty)*) => ($(
        impl BitAnd<$t, $t> for $t {
            #[inline]
            fn bitand(self, rhs: $t) -> $t { self & rhs }
        }
    )*)
}

bitand_impl! { bool uint u8 u16 u32 u64 int i8 i16 i32 i64 }

/// The `BitOr` trait is used to specify the functionality of `|`.
///
/// # Example
///
/// A trivial implementation of `BitOr`. When `Foo | Foo` happens, it ends up
/// calling `bitor`, and therefore, `main` prints `Bitwise Or-ing!`.
///
/// ```
/// use std::ops::BitOr;
///
/// #[deriving(Copy)]
/// struct Foo;
///
/// impl BitOr<Foo, Foo> for Foo {
///     fn bitor(self, _rhs: Foo) -> Foo {
///         println!("Bitwise Or-ing!");
///         self
///     }
/// }
///
/// fn main() {
///     Foo | Foo;
/// }
/// ```
#[lang="bitor"]
pub trait BitOr<RHS, Result> {
    /// The method for the `|` operator
    fn bitor(self, rhs: RHS) -> Result;
}

macro_rules! bitor_impl {
    ($($t:ty)*) => ($(
        impl BitOr<$t,$t> for $t {
            #[inline]
            fn bitor(self, rhs: $t) -> $t { self | rhs }
        }
    )*)
}

bitor_impl! { bool uint u8 u16 u32 u64 int i8 i16 i32 i64 }

/// The `BitXor` trait is used to specify the functionality of `^`.
///
/// # Example
///
/// A trivial implementation of `BitXor`. When `Foo ^ Foo` happens, it ends up
/// calling `bitxor`, and therefore, `main` prints `Bitwise Xor-ing!`.
///
/// ```
/// use std::ops::BitXor;
///
/// #[deriving(Copy)]
/// struct Foo;
///
/// impl BitXor<Foo, Foo> for Foo {
///     fn bitxor(self, _rhs: Foo) -> Foo {
///         println!("Bitwise Xor-ing!");
///         self
///     }
/// }
///
/// fn main() {
///     Foo ^ Foo;
/// }
/// ```
#[lang="bitxor"]
pub trait BitXor<RHS, Result> {
    /// The method for the `^` operator
    fn bitxor(self, rhs: RHS) -> Result;
}

macro_rules! bitxor_impl {
    ($($t:ty)*) => ($(
        impl BitXor<$t, $t> for $t {
            #[inline]
            fn bitxor(self, other: $t) -> $t { self ^ other }
        }
    )*)
}

bitxor_impl! { bool uint u8 u16 u32 u64 int i8 i16 i32 i64 }

/// The `Shl` trait is used to specify the functionality of `<<`.
///
/// # Example
///
/// A trivial implementation of `Shl`. When `Foo << Foo` happens, it ends up
/// calling `shl`, and therefore, `main` prints `Shifting left!`.
///
/// ```
/// use std::ops::Shl;
///
/// #[deriving(Copy)]
/// struct Foo;
///
/// impl Shl<Foo, Foo> for Foo {
///     fn shl(self, _rhs: Foo) -> Foo {
///         println!("Shifting left!");
///         self
///     }
/// }
///
/// fn main() {
///     Foo << Foo;
/// }
/// ```
#[lang="shl"]
pub trait Shl<RHS, Result> {
    /// The method for the `<<` operator
    fn shl(self, rhs: RHS) -> Result;
}

macro_rules! shl_impl {
    ($($t:ty)*) => ($(
        impl Shl<uint, $t> for $t {
            #[inline]
            fn shl(self, other: uint) -> $t {
                self << other
            }
        }
    )*)
}

shl_impl! { uint u8 u16 u32 u64 int i8 i16 i32 i64 }

/// The `Shr` trait is used to specify the functionality of `>>`.
///
/// # Example
///
/// A trivial implementation of `Shr`. When `Foo >> Foo` happens, it ends up
/// calling `shr`, and therefore, `main` prints `Shifting right!`.
///
/// ```
/// use std::ops::Shr;
///
/// #[deriving(Copy)]
/// struct Foo;
///
/// impl Shr<Foo, Foo> for Foo {
///     fn shr(self, _rhs: Foo) -> Foo {
///         println!("Shifting right!");
///         self
///     }
/// }
///
/// fn main() {
///     Foo >> Foo;
/// }
/// ```
#[lang="shr"]
pub trait Shr<RHS, Result> {
    /// The method for the `>>` operator
    fn shr(self, rhs: RHS) -> Result;
}

macro_rules! shr_impl {
    ($($t:ty)*) => ($(
        impl Shr<uint, $t> for $t {
            #[inline]
            fn shr(self, other: uint) -> $t { self >> other }
        }
    )*)
}

shr_impl! { uint u8 u16 u32 u64 int i8 i16 i32 i64 }

/// The `Index` trait is used to specify the functionality of indexing operations
/// like `arr[idx]` when used in an immutable context.
///
/// # Example
///
/// A trivial implementation of `Index`. When `Foo[Foo]` happens, it ends up
/// calling `index`, and therefore, `main` prints `Indexing!`.
///
/// ```
/// use std::ops::Index;
///
/// #[deriving(Copy)]
/// struct Foo;
///
/// impl Index<Foo, Foo> for Foo {
///     fn index<'a>(&'a self, _index: &Foo) -> &'a Foo {
///         println!("Indexing!");
///         self
///     }
/// }
///
/// fn main() {
///     Foo[Foo];
/// }
/// ```
#[lang="index"]
pub trait Index<Sized? Index, Sized? Result> for Sized? {
    /// The method for the indexing (`Foo[Bar]`) operation
    fn index<'a>(&'a self, index: &Index) -> &'a Result;
}

/// The `IndexMut` trait is used to specify the functionality of indexing
/// operations like `arr[idx]`, when used in a mutable context.
///
/// # Example
///
/// A trivial implementation of `IndexMut`. When `Foo[Foo]` happens, it ends up
/// calling `index_mut`, and therefore, `main` prints `Indexing!`.
///
/// ```
/// use std::ops::IndexMut;
///
/// #[deriving(Copy)]
/// struct Foo;
///
/// impl IndexMut<Foo, Foo> for Foo {
///     fn index_mut<'a>(&'a mut self, _index: &Foo) -> &'a mut Foo {
///         println!("Indexing!");
///         self
///     }
/// }
///
/// fn main() {
///     &mut Foo[Foo];
/// }
/// ```
#[lang="index_mut"]
pub trait IndexMut<Sized? Index, Sized? Result> for Sized? {
    /// The method for the indexing (`Foo[Bar]`) operation
    fn index_mut<'a>(&'a mut self, index: &Index) -> &'a mut Result;
}

/// The `Slice` trait is used to specify the functionality of slicing operations
/// like `arr[from..to]` when used in an immutable context.
///
/// # Example
///
/// A trivial implementation of `Slice`. When `Foo[..Foo]` happens, it ends up
/// calling `slice_to`, and therefore, `main` prints `Slicing!`.
///
/// ```ignore
/// use std::ops::Slice;
///
/// #[deriving(Copy)]
/// struct Foo;
///
/// impl Slice<Foo, Foo> for Foo {
///     fn as_slice_<'a>(&'a self) -> &'a Foo {
///         println!("Slicing!");
///         self
///     }
///     fn slice_from_or_fail<'a>(&'a self, _from: &Foo) -> &'a Foo {
///         println!("Slicing!");
///         self
///     }
///     fn slice_to_or_fail<'a>(&'a self, _to: &Foo) -> &'a Foo {
///         println!("Slicing!");
///         self
///     }
///     fn slice_or_fail<'a>(&'a self, _from: &Foo, _to: &Foo) -> &'a Foo {
///         println!("Slicing!");
///         self
///     }
/// }
///
/// fn main() {
///     Foo[..Foo];
/// }
/// ```
#[lang="slice"]
pub trait Slice<Sized? Idx, Sized? Result> for Sized? {
    /// The method for the slicing operation foo[]
    fn as_slice_<'a>(&'a self) -> &'a Result;
    /// The method for the slicing operation foo[from..]
    fn slice_from_or_fail<'a>(&'a self, from: &Idx) -> &'a Result;
    /// The method for the slicing operation foo[..to]
    fn slice_to_or_fail<'a>(&'a self, to: &Idx) -> &'a Result;
    /// The method for the slicing operation foo[from..to]
    fn slice_or_fail<'a>(&'a self, from: &Idx, to: &Idx) -> &'a Result;
}

/// The `SliceMut` trait is used to specify the functionality of slicing
/// operations like `arr[from..to]`, when used in a mutable context.
///
/// # Example
///
/// A trivial implementation of `SliceMut`. When `Foo[Foo..]` happens, it ends up
/// calling `slice_from_mut`, and therefore, `main` prints `Slicing!`.
///
/// ```ignore
/// use std::ops::SliceMut;
///
/// #[deriving(Copy)]
/// struct Foo;
///
/// impl SliceMut<Foo, Foo> for Foo {
///     fn as_mut_slice_<'a>(&'a mut self) -> &'a mut Foo {
///         println!("Slicing!");
///         self
///     }
///     fn slice_from_or_fail_mut<'a>(&'a mut self, _from: &Foo) -> &'a mut Foo {
///         println!("Slicing!");
///         self
///     }
///     fn slice_to_or_fail_mut<'a>(&'a mut self, _to: &Foo) -> &'a mut Foo {
///         println!("Slicing!");
///         self
///     }
///     fn slice_or_fail_mut<'a>(&'a mut self, _from: &Foo, _to: &Foo) -> &'a mut Foo {
///         println!("Slicing!");
///         self
///     }
/// }
///
/// pub fn main() {
///     Foo[mut Foo..];
/// }
/// ```
#[lang="slice_mut"]
pub trait SliceMut<Sized? Idx, Sized? Result> for Sized? {
    /// The method for the slicing operation foo[]
    fn as_mut_slice_<'a>(&'a mut self) -> &'a mut Result;
    /// The method for the slicing operation foo[from..]
    fn slice_from_or_fail_mut<'a>(&'a mut self, from: &Idx) -> &'a mut Result;
    /// The method for the slicing operation foo[..to]
    fn slice_to_or_fail_mut<'a>(&'a mut self, to: &Idx) -> &'a mut Result;
    /// The method for the slicing operation foo[from..to]
    fn slice_or_fail_mut<'a>(&'a mut self, from: &Idx, to: &Idx) -> &'a mut Result;
}


/// An unbounded range.
#[deriving(Copy)]
#[lang="full_range"]
pub struct FullRange;

/// A (half-open) range which is bounded at both ends.
#[deriving(Copy)]
#[lang="range"]
pub struct Range<Idx> {
    /// The lower bound of the range (inclusive).
    pub start: Idx,
    /// The upper bound of the range (exclusive).
    pub end: Idx,
}

// FIXME(#19391) needs a snapshot
//impl<Idx: Clone + Step<T=uint>> Iterator<Idx> for Range<Idx> {
impl<Idx: Clone + Step> Iterator<Idx> for Range<Idx> {
    #[inline]
    fn next(&mut self) -> Option<Idx> {
        if self.start < self.end {
            let result = self.start.clone();
            self.start.step();
            return Some(result);
        }

        return None;
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        if let Some(hint) = Step::steps_between(&self.end, &self.start) {
            (hint, Some(hint))
        } else {
            (0, None)
        }
    }
}

impl<Idx: Clone + Step> DoubleEndedIterator<Idx> for Range<Idx> {
    #[inline]
    fn next_back(&mut self) -> Option<Idx> {
        if self.start < self.end {
            self.end.step_back();
            return Some(self.end.clone());
        }

        return None;
    }
}

impl<Idx: Clone + Step> ExactSizeIterator<Idx> for Range<Idx> {}

/// A range which is only bounded below.
#[deriving(Copy)]
#[lang="range_from"]
pub struct RangeFrom<Idx> {
    /// The lower bound of the range (inclusive).
    pub start: Idx,
}

impl<Idx: Clone + Step> Iterator<Idx> for RangeFrom<Idx> {
    #[inline]
    fn next(&mut self) -> Option<Idx> {
        // Deliberately overflow so we loop forever.
        let result = self.start.clone();
        self.start.step();
        return Some(result);
    }
}

/// A range which is only bounded above.
#[deriving(Copy)]
#[lang="range_to"]
pub struct RangeTo<Idx> {
    /// The upper bound of the range (exclusive).
    pub end: Idx,
}


/// The `Deref` trait is used to specify the functionality of dereferencing
/// operations like `*v`.
///
/// # Example
///
/// A struct with a single field which is accessible via dereferencing the
/// struct.
///
/// ```
/// #![feature(associated_types)]
///
/// use std::ops::Deref;
///
/// struct DerefExample<T> {
///     value: T
/// }
///
/// impl<T> Deref for DerefExample<T> {
///     type Target = T;
///
///     fn deref<'a>(&'a self) -> &'a T {
///         &self.value
///     }
/// }
///
/// fn main() {
///     let x = DerefExample { value: 'a' };
///     assert_eq!('a', *x);
/// }
/// ```
#[lang="deref"]
pub trait Deref for Sized? {
    type Sized? Target;

    /// The method called to dereference a value
    fn deref<'a>(&'a self) -> &'a Self::Target;
}

impl<'a, Sized? T> Deref for &'a T {
    type Target = T;

    fn deref(&self) -> &T { *self }
}

impl<'a, Sized? T> Deref for &'a mut T {
    type Target = T;

    fn deref(&self) -> &T { *self }
}

/// The `DerefMut` trait is used to specify the functionality of dereferencing
/// mutably like `*v = 1;`
///
/// # Example
///
/// A struct with a single field which is modifiable via dereferencing the
/// struct.
///
/// ```
/// #![feature(associated_types)]
///
/// use std::ops::{Deref, DerefMut};
///
/// struct DerefMutExample<T> {
///     value: T
/// }
///
/// impl<T> Deref for DerefMutExample<T> {
///     type Target = T;
///
///     fn deref<'a>(&'a self) -> &'a T {
///         &self.value
///     }
/// }
///
/// impl<T> DerefMut for DerefMutExample<T> {
///     fn deref_mut<'a>(&'a mut self) -> &'a mut T {
///         &mut self.value
///     }
/// }
///
/// fn main() {
///     let mut x = DerefMutExample { value: 'a' };
///     *x = 'b';
///     assert_eq!('b', *x);
/// }
/// ```
#[lang="deref_mut"]
pub trait DerefMut for Sized? : Deref {
    /// The method called to mutably dereference a value
    fn deref_mut<'a>(&'a mut self) -> &'a mut <Self as Deref>::Target;
}

impl<'a, Sized? T> DerefMut for &'a mut T {
    fn deref_mut(&mut self) -> &mut T { *self }
}

/// A version of the call operator that takes an immutable receiver.
#[lang="fn"]
pub trait Fn<Args,Result> for Sized? {
    /// This is called when the call operator is used.
    extern "rust-call" fn call(&self, args: Args) -> Result;
}

/// A version of the call operator that takes a mutable receiver.
#[lang="fn_mut"]
pub trait FnMut<Args,Result> for Sized? {
    /// This is called when the call operator is used.
    extern "rust-call" fn call_mut(&mut self, args: Args) -> Result;
}

/// A version of the call operator that takes a by-value receiver.
#[lang="fn_once"]
pub trait FnOnce<Args,Result> {
    /// This is called when the call operator is used.
    extern "rust-call" fn call_once(self, args: Args) -> Result;
}

impl<Sized? F,A,R> FnMut<A,R> for F
    where F : Fn<A,R>
{
    extern "rust-call" fn call_mut(&mut self, args: A) -> R {
        self.call(args)
    }
}

impl<F,A,R> FnOnce<A,R> for F
    where F : FnMut<A,R>
{
    extern "rust-call" fn call_once(mut self, args: A) -> R {
        self.call_mut(args)
    }
}
