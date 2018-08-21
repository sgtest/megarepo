// Copyright 2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! A vector type intended to be used for collecting from iterators onto the stack.
//!
//! Space for up to N elements is provided on the stack.  If more elements are collected, Vec is
//! used to store the values on the heap. SmallVec is similar to AccumulateVec, but adds
//! the ability to push elements.
//!
//! The N above is determined by Array's implementor, by way of an associated constant.

use std::ops::{Deref, DerefMut};
use std::iter::{IntoIterator, FromIterator};
use std::fmt::{self, Debug};
use std::mem;
use std::ptr;

use rustc_serialize::{Encodable, Encoder, Decodable, Decoder};

use accumulate_vec::{IntoIter, AccumulateVec};
use array_vec::Array;

pub struct SmallVec<A: Array>(AccumulateVec<A>);

pub type OneVector<T> = SmallVec<[T; 1]>;

impl<A> Clone for SmallVec<A>
    where A: Array,
          A::Element: Clone {
    fn clone(&self) -> Self {
        SmallVec(self.0.clone())
    }
}

impl<A> Debug for SmallVec<A>
    where A: Array + Debug,
          A::Element: Debug {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("SmallVec").field(&self.0).finish()
    }
}

impl<A: Array> SmallVec<A> {
    pub fn new() -> Self {
        SmallVec(AccumulateVec::new())
    }

    pub fn is_array(&self) -> bool {
        self.0.is_array()
    }

    pub fn with_capacity(cap: usize) -> Self {
        let mut vec = SmallVec::new();
        vec.reserve(cap);
        vec
    }

    pub fn one(el: A::Element) -> Self {
        SmallVec(AccumulateVec::one(el))
    }

    pub fn many<I: IntoIterator<Item=A::Element>>(els: I) -> Self {
        SmallVec(AccumulateVec::many(els))
    }

    pub fn expect_one(self, err: &'static str) -> A::Element {
        assert!(self.len() == 1, err);
        match self.0 {
            AccumulateVec::Array(arr) => arr.into_iter().next().unwrap(),
            AccumulateVec::Heap(vec) => vec.into_iter().next().unwrap(),
        }
    }

    /// Will reallocate onto the heap if needed.
    pub fn push(&mut self, el: A::Element) {
        self.reserve(1);
        match self.0 {
            AccumulateVec::Array(ref mut array) => array.push(el),
            AccumulateVec::Heap(ref mut vec) => vec.push(el),
        }
    }

    pub fn reserve(&mut self, n: usize) {
        match self.0 {
            AccumulateVec::Array(_) => {
                if self.len() + n > A::LEN {
                    let len = self.len();
                    let array = mem::replace(&mut self.0,
                            AccumulateVec::Heap(Vec::with_capacity(len + n)));
                    if let AccumulateVec::Array(array) = array {
                        match self.0 {
                            AccumulateVec::Heap(ref mut vec) => vec.extend(array),
                            _ => unreachable!()
                        }
                    }
                }
            }
            AccumulateVec::Heap(ref mut vec) => vec.reserve(n)
        }
    }

    pub unsafe fn set_len(&mut self, len: usize) {
        match self.0 {
            AccumulateVec::Array(ref mut arr) => arr.set_len(len),
            AccumulateVec::Heap(ref mut vec) => vec.set_len(len),
        }
    }

    pub fn insert(&mut self, index: usize, element: A::Element) {
        let len = self.len();

        // Reserve space for shifting elements to the right
        self.reserve(1);

        assert!(index <= len);

        unsafe {
            // infallible
            // The spot to put the new value
            {
                let p = self.as_mut_ptr().add(index);
                // Shift everything over to make space. (Duplicating the
                // `index`th element into two consecutive places.)
                ptr::copy(p, p.offset(1), len - index);
                // Write it in, overwriting the first copy of the `index`th
                // element.
                ptr::write(p, element);
            }
            self.set_len(len + 1);
        }
    }

    pub fn truncate(&mut self, len: usize) {
        unsafe {
            while len < self.len() {
                // Decrement len before the drop_in_place(), so a panic on Drop
                // doesn't re-drop the just-failed value.
                let newlen = self.len() - 1;
                self.set_len(newlen);
                ::std::ptr::drop_in_place(self.get_unchecked_mut(newlen));
            }
        }
    }
}

impl<A: Array> Deref for SmallVec<A> {
    type Target = AccumulateVec<A>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<A: Array> DerefMut for SmallVec<A> {
    fn deref_mut(&mut self) -> &mut AccumulateVec<A> {
        &mut self.0
    }
}

impl<A: Array> FromIterator<A::Element> for SmallVec<A> {
    fn from_iter<I>(iter: I) -> Self where I: IntoIterator<Item=A::Element> {
        SmallVec(iter.into_iter().collect())
    }
}

impl<A: Array> Extend<A::Element> for SmallVec<A> {
    fn extend<I: IntoIterator<Item=A::Element>>(&mut self, iter: I) {
        let iter = iter.into_iter();
        self.reserve(iter.size_hint().0);
        match self.0 {
            AccumulateVec::Heap(ref mut vec) => vec.extend(iter),
            _ => iter.for_each(|el| self.push(el))
        }
    }
}

impl<A: Array> IntoIterator for SmallVec<A> {
    type Item = A::Element;
    type IntoIter = IntoIter<A>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<A: Array> Default for SmallVec<A> {
    fn default() -> SmallVec<A> {
        SmallVec::new()
    }
}

impl<A> Encodable for SmallVec<A>
    where A: Array,
          A::Element: Encodable {
    fn encode<S: Encoder>(&self, s: &mut S) -> Result<(), S::Error> {
        s.emit_seq(self.len(), |s| {
            for (i, e) in self.iter().enumerate() {
                s.emit_seq_elt(i, |s| e.encode(s))?;
            }
            Ok(())
        })
    }
}

impl<A> Decodable for SmallVec<A>
    where A: Array,
          A::Element: Decodable {
    fn decode<D: Decoder>(d: &mut D) -> Result<SmallVec<A>, D::Error> {
        d.read_seq(|d, len| {
            let mut vec = SmallVec::with_capacity(len);
            // FIXME(#48994) - could just be collected into a Result<SmallVec, D::Error>
            for i in 0..len {
                vec.push(d.read_seq_elt(i, |d| Decodable::decode(d))?);
            }
            Ok(vec)
        })
    }
}

#[cfg(test)]
mod tests {
    extern crate test;
    use self::test::Bencher;

    use super::*;

    #[test]
    fn test_len() {
        let v: OneVector<isize> = OneVector::new();
        assert_eq!(0, v.len());

        assert_eq!(1, OneVector::one(1).len());
        assert_eq!(5, OneVector::many(vec![1, 2, 3, 4, 5]).len());
    }

    #[test]
    fn test_push_get() {
        let mut v = OneVector::new();
        v.push(1);
        assert_eq!(1, v.len());
        assert_eq!(1, v[0]);
        v.push(2);
        assert_eq!(2, v.len());
        assert_eq!(2, v[1]);
        v.push(3);
        assert_eq!(3, v.len());
        assert_eq!(3, v[2]);
    }

    #[test]
    fn test_from_iter() {
        let v: OneVector<isize> = (vec![1, 2, 3]).into_iter().collect();
        assert_eq!(3, v.len());
        assert_eq!(1, v[0]);
        assert_eq!(2, v[1]);
        assert_eq!(3, v[2]);
    }

    #[test]
    fn test_move_iter() {
        let v = OneVector::new();
        let v: Vec<isize> = v.into_iter().collect();
        assert_eq!(v, Vec::new());

        let v = OneVector::one(1);
        assert_eq!(v.into_iter().collect::<Vec<_>>(), [1]);

        let v = OneVector::many(vec![1, 2, 3]);
        assert_eq!(v.into_iter().collect::<Vec<_>>(), [1, 2, 3]);
    }

    #[test]
    #[should_panic]
    fn test_expect_one_zero() {
        let _: isize = OneVector::new().expect_one("");
    }

    #[test]
    #[should_panic]
    fn test_expect_one_many() {
        OneVector::many(vec![1, 2]).expect_one("");
    }

    #[test]
    fn test_expect_one_one() {
        assert_eq!(1, OneVector::one(1).expect_one(""));
        assert_eq!(1, OneVector::many(vec![1]).expect_one(""));
    }

    #[bench]
    fn fill_small_vec_1_10_with_cap(b: &mut Bencher) {
        b.iter(|| {
            let mut sv: SmallVec<[usize; 1]> = SmallVec::with_capacity(10);

            sv.extend(0..10);
        })
    }

    #[bench]
    fn fill_small_vec_1_10_wo_cap(b: &mut Bencher) {
        b.iter(|| {
            let mut sv: SmallVec<[usize; 1]> = SmallVec::new();

            sv.extend(0..10);
        })
    }

    #[bench]
    fn fill_small_vec_8_10_with_cap(b: &mut Bencher) {
        b.iter(|| {
            let mut sv: SmallVec<[usize; 8]> = SmallVec::with_capacity(10);

            sv.extend(0..10);
        })
    }

    #[bench]
    fn fill_small_vec_8_10_wo_cap(b: &mut Bencher) {
        b.iter(|| {
            let mut sv: SmallVec<[usize; 8]> = SmallVec::new();

            sv.extend(0..10);
        })
    }

    #[bench]
    fn fill_small_vec_32_10_with_cap(b: &mut Bencher) {
        b.iter(|| {
            let mut sv: SmallVec<[usize; 32]> = SmallVec::with_capacity(10);

            sv.extend(0..10);
        })
    }

    #[bench]
    fn fill_small_vec_32_10_wo_cap(b: &mut Bencher) {
        b.iter(|| {
            let mut sv: SmallVec<[usize; 32]> = SmallVec::new();

            sv.extend(0..10);
        })
    }

    #[bench]
    fn fill_small_vec_1_50_with_cap(b: &mut Bencher) {
        b.iter(|| {
            let mut sv: SmallVec<[usize; 1]> = SmallVec::with_capacity(50);

            sv.extend(0..50);
        })
    }

    #[bench]
    fn fill_small_vec_1_50_wo_cap(b: &mut Bencher) {
        b.iter(|| {
            let mut sv: SmallVec<[usize; 1]> = SmallVec::new();

            sv.extend(0..50);
        })
    }

    #[bench]
    fn fill_small_vec_8_50_with_cap(b: &mut Bencher) {
        b.iter(|| {
            let mut sv: SmallVec<[usize; 8]> = SmallVec::with_capacity(50);

            sv.extend(0..50);
        })
    }

    #[bench]
    fn fill_small_vec_8_50_wo_cap(b: &mut Bencher) {
        b.iter(|| {
            let mut sv: SmallVec<[usize; 8]> = SmallVec::new();

            sv.extend(0..50);
        })
    }

    #[bench]
    fn fill_small_vec_32_50_with_cap(b: &mut Bencher) {
        b.iter(|| {
            let mut sv: SmallVec<[usize; 32]> = SmallVec::with_capacity(50);

            sv.extend(0..50);
        })
    }

    #[bench]
    fn fill_small_vec_32_50_wo_cap(b: &mut Bencher) {
        b.iter(|| {
            let mut sv: SmallVec<[usize; 32]> = SmallVec::new();

            sv.extend(0..50);
        })
    }
}
