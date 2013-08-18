// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! A type representing either success or failure

#[allow(missing_doc)];

use clone::Clone;
use cmp::Eq;
use either;
use iterator::Iterator;
use option::{None, Option, Some, OptionIterator};
use vec;
use vec::OwnedVector;
use to_str::ToStr;
use str::StrSlice;

/// `Result` is a type that represents either success (`Ok`) or failure (`Err`).
///
/// In order to provide informative error messages, `E` is required to implement `ToStr`.
/// It is further recommended for `E` to be a descriptive error type, eg a `enum` for
/// all possible errors cases.
#[deriving(Clone, Eq)]
pub enum Result<T, E> {
    /// Contains the successful result value
    Ok(T),
    /// Contains the error value
    Err(E)
}

impl<T, E: ToStr> Result<T, E> {
    /// Convert to the `either` type
    ///
    /// `Ok` result variants are converted to `either::Right` variants, `Err`
    /// result variants are converted to `either::Left`.
    #[inline]
    pub fn to_either(self)-> either::Either<E, T>{
        match self {
            Ok(t) => either::Right(t),
            Err(e) => either::Left(e),
        }
    }

    /// Get a reference to the value out of a successful result
    ///
    /// # Failure
    ///
    /// If the result is an error
    #[inline]
    pub fn get_ref<'a>(&'a self) -> &'a T {
        match *self {
            Ok(ref t) => t,
            Err(ref e) => fail!("called `Result::get_ref()` on `Err` value: %s", e.to_str()),
        }
    }

    /// Returns true if the result is `Ok`
    #[inline]
    pub fn is_ok(&self) -> bool {
        match *self {
            Ok(_) => true,
            Err(_) => false
        }
    }

    /// Returns true if the result is `Err`
    #[inline]
    pub fn is_err(&self) -> bool {
        !self.is_ok()
    }

    /// Call a method based on a previous result
    ///
    /// If `self` is `Ok` then the value is extracted and passed to `op`
    /// whereupon `op`s result is returned. if `self` is `Err` then it is
    /// immediately returned. This function can be used to compose the results
    /// of two functions.
    ///
    /// Example:
    ///
    ///     for buf in read_file(file) {
    ///         print_buf(buf)
    ///     }
    #[inline]
    pub fn iter<'r>(&'r self) -> OptionIterator<&'r T> {
        match *self {
            Ok(ref t) => Some(t),
            Err(*) => None,
        }.move_iter()
    }

    /// Call a method based on a previous result
    ///
    /// If `self` is `Err` then the value is extracted and passed to `op`
    /// whereupon `op`s result is returned. if `self` is `Ok` then it is
    /// immediately returned.  This function can be used to pass through a
    /// successful result while handling an error.
    #[inline]
    pub fn iter_err<'r>(&'r self) -> OptionIterator<&'r E> {
        match *self {
            Ok(*) => None,
            Err(ref t) => Some(t),
        }.move_iter()
    }

    /// Unwraps a result, yielding the content of an `Ok`.
    /// Fails if the value is a `Err` with an error message derived
    /// from `E`'s `ToStr` implementation.
    #[inline]
    pub fn unwrap(self) -> T {
        match self {
            Ok(t) => t,
            Err(e) => fail!("called `Result::unwrap()` on `Err` value: %s", e.to_str()),
        }
    }

    /// Unwraps a result, yielding the content of an `Err`.
    /// Fails if the value is a `Ok`.
    #[inline]
    pub fn unwrap_err(self) -> E {
        self.expect_err("called `Result::unwrap_err()` on `Ok` value")
    }

    /// Unwraps a result, yielding the content of an `Ok`.
    /// Fails if the value is a `Err` with a custom failure message.
    #[inline]
    pub fn expect(self, reason: &str) -> T {
        match self {
            Ok(t) => t,
            Err(_) => fail!(reason.to_owned()),
        }
    }

    /// Unwraps a result, yielding the content of an `Err`
    /// Fails if the value is a `Ok` with a custom failure message.
    #[inline]
    pub fn expect_err(self, reason: &str) -> E {
        match self {
            Err(e) => e,
            Ok(_) => fail!(reason.to_owned()),
        }
    }

    /// Call a method based on a previous result
    ///
    /// If `self` is `Ok` then the value is extracted and passed to `op`
    /// whereupon `op`s result is wrapped in `Ok` and returned. if `self` is
    /// `Err` then it is immediately returned.  This function can be used to
    /// compose the results of two functions.
    ///
    /// Example:
    ///
    ///     let res = do read_file(file).map_move |buf| {
    ///         parse_bytes(buf)
    ///     }
    #[inline]
    pub fn map_move<U>(self, op: &fn(T) -> U) -> Result<U,E> {
        match self {
          Ok(t) => Ok(op(t)),
          Err(e) => Err(e)
        }
    }

    /// Call a method based on a previous result
    ///
    /// If `self` is `Err` then the value is extracted and passed to `op`
    /// whereupon `op`s result is wrapped in an `Err` and returned. if `self` is
    /// `Ok` then it is immediately returned.  This function can be used to pass
    /// through a successful result while handling an error.
    #[inline]
    pub fn map_err_move<F>(self, op: &fn(E) -> F) -> Result<T,F> {
        match self {
          Ok(t) => Ok(t),
          Err(e) => Err(op(e))
        }
    }

    /// Call a method based on a previous result
    ///
    /// If `self` is `Ok` then the value is extracted and passed to `op`
    /// whereupon `op`s result is returned. if `self` is `Err` then it is
    /// immediately returned. This function can be used to compose the results
    /// of two functions.
    ///
    /// Example:
    ///
    ///     let res = do read_file(file) |buf| {
    ///         Ok(parse_bytes(buf))
    ///     };
    #[inline]
    pub fn chain<U>(self, op: &fn(T) -> Result<U, E>) -> Result<U, E> {
        match self {
            Ok(t) => op(t),
            Err(e) => Err(e),
        }
    }

    /// Call a function based on a previous result
    ///
    /// If `self` is `Err` then the value is extracted and passed to `op`
    /// whereupon `op`s result is returned. if `self` is `Ok` then it is
    /// immediately returned.  This function can be used to pass through a
    /// successful result while handling an error.
    #[inline]
    pub fn chain_err<F>(self, op: &fn(E) -> Result<T, F>) -> Result<T, F> {
        match self {
            Ok(t) => Ok(t),
            Err(e) => op(e),
        }
    }
}

impl<T: Clone, E: ToStr> Result<T, E> {
    /// Call a method based on a previous result
    ///
    /// If `self` is `Err` then the value is extracted and passed to `op`
    /// whereupon `op`s result is wrapped in an `Err` and returned. if `self` is
    /// `Ok` then it is immediately returned.  This function can be used to pass
    /// through a successful result while handling an error.
    #[inline]
    pub fn map_err<F: Clone>(&self, op: &fn(&E) -> F) -> Result<T,F> {
        match *self {
            Ok(ref t) => Ok(t.clone()),
            Err(ref e) => Err(op(e))
        }
    }
}

impl<T, E: Clone + ToStr> Result<T, E> {
    /// Call a method based on a previous result
    ///
    /// If `self` is `Ok` then the value is extracted and passed to `op`
    /// whereupon `op`s result is wrapped in `Ok` and returned. if `self` is
    /// `Err` then it is immediately returned.  This function can be used to
    /// compose the results of two functions.
    ///
    /// Example:
    ///
    ///     let res = do read_file(file).map |buf| {
    ///         parse_bytes(buf)
    ///     };
    #[inline]
    pub fn map<U>(&self, op: &fn(&T) -> U) -> Result<U,E> {
        match *self {
            Ok(ref t) => Ok(op(t)),
            Err(ref e) => Err(e.clone())
        }
    }
}

#[inline]
#[allow(missing_doc)]
pub fn map_opt<T, U: ToStr, V>(o_t: &Option<T>,
                               op: &fn(&T) -> Result<V,U>) -> Result<Option<V>,U> {
    match *o_t {
        None => Ok(None),
        Some(ref t) => match op(t) {
            Ok(v) => Ok(Some(v)),
            Err(e) => Err(e)
        }
    }
}

/// Takes each element in the iterator: if it is an error, no further
/// elements are taken, and the error is returned.
/// Should no error occur, a vector containing the values of each Result
/// is returned.
///
/// Here is an example which increments every integer in a vector,
/// checking for overflow:
///
///     fn inc_conditionally(x: uint) -> Result<uint, &'static str> {
///         if x == uint::max_value { return Err("overflow"); }
///         else { return Ok(x+1u); }
///     }
///     let v = [1u, 2, 3];
///     let res = collect(v.iter().map(|&x| inc_conditionally(x)));
///     assert!(res == Ok(~[2u, 3, 4]));
#[inline]
pub fn collect<T, E, Iter: Iterator<Result<T, E>>>(mut iterator: Iter)
    -> Result<~[T], E> {
    let (lower, _) = iterator.size_hint();
    let mut vs: ~[T] = vec::with_capacity(lower);
    for t in iterator {
        match t {
            Ok(v) => vs.push(v),
            Err(u) => return Err(u)
        }
    }
    Ok(vs)
}

/// Perform a fold operation over the result values from an iterator.
///
/// If an `Err` is encountered, it is immediately returned.
/// Otherwise, the folded value is returned.
#[inline]
pub fn fold<T, V, E,
            Iter: Iterator<Result<T, E>>>(
            mut iterator: Iter,
            mut init: V,
            f: &fn(V, T) -> V)
         -> Result<V, E> {
    for t in iterator {
        match t {
            Ok(v) => init = f(init, v),
            Err(u) => return Err(u)
        }
    }
    Ok(init)
}

/// Perform a trivial fold operation over the result values
/// from an iterator.
///
/// If an `Err` is encountered, it is immediately returned.
/// Otherwise, a simple `Ok(())` is returned.
#[inline]
pub fn fold_<T, E, Iter: Iterator<Result<T, E>>>(
             iterator: Iter)
          -> Result<(), E> {
    fold(iterator, (), |_, _| ())
}


#[cfg(test)]
mod tests {
    use super::*;

    use either;
    use iterator::range;
    use str::OwnedStr;
    use vec::ImmutableVector;

    pub fn op1() -> Result<int, ~str> { Ok(666) }

    pub fn op2(i: int) -> Result<uint, ~str> {
        Ok(i as uint + 1u)
    }

    pub fn op3() -> Result<int, ~str> { Err(~"sadface") }

    #[test]
    pub fn chain_success() {
        assert_eq!(op1().chain(op2).unwrap(), 667u);
    }

    #[test]
    pub fn chain_failure() {
        assert_eq!(op3().chain( op2).unwrap_err(), ~"sadface");
    }

    #[test]
    pub fn test_impl_iter() {
        let mut valid = false;
        let okval = Ok::<~str, ~str>(~"a");
        do okval.iter().next().map |_| { valid = true; };
        assert!(valid);

        let errval = Err::<~str, ~str>(~"b");
        do errval.iter().next().map |_| { valid = false; };
        assert!(valid);
    }

    #[test]
    pub fn test_impl_iter_err() {
        let mut valid = true;
        let okval = Ok::<~str, ~str>(~"a");
        do okval.iter_err().next().map |_| { valid = false };
        assert!(valid);

        valid = false;
        let errval = Err::<~str, ~str>(~"b");
        do errval.iter_err().next().map |_| { valid = true };
        assert!(valid);
    }

    #[test]
    pub fn test_impl_map() {
        assert_eq!(Ok::<~str, ~str>(~"a").map(|x| (~"b").append(*x)), Ok(~"ba"));
        assert_eq!(Err::<~str, ~str>(~"a").map(|x| (~"b").append(*x)), Err(~"a"));
    }

    #[test]
    pub fn test_impl_map_err() {
        assert_eq!(Ok::<~str, ~str>(~"a").map_err(|x| (~"b").append(*x)), Ok(~"a"));
        assert_eq!(Err::<~str, ~str>(~"a").map_err(|x| (~"b").append(*x)), Err(~"ba"));
    }

    #[test]
    pub fn test_impl_map_move() {
        assert_eq!(Ok::<~str, ~str>(~"a").map_move(|x| x + "b"), Ok(~"ab"));
        assert_eq!(Err::<~str, ~str>(~"a").map_move(|x| x + "b"), Err(~"a"));
    }

    #[test]
    pub fn test_impl_map_err_move() {
        assert_eq!(Ok::<~str, ~str>(~"a").map_err_move(|x| x + "b"), Ok(~"a"));
        assert_eq!(Err::<~str, ~str>(~"a").map_err_move(|x| x + "b"), Err(~"ab"));
    }

    #[test]
    pub fn test_get_ref_method() {
        let foo: Result<int, ()> = Ok(100);
        assert_eq!(*foo.get_ref(), 100);
    }

    #[test]
    pub fn test_to_either() {
        let r: Result<int, ()> = Ok(100);
        let err: Result<(), int> = Err(404);

        assert_eq!(r.to_either(), either::Right(100));
        assert_eq!(err.to_either(), either::Left(404));
    }

    #[test]
    fn test_collect() {
        assert_eq!(collect(range(0, 0)
                           .map(|_| Ok::<int, ()>(0))),
                   Ok(~[]));
        assert_eq!(collect(range(0, 3)
                           .map(|x| Ok::<int, ()>(x))),
                   Ok(~[0, 1, 2]));
        assert_eq!(collect(range(0, 3)
                           .map(|x| if x > 1 { Err(x) } else { Ok(x) })),
                   Err(2));

        // test that it does not take more elements than it needs
        let functions = [|| Ok(()), || Err(1), || fail!()];

        assert_eq!(collect(functions.iter().map(|f| (*f)())),
                   Err(1));
    }

    #[test]
    fn test_fold() {
        assert_eq!(fold_(range(0, 0)
                        .map(|_| Ok::<(), ()>(()))),
                   Ok(()));
        assert_eq!(fold(range(0, 3)
                        .map(|x| Ok::<int, ()>(x)),
                        0, |a, b| a + b),
                   Ok(3));
        assert_eq!(fold_(range(0, 3)
                        .map(|x| if x > 1 { Err(x) } else { Ok(()) })),
                   Err(2));

        // test that it does not take more elements than it needs
        let functions = [|| Ok(()), || Err(1), || fail!()];

        assert_eq!(fold_(functions.iter()
                        .map(|f| (*f)())),
                   Err(1));
    }
}
