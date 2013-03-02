// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!
 *
 * Defines a type OptVec<T> that can be used in place of ~[T].
 * OptVec avoids the need for allocation for empty vectors.
 * OptVec implements the iterable interface as well as
 * other useful things like `push()` and `len()`.
 */

use core::prelude::*;
use core::iter;
use core::iter::BaseIter;

#[auto_encode]
#[auto_decode]
pub enum OptVec<T> {
    Empty,
    Vec(~[T])
}

pub fn with<T>(+t: T) -> OptVec<T> {
    Vec(~[t])
}

pub fn from<T>(+t: ~[T]) -> OptVec<T> {
    if t.len() == 0 {
        Empty
    } else {
        Vec(t)
    }
}

impl<T> OptVec<T> {
    fn push(&mut self, +t: T) {
        match *self {
            Vec(ref mut v) => {
                v.push(t);
                return;
            }
            Empty => {}
        }

        // FIXME(#5074): flow insensitive means we can't move
        // assignment inside `match`
        *self = Vec(~[t]);
    }

    fn map<U>(&self, op: &fn(&T) -> U) -> OptVec<U> {
        match *self {
            Empty => Empty,
            Vec(ref v) => Vec(v.map(op))
        }
    }

    pure fn get(&self, i: uint) -> &self/T {
        match *self {
            Empty => fail!(fmt!("Invalid index %u", i)),
            Vec(ref v) => &v[i]
        }
    }

    pure fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pure fn len(&self) -> uint {
        match *self {
            Empty => 0,
            Vec(ref v) => v.len()
        }
    }
}

pub fn take_vec<T>(+v: OptVec<T>) -> ~[T] {
    match v {
        Empty => ~[],
        Vec(v) => v
    }
}

impl<T:Copy> OptVec<T> {
    fn prepend(&self, +t: T) -> OptVec<T> {
        let mut v0 = ~[t];
        match *self {
            Empty => {}
            Vec(ref v1) => { v0.push_all(*v1); }
        }
        return Vec(v0);
    }

    fn push_all<I: BaseIter<T>>(&mut self, from: &I) {
        for from.each |e| {
            self.push(copy *e);
        }
    }
}

impl<A:Eq> Eq for OptVec<A> {
    pure fn eq(&self, other: &OptVec<A>) -> bool {
        // Note: cannot use #[deriving_eq] here because
        // (Empty, Vec(~[])) ought to be equal.
        match (self, other) {
            (&Empty, &Empty) => true,
            (&Empty, &Vec(ref v)) => v.is_empty(),
            (&Vec(ref v), &Empty) => v.is_empty(),
            (&Vec(ref v1), &Vec(ref v2)) => *v1 == *v2
        }
    }

    pure fn ne(&self, other: &OptVec<A>) -> bool {
        !self.eq(other)
    }
}

impl<A> BaseIter<A> for OptVec<A> {
    pure fn each(&self, blk: fn(v: &A) -> bool) {
        match *self {
            Empty => {}
            Vec(ref v) => v.each(blk)
        }
    }

    pure fn size_hint(&self) -> Option<uint> {
        Some(self.len())
    }
}

impl<A> iter::ExtendedIter<A> for OptVec<A> {
    #[inline(always)]
    pure fn eachi(&self, blk: fn(+v: uint, v: &A) -> bool) {
        iter::eachi(self, blk)
    }
    #[inline(always)]
    pure fn all(&self, blk: fn(&A) -> bool) -> bool {
        iter::all(self, blk)
    }
    #[inline(always)]
    pure fn any(&self, blk: fn(&A) -> bool) -> bool {
        iter::any(self, blk)
    }
    #[inline(always)]
    pure fn foldl<B>(&self, +b0: B, blk: fn(&B, &A) -> B) -> B {
        iter::foldl(self, b0, blk)
    }
    #[inline(always)]
    pure fn position(&self, f: fn(&A) -> bool) -> Option<uint> {
        iter::position(self, f)
    }
    #[inline(always)]
    pure fn map_to_vec<B>(&self, op: fn(&A) -> B) -> ~[B] {
        iter::map_to_vec(self, op)
    }
    #[inline(always)]
    pure fn flat_map_to_vec<B,IB:BaseIter<B>>(&self, op: fn(&A) -> IB)
        -> ~[B] {
        iter::flat_map_to_vec(self, op)
    }

}

impl<A: Eq> iter::EqIter<A> for OptVec<A> {
    #[inline(always)]
    pure fn contains(&self, x: &A) -> bool { iter::contains(self, x) }
    #[inline(always)]
    pure fn count(&self, x: &A) -> uint { iter::count(self, x) }
}

impl<A: Copy> iter::CopyableIter<A> for OptVec<A> {
    #[inline(always)]
    pure fn filter_to_vec(&self, pred: fn(&A) -> bool) -> ~[A] {
        iter::filter_to_vec(self, pred)
    }
    #[inline(always)]
    pure fn to_vec(&self) -> ~[A] { iter::to_vec(self) }
    #[inline(always)]
    pure fn find(&self, f: fn(&A) -> bool) -> Option<A> {
        iter::find(self, f)
    }
}

impl<A: Copy+Ord> iter::CopyableOrderedIter<A> for OptVec<A> {
    #[inline(always)]
    pure fn min(&self) -> A { iter::min(self) }
    #[inline(always)]
    pure fn max(&self) -> A { iter::max(self) }
}
