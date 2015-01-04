// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![deprecated = "use std::vec::CowVec"]

pub use self::MaybeOwnedVector::*;

use std::cmp::Ordering;
use std::default::Default;
use std::fmt;
use std::iter::FromIterator;
use std::path::BytesContainer;
use std::slice;

// Note 1: It is not clear whether the flexibility of providing both
// the `Growable` and `FixedLen` variants is sufficiently useful.
// Consider restricting to just a two variant enum.

// Note 2: Once Dynamically Sized Types (DST) lands, it might be
// reasonable to replace this with something like `enum MaybeOwned<'a,
// Sized? U>{ Owned(Box<U>), Borrowed(&'a U) }`; and then `U` could be
// instantiated with `[T]` or `str`, etc.  Of course, that would imply
// removing the `Growable` variant, which relates to note 1 above.
// Alternatively, we might add `MaybeOwned` for the general case but
// keep some form of `MaybeOwnedVector` to avoid unnecessary copying
// of the contents of `Vec<T>`, since we anticipate that to be a
// frequent way to dynamically construct a vector.

/// MaybeOwnedVector<'a,T> abstracts over `Vec<T>`, `&'a [T]`.
///
/// Some clients will have a pre-allocated vector ready to hand off in
/// a slice; others will want to create the set on the fly and hand
/// off ownership, via `Growable`.
pub enum MaybeOwnedVector<'a,T:'a> {
    Growable(Vec<T>),
    Borrowed(&'a [T]),
}

/// Trait for moving into a `MaybeOwnedVector`
pub trait IntoMaybeOwnedVector<'a,T> {
    /// Moves self into a `MaybeOwnedVector`
    fn into_maybe_owned(self) -> MaybeOwnedVector<'a,T>;
}

#[allow(deprecated)]
impl<'a,T:'a> IntoMaybeOwnedVector<'a,T> for Vec<T> {
    #[allow(deprecated)]
    #[inline]
    fn into_maybe_owned(self) -> MaybeOwnedVector<'a,T> { Growable(self) }
}

#[allow(deprecated)]
impl<'a,T> IntoMaybeOwnedVector<'a,T> for &'a [T] {
    #[allow(deprecated)]
    #[inline]
    fn into_maybe_owned(self) -> MaybeOwnedVector<'a,T> { Borrowed(self) }
}

impl<'a,T> MaybeOwnedVector<'a,T> {
    pub fn iter(&'a self) -> slice::Iter<'a,T> {
        match self {
            &Growable(ref v) => v.as_slice().iter(),
            &Borrowed(ref v) => v.iter(),
        }
    }

    pub fn len(&self) -> uint { self.as_slice().len() }

    #[allow(deprecated)]
    pub fn is_empty(&self) -> bool { self.len() == 0 }
}

impl<'a, T: PartialEq> PartialEq for MaybeOwnedVector<'a, T> {
    fn eq(&self, other: &MaybeOwnedVector<T>) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl<'a, T: Eq> Eq for MaybeOwnedVector<'a, T> {}

impl<'a, T: PartialOrd> PartialOrd for MaybeOwnedVector<'a, T> {
    fn partial_cmp(&self, other: &MaybeOwnedVector<T>) -> Option<Ordering> {
        self.as_slice().partial_cmp(other.as_slice())
    }
}

impl<'a, T: Ord> Ord for MaybeOwnedVector<'a, T> {
    fn cmp(&self, other: &MaybeOwnedVector<T>) -> Ordering {
        self.as_slice().cmp(other.as_slice())
    }
}

// The `Vector` trait is provided in the prelude and is implemented on
// both `&'a [T]` and `Vec<T>`, so it makes sense to try to support it
// seamlessly.  The other vector related traits from the prelude do
// not appear to be implemented on both `&'a [T]` and `Vec<T>`.  (It
// is possible that this is an oversight in some cases.)
//
// In any case, with `Vector` in place, the client can just use
// `as_slice` if they prefer that over `match`.

impl<'b,T> AsSlice<T> for MaybeOwnedVector<'b,T> {
    fn as_slice<'a>(&'a self) -> &'a [T] {
        match self {
            &Growable(ref v) => v.as_slice(),
            &Borrowed(ref v) => v.as_slice(),
        }
    }
}

impl<'a,T> FromIterator<T> for MaybeOwnedVector<'a,T> {
    #[allow(deprecated)]
    fn from_iter<I:Iterator<Item=T>>(iterator: I) -> MaybeOwnedVector<'a,T> {
        // If we are building from scratch, might as well build the
        // most flexible variant.
        Growable(iterator.collect())
    }
}

impl<'a,T:fmt::Show> fmt::Show for MaybeOwnedVector<'a,T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.as_slice().fmt(f)
    }
}

impl<'a, T: Clone> Clone for MaybeOwnedVector<'a, T> {
    #[allow(deprecated)]
    fn clone(&self) -> MaybeOwnedVector<'a, T> {
        match *self {
            Growable(ref v) => Growable(v.clone()),
            Borrowed(v) => Borrowed(v)
        }
    }
}

impl<'a, T> Default for MaybeOwnedVector<'a, T> {
    #[allow(deprecated)]
    fn default() -> MaybeOwnedVector<'a, T> {
        Growable(Vec::new())
    }
}

impl<'a> BytesContainer for MaybeOwnedVector<'a, u8> {
    fn container_as_bytes(&self) -> &[u8] {
        self.as_slice()
    }
}

impl<'a,T:Clone> MaybeOwnedVector<'a,T> {
    /// Convert `self` into a growable `Vec`, not making a copy if possible.
    pub fn into_vec(self) -> Vec<T> {
        match self {
            Growable(v) => v,
            Borrowed(v) => v.to_vec(),
        }
    }
}
