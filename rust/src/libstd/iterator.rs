// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*! Composable external iterators

The `Iterator` trait defines an interface for objects which implement iteration as a state machine.

Algorithms like `zip` are provided as `Iterator` implementations which wrap other objects
implementing the `Iterator` trait.

*/

use cmp;
use num::{Zero, One, Integer, Saturating};
use option::{Option, Some, None};
use ops::{Add, Mul, Sub};
use cmp::Ord;
use clone::Clone;
use uint;
use util;

/// Conversion from an `Iterator`
pub trait FromIterator<A> {
    /// Build a container with elements from an external iterator.
    fn from_iterator<T: Iterator<A>>(iterator: &mut T) -> Self;
}

/// A type growable from an `Iterator` implementation
pub trait Extendable<A>: FromIterator<A> {
    /// Extend a container with the elements yielded by an iterator
    fn extend<T: Iterator<A>>(&mut self, iterator: &mut T);
}

/// An interface for dealing with "external iterators". These types of iterators
/// can be resumed at any time as all state is stored internally as opposed to
/// being located on the call stack.
pub trait Iterator<A> {
    /// Advance the iterator and return the next value. Return `None` when the end is reached.
    fn next(&mut self) -> Option<A>;

    /// Return a lower bound and upper bound on the remaining length of the iterator.
    ///
    /// The common use case for the estimate is pre-allocating space to store the results.
    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) { (0, None) }

    /// Chain this iterator with another, returning a new iterator which will
    /// finish iterating over the current iterator, and then it will iterate
    /// over the other specified iterator.
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// let a = [0];
    /// let b = [1];
    /// let mut it = a.iter().chain(b.iter());
    /// assert_eq!(it.next().get(), &0);
    /// assert_eq!(it.next().get(), &1);
    /// assert!(it.next().is_none());
    /// ~~~
    #[inline]
    fn chain<U: Iterator<A>>(self, other: U) -> Chain<Self, U> {
        Chain{a: self, b: other, flag: false}
    }

    /// Creates an iterator which iterates over both this and the specified
    /// iterators simultaneously, yielding the two elements as pairs. When
    /// either iterator returns None, all further invocations of next() will
    /// return None.
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// let a = [0];
    /// let b = [1];
    /// let mut it = a.iter().zip(b.iter());
    /// assert_eq!(it.next().get(), (&0, &1));
    /// assert!(it.next().is_none());
    /// ~~~
    #[inline]
    fn zip<B, U: Iterator<B>>(self, other: U) -> Zip<Self, U> {
        Zip{a: self, b: other}
    }

    /// Creates a new iterator which will apply the specified function to each
    /// element returned by the first, yielding the mapped element instead.
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// let a = [1, 2];
    /// let mut it = a.iter().map(|&x| 2 * x);
    /// assert_eq!(it.next().get(), 2);
    /// assert_eq!(it.next().get(), 4);
    /// assert!(it.next().is_none());
    /// ~~~
    #[inline]
    fn map<'r, B>(self, f: &'r fn(A) -> B) -> Map<'r, A, B, Self> {
        Map{iter: self, f: f}
    }

    /// Creates an iterator which applies the predicate to each element returned
    /// by this iterator. Only elements which have the predicate evaluate to
    /// `true` will be yielded.
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// let a = [1, 2];
    /// let mut it = a.iter().filter(|&x| *x > 1);
    /// assert_eq!(it.next().get(), &2);
    /// assert!(it.next().is_none());
    /// ~~~
    #[inline]
    fn filter<'r>(self, predicate: &'r fn(&A) -> bool) -> Filter<'r, A, Self> {
        Filter{iter: self, predicate: predicate}
    }

    /// Creates an iterator which both filters and maps elements.
    /// If the specified function returns None, the element is skipped.
    /// Otherwise the option is unwrapped and the new value is yielded.
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// let a = [1, 2];
    /// let mut it = a.iter().filter_map(|&x| if x > 1 {Some(2 * x)} else {None});
    /// assert_eq!(it.next().get(), 4);
    /// assert!(it.next().is_none());
    /// ~~~
    #[inline]
    fn filter_map<'r, B>(self, f: &'r fn(A) -> Option<B>) -> FilterMap<'r, A, B, Self> {
        FilterMap { iter: self, f: f }
    }

    /// Creates an iterator which yields a pair of the value returned by this
    /// iterator plus the current index of iteration.
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// let a = [100, 200];
    /// let mut it = a.iter().enumerate();
    /// assert_eq!(it.next().get(), (0, &100));
    /// assert_eq!(it.next().get(), (1, &200));
    /// assert!(it.next().is_none());
    /// ~~~
    #[inline]
    fn enumerate(self) -> Enumerate<Self> {
        Enumerate{iter: self, count: 0}
    }


    /// Creates an iterator that has a `.peek()` method
    /// that returns a optional reference to the next element.
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// let a = [100, 200, 300];
    /// let mut it = xs.iter().map(|&x|x).peekable();
    /// assert_eq!(it.peek().unwrap(), &100);
    /// assert_eq!(it.next().unwrap(), 100);
    /// assert_eq!(it.next().unwrap(), 200);
    /// assert_eq!(it.peek().unwrap(), &300);
    /// assert_eq!(it.peek().unwrap(), &300);
    /// assert_eq!(it.next().unwrap(), 300);
    /// assert!(it.peek().is_none());
    /// assert!(it.next().is_none());
    /// ~~~
    #[inline]
    fn peekable(self) -> Peekable<A, Self> {
        Peekable{iter: self, peeked: None}
    }

    /// Creates an iterator which invokes the predicate on elements until it
    /// returns false. Once the predicate returns false, all further elements are
    /// yielded.
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// let a = [1, 2, 3, 2, 1];
    /// let mut it = a.iter().skip_while(|&a| *a < 3);
    /// assert_eq!(it.next().get(), &3);
    /// assert_eq!(it.next().get(), &2);
    /// assert_eq!(it.next().get(), &1);
    /// assert!(it.next().is_none());
    /// ~~~
    #[inline]
    fn skip_while<'r>(self, predicate: &'r fn(&A) -> bool) -> SkipWhile<'r, A, Self> {
        SkipWhile{iter: self, flag: false, predicate: predicate}
    }

    /// Creates an iterator which yields elements so long as the predicate
    /// returns true. After the predicate returns false for the first time, no
    /// further elements will be yielded.
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// let a = [1, 2, 3, 2, 1];
    /// let mut it = a.iter().take_while(|&a| *a < 3);
    /// assert_eq!(it.next().get(), &1);
    /// assert_eq!(it.next().get(), &2);
    /// assert!(it.next().is_none());
    /// ~~~
    #[inline]
    fn take_while<'r>(self, predicate: &'r fn(&A) -> bool) -> TakeWhile<'r, A, Self> {
        TakeWhile{iter: self, flag: false, predicate: predicate}
    }

    /// Creates an iterator which skips the first `n` elements of this iterator,
    /// and then it yields all further items.
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// let a = [1, 2, 3, 4, 5];
    /// let mut it = a.iter().skip(3);
    /// assert_eq!(it.next().get(), &4);
    /// assert_eq!(it.next().get(), &5);
    /// assert!(it.next().is_none());
    /// ~~~
    #[inline]
    fn skip(self, n: uint) -> Skip<Self> {
        Skip{iter: self, n: n}
    }

    /// Creates an iterator which yields the first `n` elements of this
    /// iterator, and then it will always return None.
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// let a = [1, 2, 3, 4, 5];
    /// let mut it = a.iter().take(3);
    /// assert_eq!(it.next().get(), &1);
    /// assert_eq!(it.next().get(), &2);
    /// assert_eq!(it.next().get(), &3);
    /// assert!(it.next().is_none());
    /// ~~~
    #[inline]
    fn take(self, n: uint) -> Take<Self> {
        Take{iter: self, n: n}
    }

    /// Creates a new iterator which behaves in a similar fashion to foldl.
    /// There is a state which is passed between each iteration and can be
    /// mutated as necessary. The yielded values from the closure are yielded
    /// from the Scan instance when not None.
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// let a = [1, 2, 3, 4, 5];
    /// let mut it = a.iter().scan(1, |fac, &x| {
    ///   *fac = *fac * x;
    ///   Some(*fac)
    /// });
    /// assert_eq!(it.next().get(), 1);
    /// assert_eq!(it.next().get(), 2);
    /// assert_eq!(it.next().get(), 6);
    /// assert_eq!(it.next().get(), 24);
    /// assert_eq!(it.next().get(), 120);
    /// assert!(it.next().is_none());
    /// ~~~
    #[inline]
    fn scan<'r, St, B>(self, initial_state: St, f: &'r fn(&mut St, A) -> Option<B>)
        -> Scan<'r, A, B, Self, St> {
        Scan{iter: self, f: f, state: initial_state}
    }

    /// Creates an iterator that maps each element to an iterator,
    /// and yields the elements of the produced iterators
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// let xs = [2u, 3];
    /// let ys = [0u, 1, 0, 1, 2];
    /// let mut it = xs.iter().flat_map(|&x| count(0u, 1).take(x));
    /// // Check that `it` has the same elements as `ys`
    /// let mut i = 0;
    /// for x: uint in it {
    ///     assert_eq!(x, ys[i]);
    ///     i += 1;
    /// }
    /// ~~~
    #[inline]
    fn flat_map<'r, B, U: Iterator<B>>(self, f: &'r fn(A) -> U)
        -> FlatMap<'r, A, Self, U> {
        FlatMap{iter: self, f: f, frontiter: None, backiter: None }
    }

    /// Creates an iterator that calls a function with a reference to each
    /// element before yielding it. This is often useful for debugging an
    /// iterator pipeline.
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    ///let xs = [1u, 4, 2, 3, 8, 9, 6];
    ///let sum = xs.iter()
    ///            .map(|&x| x)
    ///            .inspect(|&x| debug!("filtering %u", x))
    ///            .filter(|&x| x % 2 == 0)
    ///            .inspect(|&x| debug!("%u made it through", x))
    ///            .sum();
    ///println(sum.to_str());
    /// ~~~
    #[inline]
    fn inspect<'r>(self, f: &'r fn(&A)) -> Inspect<'r, A, Self> {
        Inspect{iter: self, f: f}
    }

    /// An adaptation of an external iterator to the for-loop protocol of rust.
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// use std::iterator::Counter;
    ///
    /// for i in count(0, 10) {
    ///     printfln!("%d", i);
    /// }
    /// ~~~
    #[inline]
    fn advance(&mut self, f: &fn(A) -> bool) -> bool {
        loop {
            match self.next() {
                Some(x) => {
                    if !f(x) { return false; }
                }
                None => { return true; }
            }
        }
    }

    /// Loops through the entire iterator, collecting all of the elements into
    /// a container implementing `FromIterator`.
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// let a = [1, 2, 3, 4, 5];
    /// let b: ~[int] = a.iter().map(|&x| x).collect();
    /// assert!(a == b);
    /// ~~~
    #[inline]
    fn collect<B: FromIterator<A>>(&mut self) -> B {
        FromIterator::from_iterator(self)
    }

    /// Loops through the entire iterator, collecting all of the elements into
    /// a unique vector. This is simply collect() specialized for vectors.
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// let a = [1, 2, 3, 4, 5];
    /// let b: ~[int] = a.iter().map(|&x| x).to_owned_vec();
    /// assert!(a == b);
    /// ~~~
    #[inline]
    fn to_owned_vec(&mut self) -> ~[A] {
        self.collect()
    }

    /// Loops through `n` iterations, returning the `n`th element of the
    /// iterator.
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// let a = [1, 2, 3, 4, 5];
    /// let mut it = a.iter();
    /// assert!(it.nth(2).get() == &3);
    /// assert!(it.nth(2) == None);
    /// ~~~
    #[inline]
    fn nth(&mut self, mut n: uint) -> Option<A> {
        loop {
            match self.next() {
                Some(x) => if n == 0 { return Some(x) },
                None => return None
            }
            n -= 1;
        }
    }

    /// Loops through the entire iterator, returning the last element of the
    /// iterator.
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// let a = [1, 2, 3, 4, 5];
    /// assert!(a.iter().last().get() == &5);
    /// ~~~
    #[inline]
    fn last(&mut self) -> Option<A> {
        let mut last = None;
        for x in *self { last = Some(x); }
        last
    }

    /// Performs a fold operation over the entire iterator, returning the
    /// eventual state at the end of the iteration.
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// let a = [1, 2, 3, 4, 5];
    /// assert!(a.iter().fold(0, |a, &b| a + b) == 15);
    /// ~~~
    #[inline]
    fn fold<B>(&mut self, init: B, f: &fn(B, A) -> B) -> B {
        let mut accum = init;
        loop {
            match self.next() {
                Some(x) => { accum = f(accum, x); }
                None    => { break; }
            }
        }
        accum
    }

    /// Counts the number of elements in this iterator.
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// let a = [1, 2, 3, 4, 5];
    /// let mut it = a.iter();
    /// assert!(it.len() == 5);
    /// assert!(it.len() == 0);
    /// ~~~
    #[inline]
    fn len(&mut self) -> uint {
        self.fold(0, |cnt, _x| cnt + 1)
    }

    /// Tests whether the predicate holds true for all elements in the iterator.
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// let a = [1, 2, 3, 4, 5];
    /// assert!(a.iter().all(|&x| *x > 0));
    /// assert!(!a.iter().all(|&x| *x > 2));
    /// ~~~
    #[inline]
    fn all(&mut self, f: &fn(A) -> bool) -> bool {
        for x in *self { if !f(x) { return false; } }
        true
    }

    /// Tests whether any element of an iterator satisfies the specified
    /// predicate.
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// let a = [1, 2, 3, 4, 5];
    /// let mut it = a.iter();
    /// assert!(it.any(|&x| *x == 3));
    /// assert!(!it.any(|&x| *x == 3));
    /// ~~~
    #[inline]
    fn any(&mut self, f: &fn(A) -> bool) -> bool {
        for x in *self { if f(x) { return true; } }
        false
    }

    /// Return the first element satisfying the specified predicate
    #[inline]
    fn find(&mut self, predicate: &fn(&A) -> bool) -> Option<A> {
        for x in *self {
            if predicate(&x) { return Some(x) }
        }
        None
    }

    /// Return the index of the first element satisfying the specified predicate
    #[inline]
    fn position(&mut self, predicate: &fn(A) -> bool) -> Option<uint> {
        let mut i = 0;
        for x in *self {
            if predicate(x) {
                return Some(i);
            }
            i += 1;
        }
        None
    }

    /// Count the number of elements satisfying the specified predicate
    #[inline]
    fn count(&mut self, predicate: &fn(A) -> bool) -> uint {
        let mut i = 0;
        for x in *self {
            if predicate(x) { i += 1 }
        }
        i
    }

    /// Return the element that gives the maximum value from the
    /// specified function.
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// let xs = [-3, 0, 1, 5, -10];
    /// assert_eq!(*xs.iter().max_by(|x| x.abs()).unwrap(), -10);
    /// ~~~
    #[inline]
    fn max_by<B: Ord>(&mut self, f: &fn(&A) -> B) -> Option<A> {
        self.fold(None, |max: Option<(A, B)>, x| {
            let x_val = f(&x);
            match max {
                None             => Some((x, x_val)),
                Some((y, y_val)) => if x_val > y_val {
                    Some((x, x_val))
                } else {
                    Some((y, y_val))
                }
            }
        }).map_move(|(x, _)| x)
    }

    /// Return the element that gives the minimum value from the
    /// specified function.
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// let xs = [-3, 0, 1, 5, -10];
    /// assert_eq!(*xs.iter().min_by(|x| x.abs()).unwrap(), 0);
    /// ~~~
    #[inline]
    fn min_by<B: Ord>(&mut self, f: &fn(&A) -> B) -> Option<A> {
        self.fold(None, |min: Option<(A, B)>, x| {
            let x_val = f(&x);
            match min {
                None             => Some((x, x_val)),
                Some((y, y_val)) => if x_val < y_val {
                    Some((x, x_val))
                } else {
                    Some((y, y_val))
                }
            }
        }).map_move(|(x, _)| x)
    }
}

/// A range iterator able to yield elements from both ends
pub trait DoubleEndedIterator<A>: Iterator<A> {
    /// Yield an element from the end of the range, returning `None` if the range is empty.
    fn next_back(&mut self) -> Option<A>;

    /// Flip the direction of the iterator
    ///
    /// The inverted iterator flips the ends on an iterator that can already
    /// be iterated from the front and from the back.
    ///
    ///
    /// If the iterator also implements RandomAccessIterator, the inverted
    /// iterator is also random access, with the indices starting at the back
    /// of the original iterator.
    ///
    /// Note: Random access with inverted indices still only applies to the first
    /// `uint::max_value` elements of the original iterator.
    #[inline]
    fn invert(self) -> Invert<Self> {
        Invert{iter: self}
    }
}

/// A double-ended iterator yielding mutable references
pub trait MutableDoubleEndedIterator {
    // FIXME: #5898: should be called `reverse`
    /// Use an iterator to reverse a container in-place
    fn reverse_(&mut self);
}

impl<'self, A, T: DoubleEndedIterator<&'self mut A>> MutableDoubleEndedIterator for T {
    // FIXME: #5898: should be called `reverse`
    /// Use an iterator to reverse a container in-place
    fn reverse_(&mut self) {
        loop {
            match (self.next(), self.next_back()) {
                (Some(x), Some(y)) => util::swap(x, y),
                _ => break
            }
        }
    }
}

/// An object implementing random access indexing by `uint`
///
/// A `RandomAccessIterator` should be either infinite or a `DoubleEndedIterator`.
pub trait RandomAccessIterator<A>: Iterator<A> {
    /// Return the number of indexable elements. At most `std::uint::max_value`
    /// elements are indexable, even if the iterator represents a longer range.
    fn indexable(&self) -> uint;

    /// Return an element at an index
    fn idx(&self, index: uint) -> Option<A>;
}

/// An double-ended iterator with the direction inverted
#[deriving(Clone)]
pub struct Invert<T> {
    priv iter: T
}

impl<A, T: DoubleEndedIterator<A>> Iterator<A> for Invert<T> {
    #[inline]
    fn next(&mut self) -> Option<A> { self.iter.next_back() }
    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) { self.iter.size_hint() }
}

impl<A, T: DoubleEndedIterator<A>> DoubleEndedIterator<A> for Invert<T> {
    #[inline]
    fn next_back(&mut self) -> Option<A> { self.iter.next() }
}

impl<A, T: DoubleEndedIterator<A> + RandomAccessIterator<A>> RandomAccessIterator<A>
    for Invert<T> {
    #[inline]
    fn indexable(&self) -> uint { self.iter.indexable() }
    #[inline]
    fn idx(&self, index: uint) -> Option<A> {
        self.iter.idx(self.indexable() - index - 1)
    }
}

/// A trait for iterators over elements which can be added together
pub trait AdditiveIterator<A> {
    /// Iterates over the entire iterator, summing up all the elements
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// let a = [1, 2, 3, 4, 5];
    /// let mut it = a.iter().map(|&x| x);
    /// assert!(it.sum() == 15);
    /// ~~~
    fn sum(&mut self) -> A;
}

impl<A: Add<A, A> + Zero, T: Iterator<A>> AdditiveIterator<A> for T {
    #[inline]
    fn sum(&mut self) -> A { self.fold(Zero::zero::<A>(), |s, x| s + x) }
}

/// A trait for iterators over elements whose elements can be multiplied
/// together.
pub trait MultiplicativeIterator<A> {
    /// Iterates over the entire iterator, multiplying all the elements
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// use std::iterator::Counter;
    ///
    /// fn factorial(n: uint) -> uint {
    ///     count(1u, 1).take_while(|&i| i <= n).product()
    /// }
    /// assert!(factorial(0) == 1);
    /// assert!(factorial(1) == 1);
    /// assert!(factorial(5) == 120);
    /// ~~~
    fn product(&mut self) -> A;
}

impl<A: Mul<A, A> + One, T: Iterator<A>> MultiplicativeIterator<A> for T {
    #[inline]
    fn product(&mut self) -> A { self.fold(One::one::<A>(), |p, x| p * x) }
}

/// A trait for iterators over elements which can be compared to one another.
/// The type of each element must ascribe to the `Ord` trait.
pub trait OrdIterator<A> {
    /// Consumes the entire iterator to return the maximum element.
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// let a = [1, 2, 3, 4, 5];
    /// assert!(a.iter().max().get() == &5);
    /// ~~~
    fn max(&mut self) -> Option<A>;

    /// Consumes the entire iterator to return the minimum element.
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// let a = [1, 2, 3, 4, 5];
    /// assert!(a.iter().min().get() == &1);
    /// ~~~
    fn min(&mut self) -> Option<A>;
}

impl<A: Ord, T: Iterator<A>> OrdIterator<A> for T {
    #[inline]
    fn max(&mut self) -> Option<A> {
        self.fold(None, |max, x| {
            match max {
                None    => Some(x),
                Some(y) => Some(cmp::max(x, y))
            }
        })
    }

    #[inline]
    fn min(&mut self) -> Option<A> {
        self.fold(None, |min, x| {
            match min {
                None    => Some(x),
                Some(y) => Some(cmp::min(x, y))
            }
        })
    }
}

/// A trait for iterators that are clonable.
pub trait ClonableIterator {
    /// Repeats an iterator endlessly
    ///
    /// # Example
    ///
    /// ~~~ {.rust}
    /// let a = count(1,1).take(1);
    /// let mut cy = a.cycle();
    /// assert_eq!(cy.next(), Some(1));
    /// assert_eq!(cy.next(), Some(1));
    /// ~~~
    fn cycle(self) -> Cycle<Self>;
}

impl<A, T: Clone + Iterator<A>> ClonableIterator for T {
    #[inline]
    fn cycle(self) -> Cycle<T> {
        Cycle{orig: self.clone(), iter: self}
    }
}

/// An iterator that repeats endlessly
#[deriving(Clone)]
pub struct Cycle<T> {
    priv orig: T,
    priv iter: T,
}

impl<A, T: Clone + Iterator<A>> Iterator<A> for Cycle<T> {
    #[inline]
    fn next(&mut self) -> Option<A> {
        match self.iter.next() {
            None => { self.iter = self.orig.clone(); self.iter.next() }
            y => y
        }
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        // the cycle iterator is either empty or infinite
        match self.orig.size_hint() {
            sz @ (0, Some(0)) => sz,
            (0, _) => (0, None),
            _ => (uint::max_value, None)
        }
    }
}

impl<A, T: Clone + RandomAccessIterator<A>> RandomAccessIterator<A> for Cycle<T> {
    #[inline]
    fn indexable(&self) -> uint {
        if self.orig.indexable() > 0 {
            uint::max_value
        } else {
            0
        }
    }

    #[inline]
    fn idx(&self, index: uint) -> Option<A> {
        let liter = self.iter.indexable();
        let lorig = self.orig.indexable();
        if lorig == 0 {
            None
        } else if index < liter {
            self.iter.idx(index)
        } else {
            self.orig.idx((index - liter) % lorig)
        }
    }
}

/// An iterator which strings two iterators together
#[deriving(Clone)]
pub struct Chain<T, U> {
    priv a: T,
    priv b: U,
    priv flag: bool
}

impl<A, T: Iterator<A>, U: Iterator<A>> Iterator<A> for Chain<T, U> {
    #[inline]
    fn next(&mut self) -> Option<A> {
        if self.flag {
            self.b.next()
        } else {
            match self.a.next() {
                Some(x) => return Some(x),
                _ => ()
            }
            self.flag = true;
            self.b.next()
        }
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        let (a_lower, a_upper) = self.a.size_hint();
        let (b_lower, b_upper) = self.b.size_hint();

        let lower = a_lower.saturating_add(b_lower);

        let upper = match (a_upper, b_upper) {
            (Some(x), Some(y)) => Some(x.saturating_add(y)),
            _ => None
        };

        (lower, upper)
    }
}

impl<A, T: DoubleEndedIterator<A>, U: DoubleEndedIterator<A>> DoubleEndedIterator<A>
for Chain<T, U> {
    #[inline]
    fn next_back(&mut self) -> Option<A> {
        match self.b.next_back() {
            Some(x) => Some(x),
            None => self.a.next_back()
        }
    }
}

impl<A, T: RandomAccessIterator<A>, U: RandomAccessIterator<A>> RandomAccessIterator<A>
for Chain<T, U> {
    #[inline]
    fn indexable(&self) -> uint {
        let (a, b) = (self.a.indexable(), self.b.indexable());
        a.saturating_add(b)
    }

    #[inline]
    fn idx(&self, index: uint) -> Option<A> {
        let len = self.a.indexable();
        if index < len {
            self.a.idx(index)
        } else {
            self.b.idx(index - len)
        }
    }
}

/// An iterator which iterates two other iterators simultaneously
#[deriving(Clone)]
pub struct Zip<T, U> {
    priv a: T,
    priv b: U
}

impl<A, B, T: Iterator<A>, U: Iterator<B>> Iterator<(A, B)> for Zip<T, U> {
    #[inline]
    fn next(&mut self) -> Option<(A, B)> {
        match (self.a.next(), self.b.next()) {
            (Some(x), Some(y)) => Some((x, y)),
            _ => None
        }
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        let (a_lower, a_upper) = self.a.size_hint();
        let (b_lower, b_upper) = self.b.size_hint();

        let lower = cmp::min(a_lower, b_lower);

        let upper = match (a_upper, b_upper) {
            (Some(x), Some(y)) => Some(cmp::min(x,y)),
            (Some(x), None) => Some(x),
            (None, Some(y)) => Some(y),
            (None, None) => None
        };

        (lower, upper)
    }
}

impl<A, B, T: RandomAccessIterator<A>, U: RandomAccessIterator<B>>
RandomAccessIterator<(A, B)> for Zip<T, U> {
    #[inline]
    fn indexable(&self) -> uint {
        cmp::min(self.a.indexable(), self.b.indexable())
    }

    #[inline]
    fn idx(&self, index: uint) -> Option<(A, B)> {
        match (self.a.idx(index), self.b.idx(index)) {
            (Some(x), Some(y)) => Some((x, y)),
            _ => None
        }
    }
}

/// An iterator which maps the values of `iter` with `f`
pub struct Map<'self, A, B, T> {
    priv iter: T,
    priv f: &'self fn(A) -> B
}

impl<'self, A, B, T> Map<'self, A, B, T> {
    #[inline]
    fn do_map(&self, elt: Option<A>) -> Option<B> {
        match elt {
            Some(a) => Some((self.f)(a)),
            _ => None
        }
    }
}

impl<'self, A, B, T: Iterator<A>> Iterator<B> for Map<'self, A, B, T> {
    #[inline]
    fn next(&mut self) -> Option<B> {
        let next = self.iter.next();
        self.do_map(next)
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        self.iter.size_hint()
    }
}

impl<'self, A, B, T: DoubleEndedIterator<A>> DoubleEndedIterator<B> for Map<'self, A, B, T> {
    #[inline]
    fn next_back(&mut self) -> Option<B> {
        let next = self.iter.next_back();
        self.do_map(next)
    }
}

impl<'self, A, B, T: RandomAccessIterator<A>> RandomAccessIterator<B> for Map<'self, A, B, T> {
    #[inline]
    fn indexable(&self) -> uint {
        self.iter.indexable()
    }

    #[inline]
    fn idx(&self, index: uint) -> Option<B> {
        self.do_map(self.iter.idx(index))
    }
}

/// An iterator which filters the elements of `iter` with `predicate`
pub struct Filter<'self, A, T> {
    priv iter: T,
    priv predicate: &'self fn(&A) -> bool
}

impl<'self, A, T: Iterator<A>> Iterator<A> for Filter<'self, A, T> {
    #[inline]
    fn next(&mut self) -> Option<A> {
        for x in self.iter {
            if (self.predicate)(&x) {
                return Some(x);
            } else {
                loop
            }
        }
        None
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        let (_, upper) = self.iter.size_hint();
        (0, upper) // can't know a lower bound, due to the predicate
    }
}

impl<'self, A, T: DoubleEndedIterator<A>> DoubleEndedIterator<A> for Filter<'self, A, T> {
    #[inline]
    fn next_back(&mut self) -> Option<A> {
        loop {
            match self.iter.next_back() {
                None => return None,
                Some(x) => {
                    if (self.predicate)(&x) {
                        return Some(x);
                    } else {
                        loop
                    }
                }
            }
        }
    }
}

/// An iterator which uses `f` to both filter and map elements from `iter`
pub struct FilterMap<'self, A, B, T> {
    priv iter: T,
    priv f: &'self fn(A) -> Option<B>
}

impl<'self, A, B, T: Iterator<A>> Iterator<B> for FilterMap<'self, A, B, T> {
    #[inline]
    fn next(&mut self) -> Option<B> {
        for x in self.iter {
            match (self.f)(x) {
                Some(y) => return Some(y),
                None => ()
            }
        }
        None
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        let (_, upper) = self.iter.size_hint();
        (0, upper) // can't know a lower bound, due to the predicate
    }
}

impl<'self, A, B, T: DoubleEndedIterator<A>> DoubleEndedIterator<B>
for FilterMap<'self, A, B, T> {
    #[inline]
    fn next_back(&mut self) -> Option<B> {
        loop {
            match self.iter.next_back() {
                None => return None,
                Some(x) => {
                    match (self.f)(x) {
                        Some(y) => return Some(y),
                        None => ()
                    }
                }
            }
        }
    }
}

/// An iterator which yields the current count and the element during iteration
#[deriving(Clone)]
pub struct Enumerate<T> {
    priv iter: T,
    priv count: uint
}

impl<A, T: Iterator<A>> Iterator<(uint, A)> for Enumerate<T> {
    #[inline]
    fn next(&mut self) -> Option<(uint, A)> {
        match self.iter.next() {
            Some(a) => {
                let ret = Some((self.count, a));
                self.count += 1;
                ret
            }
            _ => None
        }
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        self.iter.size_hint()
    }
}

impl<A, T: RandomAccessIterator<A>> RandomAccessIterator<(uint, A)> for Enumerate<T> {
    #[inline]
    fn indexable(&self) -> uint {
        self.iter.indexable()
    }

    #[inline]
    fn idx(&self, index: uint) -> Option<(uint, A)> {
        match self.iter.idx(index) {
            Some(a) => Some((self.count + index, a)),
            _ => None,
        }
    }
}

/// An iterator with a `peek()` that returns an optional reference to the next element.
pub struct Peekable<A, T> {
    priv iter: T,
    priv peeked: Option<A>,
}

impl<A, T: Iterator<A>> Iterator<A> for Peekable<A, T> {
    #[inline]
    fn next(&mut self) -> Option<A> {
        if self.peeked.is_some() { self.peeked.take() }
        else { self.iter.next() }
    }
}

impl<'self, A, T: Iterator<A>> Peekable<A, T> {
    /// Return a reference to the next element of the iterator with out advancing it,
    /// or None if the iterator is exhausted.
    #[inline]
    pub fn peek(&'self mut self) -> Option<&'self A> {
        match self.peeked {
            Some(ref value) => Some(value),
            None => {
                self.peeked = self.iter.next();
                match self.peeked {
                    Some(ref value) => Some(value),
                    None => None,
                }
            },
        }
    }
}

/// An iterator which rejects elements while `predicate` is true
pub struct SkipWhile<'self, A, T> {
    priv iter: T,
    priv flag: bool,
    priv predicate: &'self fn(&A) -> bool
}

impl<'self, A, T: Iterator<A>> Iterator<A> for SkipWhile<'self, A, T> {
    #[inline]
    fn next(&mut self) -> Option<A> {
        let mut next = self.iter.next();
        if self.flag {
            next
        } else {
            loop {
                match next {
                    Some(x) => {
                        if (self.predicate)(&x) {
                            next = self.iter.next();
                            loop
                        } else {
                            self.flag = true;
                            return Some(x)
                        }
                    }
                    None => return None
                }
            }
        }
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        let (_, upper) = self.iter.size_hint();
        (0, upper) // can't know a lower bound, due to the predicate
    }
}

/// An iterator which only accepts elements while `predicate` is true
pub struct TakeWhile<'self, A, T> {
    priv iter: T,
    priv flag: bool,
    priv predicate: &'self fn(&A) -> bool
}

impl<'self, A, T: Iterator<A>> Iterator<A> for TakeWhile<'self, A, T> {
    #[inline]
    fn next(&mut self) -> Option<A> {
        if self.flag {
            None
        } else {
            match self.iter.next() {
                Some(x) => {
                    if (self.predicate)(&x) {
                        Some(x)
                    } else {
                        self.flag = true;
                        None
                    }
                }
                None => None
            }
        }
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        let (_, upper) = self.iter.size_hint();
        (0, upper) // can't know a lower bound, due to the predicate
    }
}

/// An iterator which skips over `n` elements of `iter`.
#[deriving(Clone)]
pub struct Skip<T> {
    priv iter: T,
    priv n: uint
}

impl<A, T: Iterator<A>> Iterator<A> for Skip<T> {
    #[inline]
    fn next(&mut self) -> Option<A> {
        let mut next = self.iter.next();
        if self.n == 0 {
            next
        } else {
            let mut n = self.n;
            while n > 0 {
                n -= 1;
                match next {
                    Some(_) => {
                        next = self.iter.next();
                        loop
                    }
                    None => {
                        self.n = 0;
                        return None
                    }
                }
            }
            self.n = 0;
            next
        }
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        let (lower, upper) = self.iter.size_hint();

        let lower = lower.saturating_sub(self.n);

        let upper = match upper {
            Some(x) => Some(x.saturating_sub(self.n)),
            None => None
        };

        (lower, upper)
    }
}

impl<A, T: RandomAccessIterator<A>> RandomAccessIterator<A> for Skip<T> {
    #[inline]
    fn indexable(&self) -> uint {
        self.iter.indexable().saturating_sub(self.n)
    }

    #[inline]
    fn idx(&self, index: uint) -> Option<A> {
        if index >= self.indexable() {
            None
        } else {
            self.iter.idx(index + self.n)
        }
    }
}

/// An iterator which only iterates over the first `n` iterations of `iter`.
#[deriving(Clone)]
pub struct Take<T> {
    priv iter: T,
    priv n: uint
}

impl<A, T: Iterator<A>> Iterator<A> for Take<T> {
    #[inline]
    fn next(&mut self) -> Option<A> {
        if self.n != 0 {
            self.n -= 1;
            self.iter.next()
        } else {
            None
        }
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        let (lower, upper) = self.iter.size_hint();

        let lower = cmp::min(lower, self.n);

        let upper = match upper {
            Some(x) if x < self.n => Some(x),
            _ => Some(self.n)
        };

        (lower, upper)
    }
}

impl<A, T: RandomAccessIterator<A>> RandomAccessIterator<A> for Take<T> {
    #[inline]
    fn indexable(&self) -> uint {
        cmp::min(self.iter.indexable(), self.n)
    }

    #[inline]
    fn idx(&self, index: uint) -> Option<A> {
        if index >= self.n {
            None
        } else {
            self.iter.idx(index)
        }
    }
}


/// An iterator to maintain state while iterating another iterator
pub struct Scan<'self, A, B, T, St> {
    priv iter: T,
    priv f: &'self fn(&mut St, A) -> Option<B>,

    /// The current internal state to be passed to the closure next.
    state: St
}

impl<'self, A, B, T: Iterator<A>, St> Iterator<B> for Scan<'self, A, B, T, St> {
    #[inline]
    fn next(&mut self) -> Option<B> {
        self.iter.next().chain(|a| (self.f)(&mut self.state, a))
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        let (_, upper) = self.iter.size_hint();
        (0, upper) // can't know a lower bound, due to the scan function
    }
}

/// An iterator that maps each element to an iterator,
/// and yields the elements of the produced iterators
///
pub struct FlatMap<'self, A, T, U> {
    priv iter: T,
    priv f: &'self fn(A) -> U,
    priv frontiter: Option<U>,
    priv backiter: Option<U>,
}

impl<'self, A, T: Iterator<A>, B, U: Iterator<B>> Iterator<B> for
    FlatMap<'self, A, T, U> {
    #[inline]
    fn next(&mut self) -> Option<B> {
        loop {
            for inner in self.frontiter.mut_iter() {
                for x in *inner {
                    return Some(x)
                }
            }
            match self.iter.next().map_move(|x| (self.f)(x)) {
                None => return self.backiter.chain_mut_ref(|it| it.next()),
                next => self.frontiter = next,
            }
        }
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        let (flo, fhi) = self.frontiter.map_default((0, Some(0)), |it| it.size_hint());
        let (blo, bhi) = self.backiter.map_default((0, Some(0)), |it| it.size_hint());
        let lo = flo.saturating_add(blo);
        match (self.iter.size_hint(), fhi, bhi) {
            ((0, Some(0)), Some(a), Some(b)) => (lo, Some(a.saturating_add(b))),
            _ => (lo, None)
        }
    }
}

impl<'self,
     A, T: DoubleEndedIterator<A>,
     B, U: DoubleEndedIterator<B>> DoubleEndedIterator<B>
     for FlatMap<'self, A, T, U> {
    #[inline]
    fn next_back(&mut self) -> Option<B> {
        loop {
            for inner in self.backiter.mut_iter() {
                match inner.next_back() {
                    None => (),
                    y => return y
                }
            }
            match self.iter.next_back().map_move(|x| (self.f)(x)) {
                None => return self.frontiter.chain_mut_ref(|it| it.next_back()),
                next => self.backiter = next,
            }
        }
    }
}

/// An iterator that calls a function with a reference to each
/// element before yielding it.
pub struct Inspect<'self, A, T> {
    priv iter: T,
    priv f: &'self fn(&A)
}

impl<'self, A, T> Inspect<'self, A, T> {
    #[inline]
    fn do_inspect(&self, elt: Option<A>) -> Option<A> {
        match elt {
            Some(ref a) => (self.f)(a),
            None => ()
        }

        elt
    }
}

impl<'self, A, T: Iterator<A>> Iterator<A> for Inspect<'self, A, T> {
    #[inline]
    fn next(&mut self) -> Option<A> {
        let next = self.iter.next();
        self.do_inspect(next)
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        self.iter.size_hint()
    }
}

impl<'self, A, T: DoubleEndedIterator<A>> DoubleEndedIterator<A>
for Inspect<'self, A, T> {
    #[inline]
    fn next_back(&mut self) -> Option<A> {
        let next = self.iter.next_back();
        self.do_inspect(next)
    }
}

impl<'self, A, T: RandomAccessIterator<A>> RandomAccessIterator<A>
for Inspect<'self, A, T> {
    #[inline]
    fn indexable(&self) -> uint {
        self.iter.indexable()
    }

    #[inline]
    fn idx(&self, index: uint) -> Option<A> {
        self.do_inspect(self.iter.idx(index))
    }
}

/// An iterator which just modifies the contained state throughout iteration.
pub struct Unfoldr<'self, A, St> {
    priv f: &'self fn(&mut St) -> Option<A>,
    /// Internal state that will be yielded on the next iteration
    state: St
}

impl<'self, A, St> Unfoldr<'self, A, St> {
    /// Creates a new iterator with the specified closure as the "iterator
    /// function" and an initial state to eventually pass to the iterator
    #[inline]
    pub fn new<'a>(initial_state: St, f: &'a fn(&mut St) -> Option<A>)
        -> Unfoldr<'a, A, St> {
        Unfoldr {
            f: f,
            state: initial_state
        }
    }
}

impl<'self, A, St> Iterator<A> for Unfoldr<'self, A, St> {
    #[inline]
    fn next(&mut self) -> Option<A> {
        (self.f)(&mut self.state)
    }
}

/// An infinite iterator starting at `start` and advancing by `step` with each
/// iteration
#[deriving(Clone)]
pub struct Counter<A> {
    /// The current state the counter is at (next value to be yielded)
    state: A,
    /// The amount that this iterator is stepping by
    step: A
}

/// Creates a new counter with the specified start/step
#[inline]
pub fn count<A>(start: A, step: A) -> Counter<A> {
    Counter{state: start, step: step}
}

/// A range of numbers from [0, N)
#[deriving(Clone, DeepClone)]
pub struct Range<A> {
    priv state: A,
    priv stop: A,
    priv one: A
}

/// Return an iterator over the range [start, stop)
#[inline]
pub fn range<A: Add<A, A> + Ord + Clone + One>(start: A, stop: A) -> Range<A> {
    Range{state: start, stop: stop, one: One::one()}
}

impl<A: Add<A, A> + Ord + Clone> Iterator<A> for Range<A> {
    #[inline]
    fn next(&mut self) -> Option<A> {
        if self.state < self.stop {
            let result = self.state.clone();
            self.state = self.state + self.one;
            Some(result)
        } else {
            None
        }
    }
}

impl<A: Sub<A, A> + Integer + Ord + Clone> DoubleEndedIterator<A> for Range<A> {
    #[inline]
    fn next_back(&mut self) -> Option<A> {
        if self.stop > self.state {
            // Integer doesn't technically define this rule, but we're going to assume that every
            // Integer is reachable from every other one by adding or subtracting enough Ones. This
            // seems like a reasonable-enough rule that every Integer should conform to, even if it
            // can't be statically checked.
            self.stop = self.stop - self.one;
            Some(self.stop.clone())
        } else {
            None
        }
    }
}

/// A range of numbers from [0, N]
#[deriving(Clone, DeepClone)]
pub struct RangeInclusive<A> {
    priv range: Range<A>,
    priv done: bool
}

/// Return an iterator over the range [start, stop]
#[inline]
pub fn range_inclusive<A: Add<A, A> + Ord + Clone + One>(start: A, stop: A) -> RangeInclusive<A> {
    RangeInclusive{range: range(start, stop), done: false}
}

impl<A: Add<A, A> + Ord + Clone> Iterator<A> for RangeInclusive<A> {
    #[inline]
    fn next(&mut self) -> Option<A> {
        match self.range.next() {
            Some(x) => Some(x),
            None => {
                if self.done {
                    None
                } else {
                    self.done = true;
                    Some(self.range.stop.clone())
                }
            }
        }
    }
}

impl<A: Sub<A, A> + Integer + Ord + Clone> DoubleEndedIterator<A> for RangeInclusive<A> {
    #[inline]
    fn next_back(&mut self) -> Option<A> {
        if self.range.stop > self.range.state {
            let result = self.range.stop.clone();
            self.range.stop = self.range.stop - self.range.one;
            Some(result)
        } else if self.done {
            None
        } else {
            self.done = true;
            Some(self.range.stop.clone())
        }
    }
}

impl<A: Add<A, A> + Clone> Iterator<A> for Counter<A> {
    #[inline]
    fn next(&mut self) -> Option<A> {
        let result = self.state.clone();
        self.state = self.state + self.step;
        Some(result)
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        (uint::max_value, None) // Too bad we can't specify an infinite lower bound
    }
}

/// An iterator that repeats an element endlessly
#[deriving(Clone, DeepClone)]
pub struct Repeat<A> {
    priv element: A
}

impl<A: Clone> Repeat<A> {
    /// Create a new `Repeat` that endlessly repeats the element `elt`.
    #[inline]
    pub fn new(elt: A) -> Repeat<A> {
        Repeat{element: elt}
    }
}

impl<A: Clone> Iterator<A> for Repeat<A> {
    #[inline]
    fn next(&mut self) -> Option<A> { self.idx(0) }
    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) { (uint::max_value, None) }
}

impl<A: Clone> DoubleEndedIterator<A> for Repeat<A> {
    #[inline]
    fn next_back(&mut self) -> Option<A> { self.idx(0) }
}

impl<A: Clone> RandomAccessIterator<A> for Repeat<A> {
    #[inline]
    fn indexable(&self) -> uint { uint::max_value }
    #[inline]
    fn idx(&self, _: uint) -> Option<A> { Some(self.element.clone()) }
}

/// Functions for lexicographical ordering of sequences.
///
/// Lexicographical ordering through `<`, `<=`, `>=`, `>` requires
/// that the elements implement both `Eq` and `Ord`.
///
/// If two sequences are equal up until the point where one ends,
/// the shorter sequence compares less.
pub mod order {
    use cmp;
    use cmp::{TotalEq, TotalOrd, Ord, Eq};
    use option::{Some, None};
    use super::Iterator;

    /// Compare `a` and `b` for equality using `TotalOrd`
    pub fn equals<A: TotalEq, T: Iterator<A>>(mut a: T, mut b: T) -> bool {
        loop {
            match (a.next(), b.next()) {
                (None, None) => return true,
                (None, _) | (_, None) => return false,
                (Some(x), Some(y)) => if !x.equals(&y) { return false },
            }
        }
    }

    /// Order `a` and `b` lexicographically using `TotalOrd`
    pub fn cmp<A: TotalOrd, T: Iterator<A>>(mut a: T, mut b: T) -> cmp::Ordering {
        loop {
            match (a.next(), b.next()) {
                (None, None) => return cmp::Equal,
                (None, _   ) => return cmp::Less,
                (_   , None) => return cmp::Greater,
                (Some(x), Some(y)) => match x.cmp(&y) {
                    cmp::Equal => (),
                    non_eq => return non_eq,
                },
            }
        }
    }

    /// Compare `a` and `b` for equality (Using partial equality, `Eq`)
    pub fn eq<A: Eq, T: Iterator<A>>(mut a: T, mut b: T) -> bool {
        loop {
            match (a.next(), b.next()) {
                (None, None) => return true,
                (None, _) | (_, None) => return false,
                (Some(x), Some(y)) => if !x.eq(&y) { return false },
            }
        }
    }

    /// Compare `a` and `b` for nonequality (Using partial equality, `Eq`)
    pub fn ne<A: Eq, T: Iterator<A>>(mut a: T, mut b: T) -> bool {
        loop {
            match (a.next(), b.next()) {
                (None, None) => return false,
                (None, _) | (_, None) => return true,
                (Some(x), Some(y)) => if x.ne(&y) { return true },
            }
        }
    }

    /// Return `a` < `b` lexicographically (Using partial order, `Ord`)
    pub fn lt<A: Eq + Ord, T: Iterator<A>>(mut a: T, mut b: T) -> bool {
        loop {
            match (a.next(), b.next()) {
                (None, None) => return false,
                (None, _   ) => return true,
                (_   , None) => return false,
                (Some(x), Some(y)) => if x.ne(&y) { return x.lt(&y) },
            }
        }
    }

    /// Return `a` <= `b` lexicographically (Using partial order, `Ord`)
    pub fn le<A: Eq + Ord, T: Iterator<A>>(mut a: T, mut b: T) -> bool {
        loop {
            match (a.next(), b.next()) {
                (None, None) => return true,
                (None, _   ) => return true,
                (_   , None) => return false,
                (Some(x), Some(y)) => if x.ne(&y) { return x.le(&y) },
            }
        }
    }

    /// Return `a` > `b` lexicographically (Using partial order, `Ord`)
    pub fn gt<A: Eq + Ord, T: Iterator<A>>(mut a: T, mut b: T) -> bool {
        loop {
            match (a.next(), b.next()) {
                (None, None) => return false,
                (None, _   ) => return false,
                (_   , None) => return true,
                (Some(x), Some(y)) => if x.ne(&y) { return x.gt(&y) },
            }
        }
    }

    /// Return `a` >= `b` lexicographically (Using partial order, `Ord`)
    pub fn ge<A: Eq + Ord, T: Iterator<A>>(mut a: T, mut b: T) -> bool {
        loop {
            match (a.next(), b.next()) {
                (None, None) => return true,
                (None, _   ) => return false,
                (_   , None) => return true,
                (Some(x), Some(y)) => if x.ne(&y) { return x.ge(&y) },
            }
        }
    }

    #[test]
    fn test_lt() {
        use vec::ImmutableVector;

        let empty: [int, ..0] = [];
        let xs = [1,2,3];
        let ys = [1,2,0];

        assert!(!lt(xs.iter(), ys.iter()));
        assert!(!le(xs.iter(), ys.iter()));
        assert!( gt(xs.iter(), ys.iter()));
        assert!( ge(xs.iter(), ys.iter()));

        assert!( lt(ys.iter(), xs.iter()));
        assert!( le(ys.iter(), xs.iter()));
        assert!(!gt(ys.iter(), xs.iter()));
        assert!(!ge(ys.iter(), xs.iter()));

        assert!( lt(empty.iter(), xs.iter()));
        assert!( le(empty.iter(), xs.iter()));
        assert!(!gt(empty.iter(), xs.iter()));
        assert!(!ge(empty.iter(), xs.iter()));

        // Sequence with NaN
        let u = [1.0, 2.0];
        let v = [0.0/0.0, 3.0];

        assert!(!lt(u.iter(), v.iter()));
        assert!(!le(u.iter(), v.iter()));
        assert!(!gt(u.iter(), v.iter()));
        assert!(!ge(u.iter(), v.iter()));

        let a = [0.0/0.0];
        let b = [1.0];
        let c = [2.0];

        assert!(lt(a.iter(), b.iter()) == (a[0] <  b[0]));
        assert!(le(a.iter(), b.iter()) == (a[0] <= b[0]));
        assert!(gt(a.iter(), b.iter()) == (a[0] >  b[0]));
        assert!(ge(a.iter(), b.iter()) == (a[0] >= b[0]));

        assert!(lt(c.iter(), b.iter()) == (c[0] <  b[0]));
        assert!(le(c.iter(), b.iter()) == (c[0] <= b[0]));
        assert!(gt(c.iter(), b.iter()) == (c[0] >  b[0]));
        assert!(ge(c.iter(), b.iter()) == (c[0] >= b[0]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prelude::*;

    use cmp;
    use uint;

    #[test]
    fn test_counter_from_iter() {
        let mut it = count(0, 5).take(10);
        let xs: ~[int] = FromIterator::from_iterator(&mut it);
        assert_eq!(xs, ~[0, 5, 10, 15, 20, 25, 30, 35, 40, 45]);
    }

    #[test]
    fn test_iterator_chain() {
        let xs = [0u, 1, 2, 3, 4, 5];
        let ys = [30u, 40, 50, 60];
        let expected = [0, 1, 2, 3, 4, 5, 30, 40, 50, 60];
        let mut it = xs.iter().chain(ys.iter());
        let mut i = 0;
        for &x in it {
            assert_eq!(x, expected[i]);
            i += 1;
        }
        assert_eq!(i, expected.len());

        let ys = count(30u, 10).take(4);
        let mut it = xs.iter().map(|&x| x).chain(ys);
        let mut i = 0;
        for x in it {
            assert_eq!(x, expected[i]);
            i += 1;
        }
        assert_eq!(i, expected.len());
    }

    #[test]
    fn test_filter_map() {
        let mut it = count(0u, 1u).take(10)
            .filter_map(|x| if x.is_even() { Some(x*x) } else { None });
        assert_eq!(it.collect::<~[uint]>(), ~[0*0, 2*2, 4*4, 6*6, 8*8]);
    }

    #[test]
    fn test_iterator_enumerate() {
        let xs = [0u, 1, 2, 3, 4, 5];
        let mut it = xs.iter().enumerate();
        for (i, &x) in it {
            assert_eq!(i, x);
        }
    }

    #[test]
    fn test_iterator_peekable() {
        let xs = ~[0u, 1, 2, 3, 4, 5];
        let mut it = xs.iter().map(|&x|x).peekable();
        assert_eq!(it.peek().unwrap(), &0);
        assert_eq!(it.next().unwrap(), 0);
        assert_eq!(it.next().unwrap(), 1);
        assert_eq!(it.next().unwrap(), 2);
        assert_eq!(it.peek().unwrap(), &3);
        assert_eq!(it.peek().unwrap(), &3);
        assert_eq!(it.next().unwrap(), 3);
        assert_eq!(it.next().unwrap(), 4);
        assert_eq!(it.peek().unwrap(), &5);
        assert_eq!(it.next().unwrap(), 5);
        assert!(it.peek().is_none());
        assert!(it.next().is_none());
    }

    #[test]
    fn test_iterator_take_while() {
        let xs = [0u, 1, 2, 3, 5, 13, 15, 16, 17, 19];
        let ys = [0u, 1, 2, 3, 5, 13];
        let mut it = xs.iter().take_while(|&x| *x < 15u);
        let mut i = 0;
        for &x in it {
            assert_eq!(x, ys[i]);
            i += 1;
        }
        assert_eq!(i, ys.len());
    }

    #[test]
    fn test_iterator_skip_while() {
        let xs = [0u, 1, 2, 3, 5, 13, 15, 16, 17, 19];
        let ys = [15, 16, 17, 19];
        let mut it = xs.iter().skip_while(|&x| *x < 15u);
        let mut i = 0;
        for &x in it {
            assert_eq!(x, ys[i]);
            i += 1;
        }
        assert_eq!(i, ys.len());
    }

    #[test]
    fn test_iterator_skip() {
        let xs = [0u, 1, 2, 3, 5, 13, 15, 16, 17, 19, 20, 30];
        let ys = [13, 15, 16, 17, 19, 20, 30];
        let mut it = xs.iter().skip(5);
        let mut i = 0;
        for &x in it {
            assert_eq!(x, ys[i]);
            i += 1;
        }
        assert_eq!(i, ys.len());
    }

    #[test]
    fn test_iterator_take() {
        let xs = [0u, 1, 2, 3, 5, 13, 15, 16, 17, 19];
        let ys = [0u, 1, 2, 3, 5];
        let mut it = xs.iter().take(5);
        let mut i = 0;
        for &x in it {
            assert_eq!(x, ys[i]);
            i += 1;
        }
        assert_eq!(i, ys.len());
    }

    #[test]
    fn test_iterator_scan() {
        // test the type inference
        fn add(old: &mut int, new: &uint) -> Option<float> {
            *old += *new as int;
            Some(*old as float)
        }
        let xs = [0u, 1, 2, 3, 4];
        let ys = [0f, 1f, 3f, 6f, 10f];

        let mut it = xs.iter().scan(0, add);
        let mut i = 0;
        for x in it {
            assert_eq!(x, ys[i]);
            i += 1;
        }
        assert_eq!(i, ys.len());
    }

    #[test]
    fn test_iterator_flat_map() {
        let xs = [0u, 3, 6];
        let ys = [0u, 1, 2, 3, 4, 5, 6, 7, 8];
        let mut it = xs.iter().flat_map(|&x| count(x, 1).take(3));
        let mut i = 0;
        for x in it {
            assert_eq!(x, ys[i]);
            i += 1;
        }
        assert_eq!(i, ys.len());
    }

    #[test]
    fn test_inspect() {
        let xs = [1u, 2, 3, 4];
        let mut n = 0;

        let ys = xs.iter()
                   .map(|&x| x)
                   .inspect(|_| n += 1)
                   .collect::<~[uint]>();

        assert_eq!(n, xs.len());
        assert_eq!(xs, ys.as_slice());
    }

    #[test]
    fn test_unfoldr() {
        fn count(st: &mut uint) -> Option<uint> {
            if *st < 10 {
                let ret = Some(*st);
                *st += 1;
                ret
            } else {
                None
            }
        }

        let mut it = Unfoldr::new(0, count);
        let mut i = 0;
        for counted in it {
            assert_eq!(counted, i);
            i += 1;
        }
        assert_eq!(i, 10);
    }

    #[test]
    fn test_cycle() {
        let cycle_len = 3;
        let it = count(0u, 1).take(cycle_len).cycle();
        assert_eq!(it.size_hint(), (uint::max_value, None));
        for (i, x) in it.take(100).enumerate() {
            assert_eq!(i % cycle_len, x);
        }

        let mut it = count(0u, 1).take(0).cycle();
        assert_eq!(it.size_hint(), (0, Some(0)));
        assert_eq!(it.next(), None);
    }

    #[test]
    fn test_iterator_nth() {
        let v = &[0, 1, 2, 3, 4];
        for i in range(0u, v.len()) {
            assert_eq!(v.iter().nth(i).unwrap(), &v[i]);
        }
    }

    #[test]
    fn test_iterator_last() {
        let v = &[0, 1, 2, 3, 4];
        assert_eq!(v.iter().last().unwrap(), &4);
        assert_eq!(v.slice(0, 1).iter().last().unwrap(), &0);
    }

    #[test]
    fn test_iterator_len() {
        let v = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        assert_eq!(v.slice(0, 4).iter().len(), 4);
        assert_eq!(v.slice(0, 10).iter().len(), 10);
        assert_eq!(v.slice(0, 0).iter().len(), 0);
    }

    #[test]
    fn test_iterator_sum() {
        let v = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        assert_eq!(v.slice(0, 4).iter().map(|&x| x).sum(), 6);
        assert_eq!(v.iter().map(|&x| x).sum(), 55);
        assert_eq!(v.slice(0, 0).iter().map(|&x| x).sum(), 0);
    }

    #[test]
    fn test_iterator_product() {
        let v = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        assert_eq!(v.slice(0, 4).iter().map(|&x| x).product(), 0);
        assert_eq!(v.slice(1, 5).iter().map(|&x| x).product(), 24);
        assert_eq!(v.slice(0, 0).iter().map(|&x| x).product(), 1);
    }

    #[test]
    fn test_iterator_max() {
        let v = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        assert_eq!(v.slice(0, 4).iter().map(|&x| x).max(), Some(3));
        assert_eq!(v.iter().map(|&x| x).max(), Some(10));
        assert_eq!(v.slice(0, 0).iter().map(|&x| x).max(), None);
    }

    #[test]
    fn test_iterator_min() {
        let v = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        assert_eq!(v.slice(0, 4).iter().map(|&x| x).min(), Some(0));
        assert_eq!(v.iter().map(|&x| x).min(), Some(0));
        assert_eq!(v.slice(0, 0).iter().map(|&x| x).min(), None);
    }

    #[test]
    fn test_iterator_size_hint() {
        let c = count(0, 1);
        let v = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        let v2 = &[10, 11, 12];
        let vi = v.iter();

        assert_eq!(c.size_hint(), (uint::max_value, None));
        assert_eq!(vi.size_hint(), (10, Some(10)));

        assert_eq!(c.take(5).size_hint(), (5, Some(5)));
        assert_eq!(c.skip(5).size_hint().second(), None);
        assert_eq!(c.take_while(|_| false).size_hint(), (0, None));
        assert_eq!(c.skip_while(|_| false).size_hint(), (0, None));
        assert_eq!(c.enumerate().size_hint(), (uint::max_value, None));
        assert_eq!(c.chain(vi.map(|&i| i)).size_hint(), (uint::max_value, None));
        assert_eq!(c.zip(vi).size_hint(), (10, Some(10)));
        assert_eq!(c.scan(0, |_,_| Some(0)).size_hint(), (0, None));
        assert_eq!(c.filter(|_| false).size_hint(), (0, None));
        assert_eq!(c.map(|_| 0).size_hint(), (uint::max_value, None));
        assert_eq!(c.filter_map(|_| Some(0)).size_hint(), (0, None));

        assert_eq!(vi.take(5).size_hint(), (5, Some(5)));
        assert_eq!(vi.take(12).size_hint(), (10, Some(10)));
        assert_eq!(vi.skip(3).size_hint(), (7, Some(7)));
        assert_eq!(vi.skip(12).size_hint(), (0, Some(0)));
        assert_eq!(vi.take_while(|_| false).size_hint(), (0, Some(10)));
        assert_eq!(vi.skip_while(|_| false).size_hint(), (0, Some(10)));
        assert_eq!(vi.enumerate().size_hint(), (10, Some(10)));
        assert_eq!(vi.chain(v2.iter()).size_hint(), (13, Some(13)));
        assert_eq!(vi.zip(v2.iter()).size_hint(), (3, Some(3)));
        assert_eq!(vi.scan(0, |_,_| Some(0)).size_hint(), (0, Some(10)));
        assert_eq!(vi.filter(|_| false).size_hint(), (0, Some(10)));
        assert_eq!(vi.map(|i| i+1).size_hint(), (10, Some(10)));
        assert_eq!(vi.filter_map(|_| Some(0)).size_hint(), (0, Some(10)));
    }

    #[test]
    fn test_collect() {
        let a = ~[1, 2, 3, 4, 5];
        let b: ~[int] = a.iter().map(|&x| x).collect();
        assert_eq!(a, b);
    }

    #[test]
    fn test_all() {
        let v: ~&[int] = ~&[1, 2, 3, 4, 5];
        assert!(v.iter().all(|&x| x < 10));
        assert!(!v.iter().all(|&x| x.is_even()));
        assert!(!v.iter().all(|&x| x > 100));
        assert!(v.slice(0, 0).iter().all(|_| fail!()));
    }

    #[test]
    fn test_any() {
        let v: ~&[int] = ~&[1, 2, 3, 4, 5];
        assert!(v.iter().any(|&x| x < 10));
        assert!(v.iter().any(|&x| x.is_even()));
        assert!(!v.iter().any(|&x| x > 100));
        assert!(!v.slice(0, 0).iter().any(|_| fail!()));
    }

    #[test]
    fn test_find() {
        let v: &[int] = &[1, 3, 9, 27, 103, 14, 11];
        assert_eq!(*v.iter().find(|x| *x & 1 == 0).unwrap(), 14);
        assert_eq!(*v.iter().find(|x| *x % 3 == 0).unwrap(), 3);
        assert!(v.iter().find(|x| *x % 12 == 0).is_none());
    }

    #[test]
    fn test_position() {
        let v = &[1, 3, 9, 27, 103, 14, 11];
        assert_eq!(v.iter().position(|x| *x & 1 == 0).unwrap(), 5);
        assert_eq!(v.iter().position(|x| *x % 3 == 0).unwrap(), 1);
        assert!(v.iter().position(|x| *x % 12 == 0).is_none());
    }

    #[test]
    fn test_count() {
        let xs = &[1, 2, 2, 1, 5, 9, 0, 2];
        assert_eq!(xs.iter().count(|x| *x == 2), 3);
        assert_eq!(xs.iter().count(|x| *x == 5), 1);
        assert_eq!(xs.iter().count(|x| *x == 95), 0);
    }

    #[test]
    fn test_max_by() {
        let xs: &[int] = &[-3, 0, 1, 5, -10];
        assert_eq!(*xs.iter().max_by(|x| x.abs()).unwrap(), -10);
    }

    #[test]
    fn test_min_by() {
        let xs: &[int] = &[-3, 0, 1, 5, -10];
        assert_eq!(*xs.iter().min_by(|x| x.abs()).unwrap(), 0);
    }

    #[test]
    fn test_invert() {
        let xs = [2, 4, 6, 8, 10, 12, 14, 16];
        let mut it = xs.iter();
        it.next();
        it.next();
        assert_eq!(it.invert().map(|&x| x).collect::<~[int]>(), ~[16, 14, 12, 10, 8, 6]);
    }

    #[test]
    fn test_double_ended_map() {
        let xs = [1, 2, 3, 4, 5, 6];
        let mut it = xs.iter().map(|&x| x * -1);
        assert_eq!(it.next(), Some(-1));
        assert_eq!(it.next(), Some(-2));
        assert_eq!(it.next_back(), Some(-6));
        assert_eq!(it.next_back(), Some(-5));
        assert_eq!(it.next(), Some(-3));
        assert_eq!(it.next_back(), Some(-4));
        assert_eq!(it.next(), None);
    }

    #[test]
    fn test_double_ended_filter() {
        let xs = [1, 2, 3, 4, 5, 6];
        let mut it = xs.iter().filter(|&x| *x & 1 == 0);
        assert_eq!(it.next_back().unwrap(), &6);
        assert_eq!(it.next_back().unwrap(), &4);
        assert_eq!(it.next().unwrap(), &2);
        assert_eq!(it.next_back(), None);
    }

    #[test]
    fn test_double_ended_filter_map() {
        let xs = [1, 2, 3, 4, 5, 6];
        let mut it = xs.iter().filter_map(|&x| if x & 1 == 0 { Some(x * 2) } else { None });
        assert_eq!(it.next_back().unwrap(), 12);
        assert_eq!(it.next_back().unwrap(), 8);
        assert_eq!(it.next().unwrap(), 4);
        assert_eq!(it.next_back(), None);
    }

    #[test]
    fn test_double_ended_chain() {
        let xs = [1, 2, 3, 4, 5];
        let ys = ~[7, 9, 11];
        let mut it = xs.iter().chain(ys.iter()).invert();
        assert_eq!(it.next().unwrap(), &11)
        assert_eq!(it.next().unwrap(), &9)
        assert_eq!(it.next_back().unwrap(), &1)
        assert_eq!(it.next_back().unwrap(), &2)
        assert_eq!(it.next_back().unwrap(), &3)
        assert_eq!(it.next_back().unwrap(), &4)
        assert_eq!(it.next_back().unwrap(), &5)
        assert_eq!(it.next_back().unwrap(), &7)
        assert_eq!(it.next_back(), None)
    }

    #[cfg(test)]
    fn check_randacc_iter<A: Eq, T: Clone + RandomAccessIterator<A>>(a: T, len: uint)
    {
        let mut b = a.clone();
        assert_eq!(len, b.indexable());
        let mut n = 0;
        for (i, elt) in a.enumerate() {
            assert_eq!(Some(elt), b.idx(i));
            n += 1;
        }
        assert_eq!(n, len);
        assert_eq!(None, b.idx(n));
        // call recursively to check after picking off an element
        if len > 0 {
            b.next();
            check_randacc_iter(b, len-1);
        }
    }


    #[test]
    fn test_double_ended_flat_map() {
        let u = [0u,1];
        let v = [5,6,7,8];
        let mut it = u.iter().flat_map(|x| v.slice(*x, v.len()).iter());
        assert_eq!(it.next_back().unwrap(), &8);
        assert_eq!(it.next().unwrap(),      &5);
        assert_eq!(it.next_back().unwrap(), &7);
        assert_eq!(it.next_back().unwrap(), &6);
        assert_eq!(it.next_back().unwrap(), &8);
        assert_eq!(it.next().unwrap(),      &6);
        assert_eq!(it.next_back().unwrap(), &7);
        assert_eq!(it.next_back(), None);
        assert_eq!(it.next(),      None);
        assert_eq!(it.next_back(), None);
    }

    #[test]
    fn test_random_access_chain() {
        let xs = [1, 2, 3, 4, 5];
        let ys = ~[7, 9, 11];
        let mut it = xs.iter().chain(ys.iter());
        assert_eq!(it.idx(0).unwrap(), &1);
        assert_eq!(it.idx(5).unwrap(), &7);
        assert_eq!(it.idx(7).unwrap(), &11);
        assert!(it.idx(8).is_none());

        it.next();
        it.next();
        it.next_back();

        assert_eq!(it.idx(0).unwrap(), &3);
        assert_eq!(it.idx(4).unwrap(), &9);
        assert!(it.idx(6).is_none());

        check_randacc_iter(it, xs.len() + ys.len() - 3);
    }

    #[test]
    fn test_random_access_enumerate() {
        let xs = [1, 2, 3, 4, 5];
        check_randacc_iter(xs.iter().enumerate(), xs.len());
    }

    #[test]
    fn test_random_access_invert() {
        let xs = [1, 2, 3, 4, 5];
        check_randacc_iter(xs.iter().invert(), xs.len());
        let mut it = xs.iter().invert();
        it.next();
        it.next_back();
        it.next();
        check_randacc_iter(it, xs.len() - 3);
    }

    #[test]
    fn test_random_access_zip() {
        let xs = [1, 2, 3, 4, 5];
        let ys = [7, 9, 11];
        check_randacc_iter(xs.iter().zip(ys.iter()), cmp::min(xs.len(), ys.len()));
    }

    #[test]
    fn test_random_access_take() {
        let xs = [1, 2, 3, 4, 5];
        let empty: &[int] = [];
        check_randacc_iter(xs.iter().take(3), 3);
        check_randacc_iter(xs.iter().take(20), xs.len());
        check_randacc_iter(xs.iter().take(0), 0);
        check_randacc_iter(empty.iter().take(2), 0);
    }

    #[test]
    fn test_random_access_skip() {
        let xs = [1, 2, 3, 4, 5];
        let empty: &[int] = [];
        check_randacc_iter(xs.iter().skip(2), xs.len() - 2);
        check_randacc_iter(empty.iter().skip(2), 0);
    }

    #[test]
    fn test_random_access_inspect() {
        let xs = [1, 2, 3, 4, 5];

        // test .map and .inspect that don't implement Clone
        let it = xs.iter().inspect(|_| {});
        assert_eq!(xs.len(), it.indexable());
        for (i, elt) in xs.iter().enumerate() {
            assert_eq!(Some(elt), it.idx(i));
        }

    }

    #[test]
    fn test_random_access_map() {
        let xs = [1, 2, 3, 4, 5];

        let it = xs.iter().map(|x| *x);
        assert_eq!(xs.len(), it.indexable());
        for (i, elt) in xs.iter().enumerate() {
            assert_eq!(Some(*elt), it.idx(i));
        }
    }

    #[test]
    fn test_random_access_cycle() {
        let xs = [1, 2, 3, 4, 5];
        let empty: &[int] = [];
        check_randacc_iter(xs.iter().cycle().take(27), 27);
        check_randacc_iter(empty.iter().cycle(), 0);
    }

    #[test]
    fn test_double_ended_range() {
        assert_eq!(range(11i, 14).invert().collect::<~[int]>(), ~[13i, 12, 11]);
        for _ in range(10i, 0).invert() {
            fail!("unreachable");
        }

        assert_eq!(range(11u, 14).invert().collect::<~[uint]>(), ~[13u, 12, 11]);
        for _ in range(10u, 0).invert() {
            fail!("unreachable");
        }
    }

    #[test]
    fn test_range_inclusive() {
        assert_eq!(range_inclusive(0i, 5).collect::<~[int]>(), ~[0i, 1, 2, 3, 4, 5]);
        assert_eq!(range_inclusive(0i, 5).invert().collect::<~[int]>(), ~[5i, 4, 3, 2, 1, 0]);
    }

    #[test]
    fn test_reverse() {
        let mut ys = [1, 2, 3, 4, 5];
        ys.mut_iter().reverse_();
        assert_eq!(ys, [5, 4, 3, 2, 1]);
    }
}
