// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!
 * Generic hashing support.
 *
 * This module provides a generic way to compute the hash of a value. The
 * simplest way to make a type hashable is to use `#[deriving(Hash)]`:
 *
 * # Example
 *
 * ```rust
 * use std::hash;
 * use std::hash::Hash;
 *
 * #[deriving(Hash)]
 * struct Person {
 *     id: uint,
 *     name: String,
 *     phone: u64,
 * }
 *
 * let person1 = Person { id: 5, name: "Janet".to_string(), phone: 555_666_7777 };
 * let person2 = Person { id: 5, name: "Bob".to_string(), phone: 555_666_7777 };
 *
 * assert!(hash::hash(&person1) != hash::hash(&person2));
 * ```
 *
 * If you need more control over how a value is hashed, you need to implement
 * the trait `Hash`:
 *
 * ```rust
 * use std::hash;
 * use std::hash::Hash;
 * use std::hash::sip::SipState;
 *
 * struct Person {
 *     id: uint,
 *     name: String,
 *     phone: u64,
 * }
 *
 * impl Hash for Person {
 *     fn hash(&self, state: &mut SipState) {
 *         self.id.hash(state);
 *         self.phone.hash(state);
 *     }
 * }
 *
 * let person1 = Person { id: 5, name: "Janet".to_string(), phone: 555_666_7777 };
 * let person2 = Person { id: 5, name: "Bob".to_string(), phone: 555_666_7777 };
 *
 * assert!(hash::hash(&person1) == hash::hash(&person2));
 * ```
 */

#![allow(unused_must_use)]

use core::prelude::*;

use alloc::boxed::Box;
use alloc::rc::Rc;
use core::intrinsics::TypeId;
use core::mem;

use vec::Vec;

/// Reexport the `sip::hash` function as our default hasher.
pub use self::sip::hash as hash;

pub mod sip;

/// A hashable type. The `S` type parameter is an abstract hash state that is
/// used by the `Hash` to compute the hash. It defaults to
/// `std::hash::sip::SipState`.
pub trait Hash<S = sip::SipState> for Sized? {
    /// Computes the hash of a value.
    fn hash(&self, state: &mut S);
}

/// A trait that computes a hash for a value. The main users of this trait are
/// containers like `HashMap`, which need a generic way hash multiple types.
pub trait Hasher<S> {
    /// Compute the hash of a value.
    fn hash<Sized? T: Hash<S>>(&self, value: &T) -> u64;
}

pub trait Writer {
    fn write(&mut self, bytes: &[u8]);
}

//////////////////////////////////////////////////////////////////////////////

macro_rules! impl_hash {
    ($ty:ident, $uty:ident) => {
        impl<S: Writer> Hash<S> for $ty {
            #[inline]
            fn hash(&self, state: &mut S) {
                let a: [u8, ..::core::$ty::BYTES] = unsafe {
                    mem::transmute((*self as $uty).to_le() as $ty)
                };
                state.write(a.as_slice())
            }
        }
    }
}

impl_hash!(u8, u8)
impl_hash!(u16, u16)
impl_hash!(u32, u32)
impl_hash!(u64, u64)
impl_hash!(uint, uint)
impl_hash!(i8, u8)
impl_hash!(i16, u16)
impl_hash!(i32, u32)
impl_hash!(i64, u64)
impl_hash!(int, uint)

impl<S: Writer> Hash<S> for bool {
    #[inline]
    fn hash(&self, state: &mut S) {
        (*self as u8).hash(state);
    }
}

impl<S: Writer> Hash<S> for char {
    #[inline]
    fn hash(&self, state: &mut S) {
        (*self as u32).hash(state);
    }
}

impl<S: Writer> Hash<S> for str {
    #[inline]
    fn hash(&self, state: &mut S) {
        state.write(self.as_bytes());
        0xffu8.hash(state)
    }
}

macro_rules! impl_hash_tuple(
    () => (
        impl<S: Writer> Hash<S> for () {
            #[inline]
            fn hash(&self, state: &mut S) {
                state.write([]);
            }
        }
    );

    ( $($name:ident)+) => (
        impl<S: Writer, $($name: Hash<S>),*> Hash<S> for ($($name,)*) {
            #[inline]
            #[allow(non_snake_case)]
            fn hash(&self, state: &mut S) {
                match *self {
                    ($(ref $name,)*) => {
                        $(
                            $name.hash(state);
                        )*
                    }
                }
            }
        }
    );
)

impl_hash_tuple!()
impl_hash_tuple!(A)
impl_hash_tuple!(A B)
impl_hash_tuple!(A B C)
impl_hash_tuple!(A B C D)
impl_hash_tuple!(A B C D E)
impl_hash_tuple!(A B C D E F)
impl_hash_tuple!(A B C D E F G)
impl_hash_tuple!(A B C D E F G H)
impl_hash_tuple!(A B C D E F G H I)
impl_hash_tuple!(A B C D E F G H I J)
impl_hash_tuple!(A B C D E F G H I J K)
impl_hash_tuple!(A B C D E F G H I J K L)

impl<S: Writer, T: Hash<S>> Hash<S> for [T] {
    #[inline]
    fn hash(&self, state: &mut S) {
        self.len().hash(state);
        for elt in self.iter() {
            elt.hash(state);
        }
    }
}


impl<S: Writer, T: Hash<S>> Hash<S> for Vec<T> {
    #[inline]
    fn hash(&self, state: &mut S) {
        self.as_slice().hash(state);
    }
}

impl<'a, S: Writer, Sized? T: Hash<S>> Hash<S> for &'a T {
    #[inline]
    fn hash(&self, state: &mut S) {
        (**self).hash(state);
    }
}

impl<'a, S: Writer, Sized? T: Hash<S>> Hash<S> for &'a mut T {
    #[inline]
    fn hash(&self, state: &mut S) {
        (**self).hash(state);
    }
}

impl<S: Writer, Sized? T: Hash<S>> Hash<S> for Box<T> {
    #[inline]
    fn hash(&self, state: &mut S) {
        (**self).hash(state);
    }
}

// FIXME (#18248) Make `T` `Sized?`
impl<S: Writer, T: Hash<S>> Hash<S> for Rc<T> {
    #[inline]
    fn hash(&self, state: &mut S) {
        (**self).hash(state);
    }
}

impl<S: Writer, T: Hash<S>> Hash<S> for Option<T> {
    #[inline]
    fn hash(&self, state: &mut S) {
        match *self {
            Some(ref x) => {
                0u8.hash(state);
                x.hash(state);
            }
            None => {
                1u8.hash(state);
            }
        }
    }
}

impl<S: Writer, T> Hash<S> for *const T {
    #[inline]
    fn hash(&self, state: &mut S) {
        // NB: raw-pointer Hash does _not_ dereference
        // to the target; it just gives you the pointer-bytes.
        (*self as uint).hash(state);
    }
}

impl<S: Writer, T> Hash<S> for *mut T {
    #[inline]
    fn hash(&self, state: &mut S) {
        // NB: raw-pointer Hash does _not_ dereference
        // to the target; it just gives you the pointer-bytes.
        (*self as uint).hash(state);
    }
}

impl<S: Writer> Hash<S> for TypeId {
    #[inline]
    fn hash(&self, state: &mut S) {
        self.hash().hash(state)
    }
}

impl<S: Writer, T: Hash<S>, U: Hash<S>> Hash<S> for Result<T, U> {
    #[inline]
    fn hash(&self, state: &mut S) {
        match *self {
            Ok(ref t) => { 1u.hash(state); t.hash(state); }
            Err(ref t) => { 2u.hash(state); t.hash(state); }
        }
    }
}

//////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use core::kinds::Sized;
    use std::mem;

    use slice::SlicePrelude;
    use super::{Hash, Hasher, Writer};

    struct MyWriterHasher;

    impl Hasher<MyWriter> for MyWriterHasher {
        fn hash<Sized? T: Hash<MyWriter>>(&self, value: &T) -> u64 {
            let mut state = MyWriter { hash: 0 };
            value.hash(&mut state);
            state.hash
        }
    }

    struct MyWriter {
        hash: u64,
    }

    impl Writer for MyWriter {
        // Most things we'll just add up the bytes.
        fn write(&mut self, buf: &[u8]) {
            for byte in buf.iter() {
                self.hash += *byte as u64;
            }
        }
    }

    #[test]
    fn test_writer_hasher() {
        use alloc::boxed::Box;

        let hasher = MyWriterHasher;

        assert_eq!(hasher.hash(&()), 0);

        assert_eq!(hasher.hash(&5u8), 5);
        assert_eq!(hasher.hash(&5u16), 5);
        assert_eq!(hasher.hash(&5u32), 5);
        assert_eq!(hasher.hash(&5u64), 5);
        assert_eq!(hasher.hash(&5u), 5);

        assert_eq!(hasher.hash(&5i8), 5);
        assert_eq!(hasher.hash(&5i16), 5);
        assert_eq!(hasher.hash(&5i32), 5);
        assert_eq!(hasher.hash(&5i64), 5);
        assert_eq!(hasher.hash(&5i), 5);

        assert_eq!(hasher.hash(&false), 0);
        assert_eq!(hasher.hash(&true), 1);

        assert_eq!(hasher.hash(&'a'), 97);

        let s: &str = "a";
        assert_eq!(hasher.hash(& s), 97 + 0xFF);
        // FIXME (#18283) Enable test
        //let s: Box<str> = box "a";
        //assert_eq!(hasher.hash(& s), 97 + 0xFF);
        let cs: &[u8] = &[1u8, 2u8, 3u8];
        assert_eq!(hasher.hash(& cs), 9);
        let cs: Box<[u8]> = box [1u8, 2u8, 3u8];
        assert_eq!(hasher.hash(& cs), 9);

        // FIXME (#18248) Add tests for hashing Rc<str> and Rc<[T]>

        unsafe {
            let ptr: *const int = mem::transmute(5i);
            assert_eq!(hasher.hash(&ptr), 5);
        }

        unsafe {
            let ptr: *mut int = mem::transmute(5i);
            assert_eq!(hasher.hash(&ptr), 5);
        }
    }

    struct Custom {
        hash: u64
    }

    impl Hash<u64> for Custom {
        fn hash(&self, state: &mut u64) {
            *state = self.hash;
        }
    }

    #[test]
    fn test_custom_state() {
        let custom = Custom { hash: 5 };
        let mut state = 0;
        custom.hash(&mut state);
        assert_eq!(state, 5);
    }
}
