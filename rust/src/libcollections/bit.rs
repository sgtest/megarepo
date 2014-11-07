// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// FIXME(Gankro): Bitv and BitvSet are very tightly coupled. Ideally (for maintenance),
// they should be in separate files/modules, with BitvSet only using Bitv's public API.

//! Collections implemented with bit vectors.
//!
//! # Example
//!
//! This is a simple example of the [Sieve of Eratosthenes][sieve]
//! which calculates prime numbers up to a given limit.
//!
//! [sieve]: http://en.wikipedia.org/wiki/Sieve_of_Eratosthenes
//!
//! ```
//! use std::collections::{BitvSet, Bitv};
//! use std::iter;
//!
//! let max_prime = 10000;
//!
//! // Store the primes as a BitvSet
//! let primes = {
//!     // Assume all numbers are prime to begin, and then we
//!     // cross off non-primes progressively
//!     let mut bv = Bitv::with_capacity(max_prime, true);
//!
//!     // Neither 0 nor 1 are prime
//!     bv.set(0, false);
//!     bv.set(1, false);
//!
//!     for i in iter::range_inclusive(2, (max_prime as f64).sqrt() as uint) {
//!         // if i is a prime
//!         if bv[i] {
//!             // Mark all multiples of i as non-prime (any multiples below i * i
//!             // will have been marked as non-prime previously)
//!             for j in iter::range_step(i * i, max_prime, i) { bv.set(j, false) }
//!         }
//!     }
//!     BitvSet::from_bitv(bv)
//! };
//!
//! // Simple primality tests below our max bound
//! let print_primes = 20;
//! print!("The primes below {} are: ", print_primes);
//! for x in range(0, print_primes) {
//!     if primes.contains(&x) {
//!         print!("{} ", x);
//!     }
//! }
//! println!("");
//!
//! // We can manipulate the internal Bitv
//! let num_primes = primes.get_ref().iter().filter(|x| *x).count();
//! println!("There are {} primes below {}", num_primes, max_prime);
//! ```

use core::prelude::*;

use core::cmp;
use core::default::Default;
use core::fmt;
use core::iter::{Chain, Enumerate, Repeat, Skip, Take};
use core::iter;
use core::slice;
use core::u32;
use std::hash;

use vec::Vec;

// FIXME(conventions): look, we just need to refactor this whole thing. Inside and out.

type MatchWords<'a> = Chain<MaskWords<'a>, Skip<Take<Enumerate<Repeat<u32>>>>>;
// Take two BitV's, and return iterators of their words, where the shorter one
// has been padded with 0's
fn match_words <'a,'b>(a: &'a Bitv, b: &'b Bitv) -> (MatchWords<'a>, MatchWords<'b>) {
    let a_len = a.storage.len();
    let b_len = b.storage.len();

    // have to uselessly pretend to pad the longer one for type matching
    if a_len < b_len {
        (a.mask_words(0).chain(Repeat::new(0u32).enumerate().take(b_len).skip(a_len)),
         b.mask_words(0).chain(Repeat::new(0u32).enumerate().take(0).skip(0)))
    } else {
        (a.mask_words(0).chain(Repeat::new(0u32).enumerate().take(0).skip(0)),
         b.mask_words(0).chain(Repeat::new(0u32).enumerate().take(a_len).skip(b_len)))
    }
}

static TRUE: bool = true;
static FALSE: bool = false;

/// The bitvector type.
///
/// # Example
///
/// ```rust
/// use collections::Bitv;
///
/// let mut bv = Bitv::with_capacity(10, false);
///
/// // insert all primes less than 10
/// bv.set(2, true);
/// bv.set(3, true);
/// bv.set(5, true);
/// bv.set(7, true);
/// println!("{}", bv.to_string());
/// println!("total bits set to true: {}", bv.iter().filter(|x| *x).count());
///
/// // flip all values in bitvector, producing non-primes less than 10
/// bv.negate();
/// println!("{}", bv.to_string());
/// println!("total bits set to true: {}", bv.iter().filter(|x| *x).count());
///
/// // reset bitvector to empty
/// bv.clear();
/// println!("{}", bv.to_string());
/// println!("total bits set to true: {}", bv.iter().filter(|x| *x).count());
/// ```
pub struct Bitv {
    /// Internal representation of the bit vector
    storage: Vec<u32>,
    /// The number of valid bits in the internal representation
    nbits: uint
}

impl Index<uint,bool> for Bitv {
    #[inline]
    fn index<'a>(&'a self, i: &uint) -> &'a bool {
        if self.get(*i) {
            &TRUE
        } else {
            &FALSE
        }
    }
}

struct MaskWords<'a> {
    iter: slice::Items<'a, u32>,
    next_word: Option<&'a u32>,
    last_word_mask: u32,
    offset: uint
}

impl<'a> Iterator<(uint, u32)> for MaskWords<'a> {
    /// Returns (offset, word)
    #[inline]
    fn next<'a>(&'a mut self) -> Option<(uint, u32)> {
        let ret = self.next_word;
        match ret {
            Some(&w) => {
                self.next_word = self.iter.next();
                self.offset += 1;
                // The last word may need to be masked
                if self.next_word.is_none() {
                    Some((self.offset - 1, w & self.last_word_mask))
                } else {
                    Some((self.offset - 1, w))
                }
            },
            None => None
        }
    }
}

impl Bitv {
    #[inline]
    fn process(&mut self, other: &Bitv, op: |u32, u32| -> u32) -> bool {
        let len = other.storage.len();
        assert_eq!(self.storage.len(), len);
        let mut changed = false;
        // Notice: `a` is *not* masked here, which is fine as long as
        // `op` is a bitwise operation, since any bits that should've
        // been masked were fine to change anyway. `b` is masked to
        // make sure its unmasked bits do not cause damage.
        for (a, (_, b)) in self.storage.iter_mut()
                           .zip(other.mask_words(0)) {
            let w = op(*a, b);
            if *a != w {
                changed = true;
                *a = w;
            }
        }
        changed
    }

    #[inline]
    fn mask_words<'a>(&'a self, mut start: uint) -> MaskWords<'a> {
        if start > self.storage.len() {
            start = self.storage.len();
        }
        let mut iter = self.storage[start..].iter();
        MaskWords {
          next_word: iter.next(),
          iter: iter,
          last_word_mask: {
              let rem = self.nbits % u32::BITS;
              if rem > 0 {
                  (1 << rem) - 1
              } else { !0 }
          },
          offset: start
        }
    }

    /// Creates an empty `Bitv`.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::Bitv;
    /// let mut bv = Bitv::new();
    /// ```
    #[unstable = "matches collection reform specification, waiting for dust to settle"]
    pub fn new() -> Bitv {
        Bitv { storage: Vec::new(), nbits: 0 }
    }

    /// Creates a `Bitv` that holds `nbits` elements, setting each element
    /// to `init`.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::Bitv;
    ///
    /// let mut bv = Bitv::with_capacity(10u, false);
    /// assert_eq!(bv.len(), 10u);
    /// for x in bv.iter() {
    ///     assert_eq!(x, false);
    /// }
    /// ```
    pub fn with_capacity(nbits: uint, init: bool) -> Bitv {
        let mut bitv = Bitv {
            storage: Vec::from_elem((nbits + u32::BITS - 1) / u32::BITS,
                                    if init { !0u32 } else { 0u32 }),
            nbits: nbits
        };

        // Zero out any unused bits in the highest word if necessary
        let used_bits = bitv.nbits % u32::BITS;
        if init && used_bits != 0 {
            let largest_used_word = (bitv.nbits + u32::BITS - 1) / u32::BITS - 1;
            bitv.storage[largest_used_word] &= (1 << used_bits) - 1;
        }

        bitv
    }

    /// Retrieves the value at index `i`.
    ///
    /// # Failure
    ///
    /// Fails if `i` is out of bounds.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::bitv;
    ///
    /// let bv = bitv::from_bytes([0b01100000]);
    /// assert_eq!(bv.get(0), false);
    /// assert_eq!(bv.get(1), true);
    ///
    /// // Can also use array indexing
    /// assert_eq!(bv[1], true);
    /// ```
    #[inline]
    pub fn get(&self, i: uint) -> bool {
        assert!(i < self.nbits);
        let w = i / u32::BITS;
        let b = i % u32::BITS;
        let x = self.storage[w] & (1 << b);
        x != 0
    }

    /// Sets the value of a bit at a index `i`.
    ///
    /// # Failure
    ///
    /// Fails if `i` is out of bounds.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::Bitv;
    ///
    /// let mut bv = Bitv::with_capacity(5, false);
    /// bv.set(3, true);
    /// assert_eq!(bv[3], true);
    /// ```
    #[inline]
    pub fn set(&mut self, i: uint, x: bool) {
        assert!(i < self.nbits);
        let w = i / u32::BITS;
        let b = i % u32::BITS;
        let flag = 1 << b;
        let val = if x { self.storage[w] | flag }
                  else { self.storage[w] & !flag };
        self.storage[w] = val;
    }

    /// Sets all bits to 1.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::bitv;
    ///
    /// let before = 0b01100000;
    /// let after  = 0b11111111;
    ///
    /// let mut bv = bitv::from_bytes([before]);
    /// bv.set_all();
    /// assert_eq!(bv, bitv::from_bytes([after]));
    /// ```
    #[inline]
    pub fn set_all(&mut self) {
        for w in self.storage.iter_mut() { *w = !0u32; }
    }

    /// Flips all bits.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::bitv;
    ///
    /// let before = 0b01100000;
    /// let after  = 0b10011111;
    ///
    /// let mut bv = bitv::from_bytes([before]);
    /// bv.negate();
    /// assert_eq!(bv, bitv::from_bytes([after]));
    /// ```
    #[inline]
    pub fn negate(&mut self) {
        for w in self.storage.iter_mut() { *w = !*w; }
    }

    /// Calculates the union of two bitvectors. This acts like the bitwise `or`
    /// function.
    ///
    /// Sets `self` to the union of `self` and `other`. Both bitvectors must be
    /// the same length. Returns `true` if `self` changed.
    ///
    /// # Failure
    ///
    /// Fails if the bitvectors are of different lengths.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::bitv;
    ///
    /// let a   = 0b01100100;
    /// let b   = 0b01011010;
    /// let res = 0b01111110;
    ///
    /// let mut a = bitv::from_bytes([a]);
    /// let b = bitv::from_bytes([b]);
    ///
    /// assert!(a.union(&b));
    /// assert_eq!(a, bitv::from_bytes([res]));
    /// ```
    #[inline]
    pub fn union(&mut self, other: &Bitv) -> bool {
        self.process(other, |w1, w2| w1 | w2)
    }

    /// Calculates the intersection of two bitvectors. This acts like the
    /// bitwise `and` function.
    ///
    /// Sets `self` to the intersection of `self` and `other`. Both bitvectors
    /// must be the same length. Returns `true` if `self` changed.
    ///
    /// # Failure
    ///
    /// Fails if the bitvectors are of different lengths.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::bitv;
    ///
    /// let a   = 0b01100100;
    /// let b   = 0b01011010;
    /// let res = 0b01000000;
    ///
    /// let mut a = bitv::from_bytes([a]);
    /// let b = bitv::from_bytes([b]);
    ///
    /// assert!(a.intersect(&b));
    /// assert_eq!(a, bitv::from_bytes([res]));
    /// ```
    #[inline]
    pub fn intersect(&mut self, other: &Bitv) -> bool {
        self.process(other, |w1, w2| w1 & w2)
    }

    /// Calculates the difference between two bitvectors.
    ///
    /// Sets each element of `self` to the value of that element minus the
    /// element of `other` at the same index. Both bitvectors must be the same
    /// length. Returns `true` if `self` changed.
    ///
    /// # Failure
    ///
    /// Fails if the bitvectors are of different length.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::bitv;
    ///
    /// let a   = 0b01100100;
    /// let b   = 0b01011010;
    /// let a_b = 0b00100100; // a - b
    /// let b_a = 0b00011010; // b - a
    ///
    /// let mut bva = bitv::from_bytes([a]);
    /// let bvb = bitv::from_bytes([b]);
    ///
    /// assert!(bva.difference(&bvb));
    /// assert_eq!(bva, bitv::from_bytes([a_b]));
    ///
    /// let bva = bitv::from_bytes([a]);
    /// let mut bvb = bitv::from_bytes([b]);
    ///
    /// assert!(bvb.difference(&bva));
    /// assert_eq!(bvb, bitv::from_bytes([b_a]));
    /// ```
    #[inline]
    pub fn difference(&mut self, other: &Bitv) -> bool {
        self.process(other, |w1, w2| w1 & !w2)
    }

    /// Returns `true` if all bits are 1.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::Bitv;
    ///
    /// let mut bv = Bitv::with_capacity(5, true);
    /// assert_eq!(bv.all(), true);
    ///
    /// bv.set(1, false);
    /// assert_eq!(bv.all(), false);
    /// ```
    #[inline]
    pub fn all(&self) -> bool {
        let mut last_word = !0u32;
        // Check that every word but the last is all-ones...
        self.mask_words(0).all(|(_, elem)|
            { let tmp = last_word; last_word = elem; tmp == !0u32 }) &&
        // ...and that the last word is ones as far as it needs to be
        (last_word == ((1 << self.nbits % u32::BITS) - 1) || last_word == !0u32)
    }

    /// Returns an iterator over the elements of the vector in order.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::bitv;
    ///
    /// let bv = bitv::from_bytes([0b01110100, 0b10010010]);
    /// assert_eq!(bv.iter().filter(|x| *x).count(), 7);
    /// ```
    #[inline]
    pub fn iter<'a>(&'a self) -> Bits<'a> {
        Bits {bitv: self, next_idx: 0, end_idx: self.nbits}
    }

    /// Returns `true` if all bits are 0.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::Bitv;
    ///
    /// let mut bv = Bitv::with_capacity(10, false);
    /// assert_eq!(bv.none(), true);
    ///
    /// bv.set(3, true);
    /// assert_eq!(bv.none(), false);
    /// ```
    pub fn none(&self) -> bool {
        self.mask_words(0).all(|(_, w)| w == 0)
    }

    /// Returns `true` if any bit is 1.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::Bitv;
    ///
    /// let mut bv = Bitv::with_capacity(10, false);
    /// assert_eq!(bv.any(), false);
    ///
    /// bv.set(3, true);
    /// assert_eq!(bv.any(), true);
    /// ```
    #[inline]
    pub fn any(&self) -> bool {
        !self.none()
    }

    /// Organises the bits into bytes, such that the first bit in the
    /// `Bitv` becomes the high-order bit of the first byte. If the
    /// size of the `Bitv` is not a multiple of eight then trailing bits
    /// will be filled-in with `false`.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::Bitv;
    ///
    /// let mut bv = Bitv::with_capacity(3, true);
    /// bv.set(1, false);
    ///
    /// assert_eq!(bv.to_bytes(), vec!(0b10100000));
    ///
    /// let mut bv = Bitv::with_capacity(9, false);
    /// bv.set(2, true);
    /// bv.set(8, true);
    ///
    /// assert_eq!(bv.to_bytes(), vec!(0b00100000, 0b10000000));
    /// ```
    pub fn to_bytes(&self) -> Vec<u8> {
        fn bit (bitv: &Bitv, byte: uint, bit: uint) -> u8 {
            let offset = byte * 8 + bit;
            if offset >= bitv.nbits {
                0
            } else {
                bitv.get(offset) as u8 << (7 - bit)
            }
        }

        let len = self.nbits/8 +
                  if self.nbits % 8 == 0 { 0 } else { 1 };
        Vec::from_fn(len, |i|
            bit(self, i, 0) |
            bit(self, i, 1) |
            bit(self, i, 2) |
            bit(self, i, 3) |
            bit(self, i, 4) |
            bit(self, i, 5) |
            bit(self, i, 6) |
            bit(self, i, 7)
        )
    }

    /// Transforms `self` into a `Vec<bool>` by turning each bit into a `bool`.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::bitv;
    ///
    /// let bv = bitv::from_bytes([0b10100000]);
    /// assert_eq!(bv.to_bools(), vec!(true, false, true, false,
    ///                                false, false, false, false));
    /// ```
    pub fn to_bools(&self) -> Vec<bool> {
        Vec::from_fn(self.nbits, |i| self.get(i))
    }

    /// Compares a `Bitv` to a slice of `bool`s.
    /// Both the `Bitv` and slice must have the same length.
    ///
    /// # Failure
    ///
    /// Fails if the the `Bitv` and slice are of different length.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::bitv;
    ///
    /// let bv = bitv::from_bytes([0b10100000]);
    ///
    /// assert!(bv.eq_vec([true, false, true, false,
    ///                    false, false, false, false]));
    /// ```
    pub fn eq_vec(&self, v: &[bool]) -> bool {
        assert_eq!(self.nbits, v.len());
        let mut i = 0;
        while i < self.nbits {
            if self.get(i) != v[i] { return false; }
            i = i + 1;
        }
        true
    }

    /// Shortens a `Bitv`, dropping excess elements.
    ///
    /// If `len` is greater than the vector's current length, this has no
    /// effect.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::bitv;
    ///
    /// let mut bv = bitv::from_bytes([0b01001011]);
    /// bv.truncate(2);
    /// assert!(bv.eq_vec([false, true]));
    /// ```
    #[unstable = "matches collection reform specification, waiting for dust to settle"]
    pub fn truncate(&mut self, len: uint) {
        if len < self.len() {
            self.nbits = len;
            let word_len = (len + u32::BITS - 1) / u32::BITS;
            self.storage.truncate(word_len);
            if len % u32::BITS > 0 {
                let mask = (1 << len % u32::BITS) - 1;
                self.storage[word_len - 1] &= mask;
            }
        }
    }

    /// Grows the vector to be able to store `size` bits without resizing.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::Bitv;
    ///
    /// let mut bv = Bitv::with_capacity(3, false);
    /// bv.reserve(10);
    /// assert_eq!(bv.len(), 3);
    /// assert!(bv.capacity() >= 10);
    /// ```
    pub fn reserve(&mut self, size: uint) {
        let old_size = self.storage.len();
        let new_size = (size + u32::BITS - 1) / u32::BITS;
        if old_size < new_size {
            self.storage.grow(new_size - old_size, 0);
        }
    }

    /// Returns the capacity in bits for this bit vector. Inserting any
    /// element less than this amount will not trigger a resizing.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::Bitv;
    ///
    /// let mut bv = Bitv::new();
    /// bv.reserve(10);
    /// assert!(bv.capacity() >= 10);
    /// ```
    #[inline]
    pub fn capacity(&self) -> uint {
        self.storage.len() * u32::BITS
    }

    /// Grows the `Bitv` in-place, adding `n` copies of `value` to the `Bitv`.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::bitv;
    ///
    /// let mut bv = bitv::from_bytes([0b01001011]);
    /// bv.grow(2, true);
    /// assert_eq!(bv.len(), 10);
    /// assert_eq!(bv.to_bytes(), vec!(0b01001011, 0b11000000));
    /// ```
    pub fn grow(&mut self, n: uint, value: bool) {
        let new_nbits = self.nbits + n;
        let new_nwords = (new_nbits + u32::BITS - 1) / u32::BITS;
        let full_value = if value { !0 } else { 0 };
        // Correct the old tail word
        let old_last_word = (self.nbits + u32::BITS - 1) / u32::BITS - 1;
        if self.nbits % u32::BITS > 0 {
            let overhang = self.nbits % u32::BITS; // # of already-used bits
            let mask = !((1 << overhang) - 1);  // e.g. 5 unused bits => 111110....0
            if value {
                self.storage[old_last_word] |= mask;
            } else {
                self.storage[old_last_word] &= !mask;
            }
        }
        // Fill in words after the old tail word
        let stop_idx = cmp::min(self.storage.len(), new_nwords);
        for idx in range(old_last_word + 1, stop_idx) {
            self.storage[idx] = full_value;
        }
        // Allocate new words, if needed
        if new_nwords > self.storage.len() {
            let to_add = new_nwords - self.storage.len();
            self.storage.grow(to_add, full_value);

            // Zero out and unused bits in the new tail word
            if value {
                let tail_word = new_nwords - 1;
                let used_bits = new_nbits % u32::BITS;
                self.storage[tail_word] &= (1 << used_bits) - 1;
            }
        }
        // Adjust internal bit count
        self.nbits = new_nbits;
    }

    /// Shortens by one element and returns the removed element.
    ///
    /// # Failure
    ///
    /// Assert if empty.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::bitv;
    ///
    /// let mut bv = bitv::from_bytes([0b01001001]);
    /// assert_eq!(bv.pop(), true);
    /// assert_eq!(bv.pop(), false);
    /// assert_eq!(bv.len(), 6);
    /// assert_eq!(bv.to_bytes(), vec!(0b01001000));
    /// ```
    pub fn pop(&mut self) -> bool {
        let ret = self.get(self.nbits - 1);
        // If we are unusing a whole word, make sure it is zeroed out
        if self.nbits % u32::BITS == 1 {
            self.storage[self.nbits / u32::BITS] = 0;
        }
        self.nbits -= 1;
        ret
    }

    /// Pushes a `bool` onto the end.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::Bitv;
    ///
    /// let mut bv = Bitv::new();
    /// bv.push(true);
    /// bv.push(false);
    /// assert!(bv.eq_vec([true, false]));
    /// ```
    pub fn push(&mut self, elem: bool) {
        let insert_pos = self.nbits;
        self.nbits += 1;
        if self.storage.len() * u32::BITS < self.nbits {
            self.storage.push(0);
        }
        self.set(insert_pos, elem);
    }

    /// Return the total number of bits in this vector
    #[inline]
    #[unstable = "matches collection reform specification, waiting for dust to settle"]
    pub fn len(&self) -> uint { self.nbits }

    /// Returns true if there are no bits in this vector
    #[inline]
    #[unstable = "matches collection reform specification, waiting for dust to settle"]
    pub fn is_empty(&self) -> bool { self.len() == 0 }

    /// Clears all bits in this vector.
    #[inline]
    #[unstable = "matches collection reform specification, waiting for dust to settle"]
    pub fn clear(&mut self) {
        for w in self.storage.iter_mut() { *w = 0u32; }
    }
}

/// Transforms a byte-vector into a `Bitv`. Each byte becomes eight bits,
/// with the most significant bits of each byte coming first. Each
/// bit becomes `true` if equal to 1 or `false` if equal to 0.
///
/// # Example
///
/// ```
/// use std::collections::bitv;
///
/// let bv = bitv::from_bytes([0b10100000, 0b00010010]);
/// assert!(bv.eq_vec([true, false, true, false,
///                    false, false, false, false,
///                    false, false, false, true,
///                    false, false, true, false]));
/// ```
pub fn from_bytes(bytes: &[u8]) -> Bitv {
    from_fn(bytes.len() * 8, |i| {
        let b = bytes[i / 8] as u32;
        let offset = i % 8;
        b >> (7 - offset) & 1 == 1
    })
}

/// Creates a `Bitv` of the specified length where the value at each
/// index is `f(index)`.
///
/// # Example
///
/// ```
/// use std::collections::bitv::from_fn;
///
/// let bv = from_fn(5, |i| { i % 2 == 0 });
/// assert!(bv.eq_vec([true, false, true, false, true]));
/// ```
pub fn from_fn(len: uint, f: |index: uint| -> bool) -> Bitv {
    let mut bitv = Bitv::with_capacity(len, false);
    for i in range(0u, len) {
        bitv.set(i, f(i));
    }
    bitv
}

impl Default for Bitv {
    #[inline]
    fn default() -> Bitv { Bitv::new() }
}

impl FromIterator<bool> for Bitv {
    fn from_iter<I:Iterator<bool>>(iterator: I) -> Bitv {
        let mut ret = Bitv::new();
        ret.extend(iterator);
        ret
    }
}

impl Extendable<bool> for Bitv {
    #[inline]
    fn extend<I: Iterator<bool>>(&mut self, mut iterator: I) {
        let (min, _) = iterator.size_hint();
        let nbits = self.nbits;
        self.reserve(nbits + min);
        for element in iterator {
            self.push(element)
        }
    }
}

impl Clone for Bitv {
    #[inline]
    fn clone(&self) -> Bitv {
        Bitv { storage: self.storage.clone(), nbits: self.nbits }
    }

    #[inline]
    fn clone_from(&mut self, source: &Bitv) {
        self.nbits = source.nbits;
        self.storage.clone_from(&source.storage);
    }
}

impl PartialOrd for Bitv {
    #[inline]
    fn partial_cmp(&self, other: &Bitv) -> Option<Ordering> {
        iter::order::partial_cmp(self.iter(), other.iter())
    }
}

impl Ord for Bitv {
    #[inline]
    fn cmp(&self, other: &Bitv) -> Ordering {
        iter::order::cmp(self.iter(), other.iter())
    }
}

impl fmt::Show for Bitv {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        for bit in self.iter() {
            try!(write!(fmt, "{}", if bit { 1u } else { 0u }));
        }
        Ok(())
    }
}

impl<S: hash::Writer> hash::Hash<S> for Bitv {
    fn hash(&self, state: &mut S) {
        self.nbits.hash(state);
        for (_, elem) in self.mask_words(0) {
            elem.hash(state);
        }
    }
}

impl cmp::PartialEq for Bitv {
    #[inline]
    fn eq(&self, other: &Bitv) -> bool {
        if self.nbits != other.nbits {
            return false;
        }
        self.mask_words(0).zip(other.mask_words(0)).all(|((_, w1), (_, w2))| w1 == w2)
    }
}

impl cmp::Eq for Bitv {}

/// An iterator for `Bitv`.
pub struct Bits<'a> {
    bitv: &'a Bitv,
    next_idx: uint,
    end_idx: uint,
}

impl<'a> Iterator<bool> for Bits<'a> {
    #[inline]
    fn next(&mut self) -> Option<bool> {
        if self.next_idx != self.end_idx {
            let idx = self.next_idx;
            self.next_idx += 1;
            Some(self.bitv.get(idx))
        } else {
            None
        }
    }

    fn size_hint(&self) -> (uint, Option<uint>) {
        let rem = self.end_idx - self.next_idx;
        (rem, Some(rem))
    }
}

impl<'a> DoubleEndedIterator<bool> for Bits<'a> {
    #[inline]
    fn next_back(&mut self) -> Option<bool> {
        if self.next_idx != self.end_idx {
            self.end_idx -= 1;
            Some(self.bitv.get(self.end_idx))
        } else {
            None
        }
    }
}

impl<'a> ExactSize<bool> for Bits<'a> {}

impl<'a> RandomAccessIterator<bool> for Bits<'a> {
    #[inline]
    fn indexable(&self) -> uint {
        self.end_idx - self.next_idx
    }

    #[inline]
    fn idx(&mut self, index: uint) -> Option<bool> {
        if index >= self.indexable() {
            None
        } else {
            Some(self.bitv.get(index))
        }
    }
}

/// An implementation of a set using a bit vector as an underlying
/// representation for holding unsigned numerical elements.
///
/// It should also be noted that the amount of storage necessary for holding a
/// set of objects is proportional to the maximum of the objects when viewed
/// as a `uint`.
///
/// # Example
///
/// ```
/// use std::collections::{BitvSet, Bitv};
/// use std::collections::bitv;
///
/// // It's a regular set
/// let mut s = BitvSet::new();
/// s.insert(0);
/// s.insert(3);
/// s.insert(7);
///
/// s.remove(&7);
///
/// if !s.contains(&7) {
///     println!("There is no 7");
/// }
///
/// // Can initialize from a `Bitv`
/// let other = BitvSet::from_bitv(bitv::from_bytes([0b11010000]));
///
/// s.union_with(&other);
///
/// // Print 0, 1, 3 in some order
/// for x in s.iter() {
///     println!("{}", x);
/// }
///
/// // Can convert back to a `Bitv`
/// let bv: Bitv = s.into_bitv();
/// assert!(bv.get(3));
/// ```
#[deriving(Clone)]
pub struct BitvSet(Bitv);

impl Default for BitvSet {
    #[inline]
    fn default() -> BitvSet { BitvSet::new() }
}

impl FromIterator<bool> for BitvSet {
    fn from_iter<I:Iterator<bool>>(iterator: I) -> BitvSet {
        let mut ret = BitvSet::new();
        ret.extend(iterator);
        ret
    }
}

impl Extendable<bool> for BitvSet {
    #[inline]
    fn extend<I: Iterator<bool>>(&mut self, iterator: I) {
        let &BitvSet(ref mut self_bitv) = self;
        self_bitv.extend(iterator);
    }
}

impl PartialOrd for BitvSet {
    #[inline]
    fn partial_cmp(&self, other: &BitvSet) -> Option<Ordering> {
        let (a_iter, b_iter) = match_words(self.get_ref(), other.get_ref());
        iter::order::partial_cmp(a_iter, b_iter)
    }
}

impl Ord for BitvSet {
    #[inline]
    fn cmp(&self, other: &BitvSet) -> Ordering {
        let (a_iter, b_iter) = match_words(self.get_ref(), other.get_ref());
        iter::order::cmp(a_iter, b_iter)
    }
}

impl cmp::PartialEq for BitvSet {
    #[inline]
    fn eq(&self, other: &BitvSet) -> bool {
        let (a_iter, b_iter) = match_words(self.get_ref(), other.get_ref());
        iter::order::eq(a_iter, b_iter)
    }
}

impl cmp::Eq for BitvSet {}

impl BitvSet {
    /// Creates a new bit vector set with initially no contents.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::BitvSet;
    /// let mut s = BitvSet::new();
    /// ```
    #[inline]
    #[unstable = "matches collection reform specification, waiting for dust to settle"]
    pub fn new() -> BitvSet {
        BitvSet(Bitv::new())
    }

    /// Creates a new bit vector set with initially no contents, able to
    /// hold `nbits` elements without resizing.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::BitvSet;
    /// let mut s = BitvSet::with_capacity(100);
    /// assert!(s.capacity() >= 100);
    /// ```
    #[inline]
    #[unstable = "matches collection reform specification, waiting for dust to settle"]
    pub fn with_capacity(nbits: uint) -> BitvSet {
        let bitv = Bitv::with_capacity(nbits, false);
        BitvSet::from_bitv(bitv)
    }

    /// Creates a new bit vector set from the given bit vector.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::{bitv, BitvSet};
    ///
    /// let bv = bitv::from_bytes([0b01100000]);
    /// let s = BitvSet::from_bitv(bv);
    ///
    /// // Print 1, 2 in arbitrary order
    /// for x in s.iter() {
    ///     println!("{}", x);
    /// }
    /// ```
    #[inline]
    pub fn from_bitv(mut bitv: Bitv) -> BitvSet {
        // Mark every bit as valid
        bitv.nbits = bitv.capacity();
        BitvSet(bitv)
    }

    /// Returns the capacity in bits for this bit vector. Inserting any
    /// element less than this amount will not trigger a resizing.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::BitvSet;
    ///
    /// let mut s = BitvSet::with_capacity(100);
    /// assert!(s.capacity() >= 100);
    /// ```
    #[inline]
    #[unstable = "matches collection reform specification, waiting for dust to settle"]
    pub fn capacity(&self) -> uint {
        let &BitvSet(ref bitv) = self;
        bitv.capacity()
    }

    /// Grows the underlying vector to be able to store `size` bits.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::BitvSet;
    ///
    /// let mut s = BitvSet::new();
    /// s.reserve(10);
    /// assert!(s.capacity() >= 10);
    /// ```
    pub fn reserve(&mut self, size: uint) {
        let &BitvSet(ref mut bitv) = self;
        bitv.reserve(size);
        if bitv.nbits < size {
            bitv.nbits = bitv.capacity();
        }
    }

    /// Consumes this set to return the underlying bit vector.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::BitvSet;
    ///
    /// let mut s = BitvSet::new();
    /// s.insert(0);
    /// s.insert(3);
    ///
    /// let bv = s.into_bitv();
    /// assert!(bv.get(0));
    /// assert!(bv.get(3));
    /// ```
    #[inline]
    pub fn into_bitv(self) -> Bitv {
        let BitvSet(bitv) = self;
        bitv
    }

    /// Returns a reference to the underlying bit vector.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::BitvSet;
    ///
    /// let mut s = BitvSet::new();
    /// s.insert(0);
    ///
    /// let bv = s.get_ref();
    /// assert_eq!(bv[0], true);
    /// ```
    #[inline]
    pub fn get_ref<'a>(&'a self) -> &'a Bitv {
        let &BitvSet(ref bitv) = self;
        bitv
    }

    #[inline]
    fn other_op(&mut self, other: &BitvSet, f: |u32, u32| -> u32) {
        // Expand the vector if necessary
        self.reserve(other.capacity());

        // Unwrap Bitvs
        let &BitvSet(ref mut self_bitv) = self;
        let &BitvSet(ref other_bitv) = other;

        // virtually pad other with 0's for equal lengths
        let mut other_words = {
            let (_, result) = match_words(self_bitv, other_bitv);
            result
        };

        // Apply values found in other
        for (i, w) in other_words {
            let old = self_bitv.storage[i];
            let new = f(old, w);
            self_bitv.storage[i] = new;
        }
    }

    /// Truncates the underlying vector to the least length required.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::BitvSet;
    ///
    /// let mut s = BitvSet::new();
    /// s.insert(32183231);
    /// s.remove(&32183231);
    ///
    /// // Internal storage will probably be bigger than necessary
    /// println!("old capacity: {}", s.capacity());
    ///
    /// // Now should be smaller
    /// s.shrink_to_fit();
    /// println!("new capacity: {}", s.capacity());
    /// ```
    #[inline]
    #[unstable = "matches collection reform specification, waiting for dust to settle"]
    pub fn shrink_to_fit(&mut self) {
        let &BitvSet(ref mut bitv) = self;
        // Obtain original length
        let old_len = bitv.storage.len();
        // Obtain coarse trailing zero length
        let n = bitv.storage.iter().rev().take_while(|&&n| n == 0).count();
        // Truncate
        let trunc_len = cmp::max(old_len - n, 1);
        bitv.storage.truncate(trunc_len);
        bitv.nbits = trunc_len * u32::BITS;
    }

    /// Iterator over each u32 stored in the `BitvSet`.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::BitvSet;
    /// use std::collections::bitv;
    ///
    /// let s = BitvSet::from_bitv(bitv::from_bytes([0b01001010]));
    ///
    /// // Print 1, 4, 6 in arbitrary order
    /// for x in s.iter() {
    ///     println!("{}", x);
    /// }
    /// ```
    #[inline]
    #[unstable = "matches collection reform specification, waiting for dust to settle"]
    pub fn iter<'a>(&'a self) -> BitPositions<'a> {
        BitPositions {set: self, next_idx: 0u}
    }

    /// Iterator over each u32 stored in `self` union `other`.
    /// See [union_with](#method.union_with) for an efficient in-place version.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::BitvSet;
    /// use std::collections::bitv;
    ///
    /// let a = BitvSet::from_bitv(bitv::from_bytes([0b01101000]));
    /// let b = BitvSet::from_bitv(bitv::from_bytes([0b10100000]));
    ///
    /// // Print 0, 1, 2, 4 in arbitrary order
    /// for x in a.union(&b) {
    ///     println!("{}", x);
    /// }
    /// ```
    #[inline]
    #[unstable = "matches collection reform specification, waiting for dust to settle"]
    pub fn union<'a>(&'a self, other: &'a BitvSet) -> TwoBitPositions<'a> {
        TwoBitPositions {
            set: self,
            other: other,
            merge: |w1, w2| w1 | w2,
            current_word: 0u32,
            next_idx: 0u
        }
    }

    /// Iterator over each uint stored in `self` intersect `other`.
    /// See [intersect_with](#method.intersect_with) for an efficient in-place version.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::BitvSet;
    /// use std::collections::bitv;
    ///
    /// let a = BitvSet::from_bitv(bitv::from_bytes([0b01101000]));
    /// let b = BitvSet::from_bitv(bitv::from_bytes([0b10100000]));
    ///
    /// // Print 2
    /// for x in a.intersection(&b) {
    ///     println!("{}", x);
    /// }
    /// ```
    #[inline]
    #[unstable = "matches collection reform specification, waiting for dust to settle"]
    pub fn intersection<'a>(&'a self, other: &'a BitvSet) -> Take<TwoBitPositions<'a>> {
        let min = cmp::min(self.capacity(), other.capacity());
        TwoBitPositions {
            set: self,
            other: other,
            merge: |w1, w2| w1 & w2,
            current_word: 0u32,
            next_idx: 0
        }.take(min)
    }

    /// Iterator over each uint stored in the `self` setminus `other`.
    /// See [difference_with](#method.difference_with) for an efficient in-place version.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::BitvSet;
    /// use std::collections::bitv;
    ///
    /// let a = BitvSet::from_bitv(bitv::from_bytes([0b01101000]));
    /// let b = BitvSet::from_bitv(bitv::from_bytes([0b10100000]));
    ///
    /// // Print 1, 4 in arbitrary order
    /// for x in a.difference(&b) {
    ///     println!("{}", x);
    /// }
    ///
    /// // Note that difference is not symmetric,
    /// // and `b - a` means something else.
    /// // This prints 0
    /// for x in b.difference(&a) {
    ///     println!("{}", x);
    /// }
    /// ```
    #[inline]
    #[unstable = "matches collection reform specification, waiting for dust to settle"]
    pub fn difference<'a>(&'a self, other: &'a BitvSet) -> TwoBitPositions<'a> {
        TwoBitPositions {
            set: self,
            other: other,
            merge: |w1, w2| w1 & !w2,
            current_word: 0u32,
            next_idx: 0
        }
    }

    /// Iterator over each u32 stored in the symmetric difference of `self` and `other`.
    /// See [symmetric_difference_with](#method.symmetric_difference_with) for
    /// an efficient in-place version.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::BitvSet;
    /// use std::collections::bitv;
    ///
    /// let a = BitvSet::from_bitv(bitv::from_bytes([0b01101000]));
    /// let b = BitvSet::from_bitv(bitv::from_bytes([0b10100000]));
    ///
    /// // Print 0, 1, 4 in arbitrary order
    /// for x in a.symmetric_difference(&b) {
    ///     println!("{}", x);
    /// }
    /// ```
    #[inline]
    #[unstable = "matches collection reform specification, waiting for dust to settle"]
    pub fn symmetric_difference<'a>(&'a self, other: &'a BitvSet) -> TwoBitPositions<'a> {
        TwoBitPositions {
            set: self,
            other: other,
            merge: |w1, w2| w1 ^ w2,
            current_word: 0u32,
            next_idx: 0
        }
    }

    /// Unions in-place with the specified other bit vector.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::BitvSet;
    /// use std::collections::bitv;
    ///
    /// let a   = 0b01101000;
    /// let b   = 0b10100000;
    /// let res = 0b11101000;
    ///
    /// let mut a = BitvSet::from_bitv(bitv::from_bytes([a]));
    /// let b = BitvSet::from_bitv(bitv::from_bytes([b]));
    /// let res = BitvSet::from_bitv(bitv::from_bytes([res]));
    ///
    /// a.union_with(&b);
    /// assert_eq!(a, res);
    /// ```
    #[inline]
    pub fn union_with(&mut self, other: &BitvSet) {
        self.other_op(other, |w1, w2| w1 | w2);
    }

    /// Intersects in-place with the specified other bit vector.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::BitvSet;
    /// use std::collections::bitv;
    ///
    /// let a   = 0b01101000;
    /// let b   = 0b10100000;
    /// let res = 0b00100000;
    ///
    /// let mut a = BitvSet::from_bitv(bitv::from_bytes([a]));
    /// let b = BitvSet::from_bitv(bitv::from_bytes([b]));
    /// let res = BitvSet::from_bitv(bitv::from_bytes([res]));
    ///
    /// a.intersect_with(&b);
    /// assert_eq!(a, res);
    /// ```
    #[inline]
    pub fn intersect_with(&mut self, other: &BitvSet) {
        self.other_op(other, |w1, w2| w1 & w2);
    }

    /// Makes this bit vector the difference with the specified other bit vector
    /// in-place.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::BitvSet;
    /// use std::collections::bitv;
    ///
    /// let a   = 0b01101000;
    /// let b   = 0b10100000;
    /// let a_b = 0b01001000; // a - b
    /// let b_a = 0b10000000; // b - a
    ///
    /// let mut bva = BitvSet::from_bitv(bitv::from_bytes([a]));
    /// let bvb = BitvSet::from_bitv(bitv::from_bytes([b]));
    /// let bva_b = BitvSet::from_bitv(bitv::from_bytes([a_b]));
    /// let bvb_a = BitvSet::from_bitv(bitv::from_bytes([b_a]));
    ///
    /// bva.difference_with(&bvb);
    /// assert_eq!(bva, bva_b);
    ///
    /// let bva = BitvSet::from_bitv(bitv::from_bytes([a]));
    /// let mut bvb = BitvSet::from_bitv(bitv::from_bytes([b]));
    ///
    /// bvb.difference_with(&bva);
    /// assert_eq!(bvb, bvb_a);
    /// ```
    #[inline]
    pub fn difference_with(&mut self, other: &BitvSet) {
        self.other_op(other, |w1, w2| w1 & !w2);
    }

    /// Makes this bit vector the symmetric difference with the specified other
    /// bit vector in-place.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::BitvSet;
    /// use std::collections::bitv;
    ///
    /// let a   = 0b01101000;
    /// let b   = 0b10100000;
    /// let res = 0b11001000;
    ///
    /// let mut a = BitvSet::from_bitv(bitv::from_bytes([a]));
    /// let b = BitvSet::from_bitv(bitv::from_bytes([b]));
    /// let res = BitvSet::from_bitv(bitv::from_bytes([res]));
    ///
    /// a.symmetric_difference_with(&b);
    /// assert_eq!(a, res);
    /// ```
    #[inline]
    pub fn symmetric_difference_with(&mut self, other: &BitvSet) {
        self.other_op(other, |w1, w2| w1 ^ w2);
    }

    /// Return the number of set bits in this set.
    #[inline]
    #[unstable = "matches collection reform specification, waiting for dust to settle"]
    pub fn len(&self) -> uint  {
        let &BitvSet(ref bitv) = self;
        bitv.storage.iter().fold(0, |acc, &n| acc + n.count_ones())
    }

    /// Returns whether there are no bits set in this set
    #[inline]
    #[unstable = "matches collection reform specification, waiting for dust to settle"]
    pub fn is_empty(&self) -> bool {
        let &BitvSet(ref bitv) = self;
        bitv.storage.iter().all(|&n| n == 0)
    }

    /// Clears all bits in this set
    #[inline]
    #[unstable = "matches collection reform specification, waiting for dust to settle"]
    pub fn clear(&mut self) {
        let &BitvSet(ref mut bitv) = self;
        bitv.clear();
    }

    /// Returns `true` if this set contains the specified integer.
    #[inline]
    #[unstable = "matches collection reform specification, waiting for dust to settle"]
    pub fn contains(&self, value: &uint) -> bool {
        let &BitvSet(ref bitv) = self;
        *value < bitv.nbits && bitv.get(*value)
    }

    /// Returns `true` if the set has no elements in common with `other`.
    /// This is equivalent to checking for an empty intersection.
    #[inline]
    #[unstable = "matches collection reform specification, waiting for dust to settle"]
    pub fn is_disjoint(&self, other: &BitvSet) -> bool {
        self.intersection(other).next().is_none()
    }

    /// Returns `true` if the set is a subset of another.
    #[inline]
    #[unstable = "matches collection reform specification, waiting for dust to settle"]
    pub fn is_subset(&self, other: &BitvSet) -> bool {
        let &BitvSet(ref self_bitv) = self;
        let &BitvSet(ref other_bitv) = other;

        // Check that `self` intersect `other` is self
        self_bitv.mask_words(0).zip(other_bitv.mask_words(0))
                               .all(|((_, w1), (_, w2))| w1 & w2 == w1) &&
        // Check that `self` setminus `other` is empty
        self_bitv.mask_words(other_bitv.storage.len()).all(|(_, w)| w == 0)
    }

    /// Returns `true` if the set is a superset of another.
    #[inline]
    #[unstable = "matches collection reform specification, waiting for dust to settle"]
    pub fn is_superset(&self, other: &BitvSet) -> bool {
        other.is_subset(self)
    }

    /// Adds a value to the set. Returns `true` if the value was not already
    /// present in the set.
    #[unstable = "matches collection reform specification, waiting for dust to settle"]
    pub fn insert(&mut self, value: uint) -> bool {
        if self.contains(&value) {
            return false;
        }

        // Ensure we have enough space to hold the new element
        if value >= self.capacity() {
            let new_cap = cmp::max(value + 1, self.capacity() * 2);
            self.reserve(new_cap);
        }

        let &BitvSet(ref mut bitv) = self;
        bitv.set(value, true);
        return true;
    }

    /// Removes a value from the set. Returns `true` if the value was
    /// present in the set.
    #[unstable = "matches collection reform specification, waiting for dust to settle"]
    pub fn remove(&mut self, value: &uint) -> bool {
        if !self.contains(value) {
            return false;
        }
        let &BitvSet(ref mut bitv) = self;
        bitv.set(*value, false);
        return true;
    }
}

impl fmt::Show for BitvSet {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        try!(write!(fmt, "{{"));
        let mut first = true;
        for n in self.iter() {
            if !first {
                try!(write!(fmt, ", "));
            }
            try!(write!(fmt, "{}", n));
            first = false;
        }
        write!(fmt, "}}")
    }
}

impl<S: hash::Writer> hash::Hash<S> for BitvSet {
    fn hash(&self, state: &mut S) {
        for pos in self.iter() {
            pos.hash(state);
        }
    }
}

/// An iterator for `BitvSet`.
pub struct BitPositions<'a> {
    set: &'a BitvSet,
    next_idx: uint
}

/// An iterator combining two `BitvSet` iterators.
pub struct TwoBitPositions<'a> {
    set: &'a BitvSet,
    other: &'a BitvSet,
    merge: |u32, u32|: 'a -> u32,
    current_word: u32,
    next_idx: uint
}

impl<'a> Iterator<uint> for BitPositions<'a> {
    fn next(&mut self) -> Option<uint> {
        while self.next_idx < self.set.capacity() {
            let idx = self.next_idx;
            self.next_idx += 1;

            if self.set.contains(&idx) {
                return Some(idx);
            }
        }

        return None;
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        (0, Some(self.set.capacity() - self.next_idx))
    }
}

impl<'a> Iterator<uint> for TwoBitPositions<'a> {
    fn next(&mut self) -> Option<uint> {
        while self.next_idx < self.set.capacity() ||
              self.next_idx < self.other.capacity() {
            let bit_idx = self.next_idx % u32::BITS;
            if bit_idx == 0 {
                let &BitvSet(ref s_bitv) = self.set;
                let &BitvSet(ref o_bitv) = self.other;
                // Merging the two words is a bit of an awkward dance since
                // one Bitv might be longer than the other
                let word_idx = self.next_idx / u32::BITS;
                let w1 = if word_idx < s_bitv.storage.len() {
                             s_bitv.storage[word_idx]
                         } else { 0 };
                let w2 = if word_idx < o_bitv.storage.len() {
                             o_bitv.storage[word_idx]
                         } else { 0 };
                self.current_word = (self.merge)(w1, w2);
            }

            self.next_idx += 1;
            if self.current_word & (1 << bit_idx) != 0 {
                return Some(self.next_idx - 1);
            }
        }
        return None;
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        let cap = cmp::max(self.set.capacity(), self.other.capacity());
        (0, Some(cap - self.next_idx))
    }
}

#[cfg(test)]
mod tests {
    use std::prelude::*;
    use std::iter::range_step;
    use std::u32;
    use std::rand;
    use std::rand::Rng;
    use test::Bencher;

    use super::{Bitv, BitvSet, from_fn, from_bytes};
    use bitv;
    use vec::Vec;

    static BENCH_BITS : uint = 1 << 14;

    #[test]
    fn test_to_str() {
        let zerolen = Bitv::new();
        assert_eq!(zerolen.to_string().as_slice(), "");

        let eightbits = Bitv::with_capacity(8u, false);
        assert_eq!(eightbits.to_string().as_slice(), "00000000")
    }

    #[test]
    fn test_0_elements() {
        let act = Bitv::new();
        let exp = Vec::from_elem(0u, false);
        assert!(act.eq_vec(exp.as_slice()));
    }

    #[test]
    fn test_1_element() {
        let mut act = Bitv::with_capacity(1u, false);
        assert!(act.eq_vec([false]));
        act = Bitv::with_capacity(1u, true);
        assert!(act.eq_vec([true]));
    }

    #[test]
    fn test_2_elements() {
        let mut b = bitv::Bitv::with_capacity(2, false);
        b.set(0, true);
        b.set(1, false);
        assert_eq!(b.to_string().as_slice(), "10");
    }

    #[test]
    fn test_10_elements() {
        let mut act;
        // all 0

        act = Bitv::with_capacity(10u, false);
        assert!((act.eq_vec(
                    [false, false, false, false, false, false, false, false, false, false])));
        // all 1

        act = Bitv::with_capacity(10u, true);
        assert!((act.eq_vec([true, true, true, true, true, true, true, true, true, true])));
        // mixed

        act = Bitv::with_capacity(10u, false);
        act.set(0u, true);
        act.set(1u, true);
        act.set(2u, true);
        act.set(3u, true);
        act.set(4u, true);
        assert!((act.eq_vec([true, true, true, true, true, false, false, false, false, false])));
        // mixed

        act = Bitv::with_capacity(10u, false);
        act.set(5u, true);
        act.set(6u, true);
        act.set(7u, true);
        act.set(8u, true);
        act.set(9u, true);
        assert!((act.eq_vec([false, false, false, false, false, true, true, true, true, true])));
        // mixed

        act = Bitv::with_capacity(10u, false);
        act.set(0u, true);
        act.set(3u, true);
        act.set(6u, true);
        act.set(9u, true);
        assert!((act.eq_vec([true, false, false, true, false, false, true, false, false, true])));
    }

    #[test]
    fn test_31_elements() {
        let mut act;
        // all 0

        act = Bitv::with_capacity(31u, false);
        assert!(act.eq_vec(
                [false, false, false, false, false, false, false, false, false, false, false,
                false, false, false, false, false, false, false, false, false, false, false, false,
                false, false, false, false, false, false, false, false]));
        // all 1

        act = Bitv::with_capacity(31u, true);
        assert!(act.eq_vec(
                [true, true, true, true, true, true, true, true, true, true, true, true, true,
                true, true, true, true, true, true, true, true, true, true, true, true, true, true,
                true, true, true, true]));
        // mixed

        act = Bitv::with_capacity(31u, false);
        act.set(0u, true);
        act.set(1u, true);
        act.set(2u, true);
        act.set(3u, true);
        act.set(4u, true);
        act.set(5u, true);
        act.set(6u, true);
        act.set(7u, true);
        assert!(act.eq_vec(
                [true, true, true, true, true, true, true, true, false, false, false, false, false,
                false, false, false, false, false, false, false, false, false, false, false, false,
                false, false, false, false, false, false]));
        // mixed

        act = Bitv::with_capacity(31u, false);
        act.set(16u, true);
        act.set(17u, true);
        act.set(18u, true);
        act.set(19u, true);
        act.set(20u, true);
        act.set(21u, true);
        act.set(22u, true);
        act.set(23u, true);
        assert!(act.eq_vec(
                [false, false, false, false, false, false, false, false, false, false, false,
                false, false, false, false, false, true, true, true, true, true, true, true, true,
                false, false, false, false, false, false, false]));
        // mixed

        act = Bitv::with_capacity(31u, false);
        act.set(24u, true);
        act.set(25u, true);
        act.set(26u, true);
        act.set(27u, true);
        act.set(28u, true);
        act.set(29u, true);
        act.set(30u, true);
        assert!(act.eq_vec(
                [false, false, false, false, false, false, false, false, false, false, false,
                false, false, false, false, false, false, false, false, false, false, false, false,
                false, true, true, true, true, true, true, true]));
        // mixed

        act = Bitv::with_capacity(31u, false);
        act.set(3u, true);
        act.set(17u, true);
        act.set(30u, true);
        assert!(act.eq_vec(
                [false, false, false, true, false, false, false, false, false, false, false, false,
                false, false, false, false, false, true, false, false, false, false, false, false,
                false, false, false, false, false, false, true]));
    }

    #[test]
    fn test_32_elements() {
        let mut act;
        // all 0

        act = Bitv::with_capacity(32u, false);
        assert!(act.eq_vec(
                [false, false, false, false, false, false, false, false, false, false, false,
                false, false, false, false, false, false, false, false, false, false, false, false,
                false, false, false, false, false, false, false, false, false]));
        // all 1

        act = Bitv::with_capacity(32u, true);
        assert!(act.eq_vec(
                [true, true, true, true, true, true, true, true, true, true, true, true, true,
                true, true, true, true, true, true, true, true, true, true, true, true, true, true,
                true, true, true, true, true]));
        // mixed

        act = Bitv::with_capacity(32u, false);
        act.set(0u, true);
        act.set(1u, true);
        act.set(2u, true);
        act.set(3u, true);
        act.set(4u, true);
        act.set(5u, true);
        act.set(6u, true);
        act.set(7u, true);
        assert!(act.eq_vec(
                [true, true, true, true, true, true, true, true, false, false, false, false, false,
                false, false, false, false, false, false, false, false, false, false, false, false,
                false, false, false, false, false, false, false]));
        // mixed

        act = Bitv::with_capacity(32u, false);
        act.set(16u, true);
        act.set(17u, true);
        act.set(18u, true);
        act.set(19u, true);
        act.set(20u, true);
        act.set(21u, true);
        act.set(22u, true);
        act.set(23u, true);
        assert!(act.eq_vec(
                [false, false, false, false, false, false, false, false, false, false, false,
                false, false, false, false, false, true, true, true, true, true, true, true, true,
                false, false, false, false, false, false, false, false]));
        // mixed

        act = Bitv::with_capacity(32u, false);
        act.set(24u, true);
        act.set(25u, true);
        act.set(26u, true);
        act.set(27u, true);
        act.set(28u, true);
        act.set(29u, true);
        act.set(30u, true);
        act.set(31u, true);
        assert!(act.eq_vec(
                [false, false, false, false, false, false, false, false, false, false, false,
                false, false, false, false, false, false, false, false, false, false, false, false,
                false, true, true, true, true, true, true, true, true]));
        // mixed

        act = Bitv::with_capacity(32u, false);
        act.set(3u, true);
        act.set(17u, true);
        act.set(30u, true);
        act.set(31u, true);
        assert!(act.eq_vec(
                [false, false, false, true, false, false, false, false, false, false, false, false,
                false, false, false, false, false, true, false, false, false, false, false, false,
                false, false, false, false, false, false, true, true]));
    }

    #[test]
    fn test_33_elements() {
        let mut act;
        // all 0

        act = Bitv::with_capacity(33u, false);
        assert!(act.eq_vec(
                [false, false, false, false, false, false, false, false, false, false, false,
                false, false, false, false, false, false, false, false, false, false, false, false,
                false, false, false, false, false, false, false, false, false, false]));
        // all 1

        act = Bitv::with_capacity(33u, true);
        assert!(act.eq_vec(
                [true, true, true, true, true, true, true, true, true, true, true, true, true,
                true, true, true, true, true, true, true, true, true, true, true, true, true, true,
                true, true, true, true, true, true]));
        // mixed

        act = Bitv::with_capacity(33u, false);
        act.set(0u, true);
        act.set(1u, true);
        act.set(2u, true);
        act.set(3u, true);
        act.set(4u, true);
        act.set(5u, true);
        act.set(6u, true);
        act.set(7u, true);
        assert!(act.eq_vec(
                [true, true, true, true, true, true, true, true, false, false, false, false, false,
                false, false, false, false, false, false, false, false, false, false, false, false,
                false, false, false, false, false, false, false, false]));
        // mixed

        act = Bitv::with_capacity(33u, false);
        act.set(16u, true);
        act.set(17u, true);
        act.set(18u, true);
        act.set(19u, true);
        act.set(20u, true);
        act.set(21u, true);
        act.set(22u, true);
        act.set(23u, true);
        assert!(act.eq_vec(
                [false, false, false, false, false, false, false, false, false, false, false,
                false, false, false, false, false, true, true, true, true, true, true, true, true,
                false, false, false, false, false, false, false, false, false]));
        // mixed

        act = Bitv::with_capacity(33u, false);
        act.set(24u, true);
        act.set(25u, true);
        act.set(26u, true);
        act.set(27u, true);
        act.set(28u, true);
        act.set(29u, true);
        act.set(30u, true);
        act.set(31u, true);
        assert!(act.eq_vec(
                [false, false, false, false, false, false, false, false, false, false, false,
                false, false, false, false, false, false, false, false, false, false, false, false,
                false, true, true, true, true, true, true, true, true, false]));
        // mixed

        act = Bitv::with_capacity(33u, false);
        act.set(3u, true);
        act.set(17u, true);
        act.set(30u, true);
        act.set(31u, true);
        act.set(32u, true);
        assert!(act.eq_vec(
                [false, false, false, true, false, false, false, false, false, false, false, false,
                false, false, false, false, false, true, false, false, false, false, false, false,
                false, false, false, false, false, false, true, true, true]));
    }

    #[test]
    fn test_equal_differing_sizes() {
        let v0 = Bitv::with_capacity(10u, false);
        let v1 = Bitv::with_capacity(11u, false);
        assert!(v0 != v1);
    }

    #[test]
    fn test_equal_greatly_differing_sizes() {
        let v0 = Bitv::with_capacity(10u, false);
        let v1 = Bitv::with_capacity(110u, false);
        assert!(v0 != v1);
    }

    #[test]
    fn test_equal_sneaky_small() {
        let mut a = bitv::Bitv::with_capacity(1, false);
        a.set(0, true);

        let mut b = bitv::Bitv::with_capacity(1, true);
        b.set(0, true);

        assert_eq!(a, b);
    }

    #[test]
    fn test_equal_sneaky_big() {
        let mut a = bitv::Bitv::with_capacity(100, false);
        for i in range(0u, 100) {
            a.set(i, true);
        }

        let mut b = bitv::Bitv::with_capacity(100, true);
        for i in range(0u, 100) {
            b.set(i, true);
        }

        assert_eq!(a, b);
    }

    #[test]
    fn test_from_bytes() {
        let bitv = from_bytes([0b10110110, 0b00000000, 0b11111111]);
        let str = format!("{}{}{}", "10110110", "00000000", "11111111");
        assert_eq!(bitv.to_string().as_slice(), str.as_slice());
    }

    #[test]
    fn test_to_bytes() {
        let mut bv = Bitv::with_capacity(3, true);
        bv.set(1, false);
        assert_eq!(bv.to_bytes(), vec!(0b10100000));

        let mut bv = Bitv::with_capacity(9, false);
        bv.set(2, true);
        bv.set(8, true);
        assert_eq!(bv.to_bytes(), vec!(0b00100000, 0b10000000));
    }

    #[test]
    fn test_from_bools() {
        let bools = vec![true, false, true, true];
        let bitv: Bitv = bools.iter().map(|n| *n).collect();
        assert_eq!(bitv.to_string().as_slice(), "1011");
    }

    #[test]
    fn test_bitv_set_from_bools() {
        let bools = vec![true, false, true, true];
        let a: BitvSet = bools.iter().map(|n| *n).collect();
        let mut b = BitvSet::new();
        b.insert(0);
        b.insert(2);
        b.insert(3);
        assert_eq!(a, b);
    }

    #[test]
    fn test_to_bools() {
        let bools = vec!(false, false, true, false, false, true, true, false);
        assert_eq!(from_bytes([0b00100110]).iter().collect::<Vec<bool>>(), bools);
    }

    #[test]
    fn test_bitv_iterator() {
        let bools = vec![true, false, true, true];
        let bitv: Bitv = bools.iter().map(|n| *n).collect();

        assert_eq!(bitv.iter().collect::<Vec<bool>>(), bools)

        let long = Vec::from_fn(10000, |i| i % 2 == 0);
        let bitv: Bitv = long.iter().map(|n| *n).collect();
        assert_eq!(bitv.iter().collect::<Vec<bool>>(), long)
    }

    #[test]
    fn test_bitv_set_iterator() {
        let bools = [true, false, true, true];
        let bitv: BitvSet = bools.iter().map(|n| *n).collect();

        let idxs: Vec<uint> = bitv.iter().collect();
        assert_eq!(idxs, vec!(0, 2, 3));

        let long: BitvSet = range(0u, 10000).map(|n| n % 2 == 0).collect();
        let real = range_step(0, 10000, 2).collect::<Vec<uint>>();

        let idxs: Vec<uint> = long.iter().collect();
        assert_eq!(idxs, real);
    }

    #[test]
    fn test_bitv_set_frombitv_init() {
        let bools = [true, false];
        let lengths = [10, 64, 100];
        for &b in bools.iter() {
            for &l in lengths.iter() {
                let bitset = BitvSet::from_bitv(Bitv::with_capacity(l, b));
                assert_eq!(bitset.contains(&1u), b)
                assert_eq!(bitset.contains(&(l-1u)), b)
                assert!(!bitset.contains(&l))
            }
        }
    }

    #[test]
    fn test_small_difference() {
        let mut b1 = Bitv::with_capacity(3, false);
        let mut b2 = Bitv::with_capacity(3, false);
        b1.set(0, true);
        b1.set(1, true);
        b2.set(1, true);
        b2.set(2, true);
        assert!(b1.difference(&b2));
        assert!(b1.get(0));
        assert!(!b1.get(1));
        assert!(!b1.get(2));
    }

    #[test]
    fn test_big_difference() {
        let mut b1 = Bitv::with_capacity(100, false);
        let mut b2 = Bitv::with_capacity(100, false);
        b1.set(0, true);
        b1.set(40, true);
        b2.set(40, true);
        b2.set(80, true);
        assert!(b1.difference(&b2));
        assert!(b1.get(0));
        assert!(!b1.get(40));
        assert!(!b1.get(80));
    }

    #[test]
    fn test_small_clear() {
        let mut b = Bitv::with_capacity(14, true);
        b.clear();
        assert!(b.none());
    }

    #[test]
    fn test_big_clear() {
        let mut b = Bitv::with_capacity(140, true);
        b.clear();
        assert!(b.none());
    }

    #[test]
    fn test_bitv_masking() {
        let b = Bitv::with_capacity(140, true);
        let mut bs = BitvSet::from_bitv(b);
        assert!(bs.contains(&139));
        assert!(!bs.contains(&140));
        assert!(bs.insert(150));
        assert!(!bs.contains(&140));
        assert!(!bs.contains(&149));
        assert!(bs.contains(&150));
        assert!(!bs.contains(&151));
    }

    #[test]
    fn test_bitv_set_basic() {
        // calculate nbits with u32::BITS granularity
        fn calc_nbits(bits: uint) -> uint {
            u32::BITS * ((bits + u32::BITS - 1) / u32::BITS)
        }

        let mut b = BitvSet::new();
        assert_eq!(b.capacity(), calc_nbits(0));
        assert!(b.insert(3));
        assert_eq!(b.capacity(), calc_nbits(3));
        assert!(!b.insert(3));
        assert!(b.contains(&3));
        assert!(b.insert(4));
        assert!(!b.insert(4));
        assert!(b.contains(&3));
        assert!(b.insert(400));
        assert_eq!(b.capacity(), calc_nbits(400));
        assert!(!b.insert(400));
        assert!(b.contains(&400));
        assert_eq!(b.len(), 3);
    }

    #[test]
    fn test_bitv_set_intersection() {
        let mut a = BitvSet::new();
        let mut b = BitvSet::new();

        assert!(a.insert(11));
        assert!(a.insert(1));
        assert!(a.insert(3));
        assert!(a.insert(77));
        assert!(a.insert(103));
        assert!(a.insert(5));

        assert!(b.insert(2));
        assert!(b.insert(11));
        assert!(b.insert(77));
        assert!(b.insert(5));
        assert!(b.insert(3));

        let expected = [3, 5, 11, 77];
        let actual = a.intersection(&b).collect::<Vec<uint>>();
        assert_eq!(actual.as_slice(), expected.as_slice());
    }

    #[test]
    fn test_bitv_set_difference() {
        let mut a = BitvSet::new();
        let mut b = BitvSet::new();

        assert!(a.insert(1));
        assert!(a.insert(3));
        assert!(a.insert(5));
        assert!(a.insert(200));
        assert!(a.insert(500));

        assert!(b.insert(3));
        assert!(b.insert(200));

        let expected = [1, 5, 500];
        let actual = a.difference(&b).collect::<Vec<uint>>();
        assert_eq!(actual.as_slice(), expected.as_slice());
    }

    #[test]
    fn test_bitv_set_symmetric_difference() {
        let mut a = BitvSet::new();
        let mut b = BitvSet::new();

        assert!(a.insert(1));
        assert!(a.insert(3));
        assert!(a.insert(5));
        assert!(a.insert(9));
        assert!(a.insert(11));

        assert!(b.insert(3));
        assert!(b.insert(9));
        assert!(b.insert(14));
        assert!(b.insert(220));

        let expected = [1, 5, 11, 14, 220];
        let actual = a.symmetric_difference(&b).collect::<Vec<uint>>();
        assert_eq!(actual.as_slice(), expected.as_slice());
    }

    #[test]
    fn test_bitv_set_union() {
        let mut a = BitvSet::new();
        let mut b = BitvSet::new();
        assert!(a.insert(1));
        assert!(a.insert(3));
        assert!(a.insert(5));
        assert!(a.insert(9));
        assert!(a.insert(11));
        assert!(a.insert(160));
        assert!(a.insert(19));
        assert!(a.insert(24));
        assert!(a.insert(200));

        assert!(b.insert(1));
        assert!(b.insert(5));
        assert!(b.insert(9));
        assert!(b.insert(13));
        assert!(b.insert(19));

        let expected = [1, 3, 5, 9, 11, 13, 19, 24, 160, 200];
        let actual = a.union(&b).collect::<Vec<uint>>();
        assert_eq!(actual.as_slice(), expected.as_slice());
    }

    #[test]
    fn test_bitv_set_subset() {
        let mut set1 = BitvSet::new();
        let mut set2 = BitvSet::new();

        assert!(set1.is_subset(&set2)); //  {}  {}
        set2.insert(100);
        assert!(set1.is_subset(&set2)); //  {}  { 1 }
        set2.insert(200);
        assert!(set1.is_subset(&set2)); //  {}  { 1, 2 }
        set1.insert(200);
        assert!(set1.is_subset(&set2)); //  { 2 }  { 1, 2 }
        set1.insert(300);
        assert!(!set1.is_subset(&set2)); // { 2, 3 }  { 1, 2 }
        set2.insert(300);
        assert!(set1.is_subset(&set2)); // { 2, 3 }  { 1, 2, 3 }
        set2.insert(400);
        assert!(set1.is_subset(&set2)); // { 2, 3 }  { 1, 2, 3, 4 }
        set2.remove(&100);
        assert!(set1.is_subset(&set2)); // { 2, 3 }  { 2, 3, 4 }
        set2.remove(&300);
        assert!(!set1.is_subset(&set2)); // { 2, 3 }  { 2, 4 }
        set1.remove(&300);
        assert!(set1.is_subset(&set2)); // { 2 }  { 2, 4 }
    }

    #[test]
    fn test_bitv_set_is_disjoint() {
        let a = BitvSet::from_bitv(from_bytes([0b10100010]));
        let b = BitvSet::from_bitv(from_bytes([0b01000000]));
        let c = BitvSet::new();
        let d = BitvSet::from_bitv(from_bytes([0b00110000]));

        assert!(!a.is_disjoint(&d));
        assert!(!d.is_disjoint(&a));

        assert!(a.is_disjoint(&b))
        assert!(a.is_disjoint(&c))
        assert!(b.is_disjoint(&a))
        assert!(b.is_disjoint(&c))
        assert!(c.is_disjoint(&a))
        assert!(c.is_disjoint(&b))
    }

    #[test]
    fn test_bitv_set_union_with() {
        //a should grow to include larger elements
        let mut a = BitvSet::new();
        a.insert(0);
        let mut b = BitvSet::new();
        b.insert(5);
        let expected = BitvSet::from_bitv(from_bytes([0b10000100]));
        a.union_with(&b);
        assert_eq!(a, expected);

        // Standard
        let mut a = BitvSet::from_bitv(from_bytes([0b10100010]));
        let mut b = BitvSet::from_bitv(from_bytes([0b01100010]));
        let c = a.clone();
        a.union_with(&b);
        b.union_with(&c);
        assert_eq!(a.len(), 4);
        assert_eq!(b.len(), 4);
    }

    #[test]
    fn test_bitv_set_intersect_with() {
        // Explicitly 0'ed bits
        let mut a = BitvSet::from_bitv(from_bytes([0b10100010]));
        let mut b = BitvSet::from_bitv(from_bytes([0b00000000]));
        let c = a.clone();
        a.intersect_with(&b);
        b.intersect_with(&c);
        assert!(a.is_empty());
        assert!(b.is_empty());

        // Uninitialized bits should behave like 0's
        let mut a = BitvSet::from_bitv(from_bytes([0b10100010]));
        let mut b = BitvSet::new();
        let c = a.clone();
        a.intersect_with(&b);
        b.intersect_with(&c);
        assert!(a.is_empty());
        assert!(b.is_empty());

        // Standard
        let mut a = BitvSet::from_bitv(from_bytes([0b10100010]));
        let mut b = BitvSet::from_bitv(from_bytes([0b01100010]));
        let c = a.clone();
        a.intersect_with(&b);
        b.intersect_with(&c);
        assert_eq!(a.len(), 2);
        assert_eq!(b.len(), 2);
    }

    #[test]
    fn test_bitv_set_difference_with() {
        // Explicitly 0'ed bits
        let mut a = BitvSet::from_bitv(from_bytes([0b00000000]));
        let b = BitvSet::from_bitv(from_bytes([0b10100010]));
        a.difference_with(&b);
        assert!(a.is_empty());

        // Uninitialized bits should behave like 0's
        let mut a = BitvSet::new();
        let b = BitvSet::from_bitv(from_bytes([0b11111111]));
        a.difference_with(&b);
        assert!(a.is_empty());

        // Standard
        let mut a = BitvSet::from_bitv(from_bytes([0b10100010]));
        let mut b = BitvSet::from_bitv(from_bytes([0b01100010]));
        let c = a.clone();
        a.difference_with(&b);
        b.difference_with(&c);
        assert_eq!(a.len(), 1);
        assert_eq!(b.len(), 1);
    }

    #[test]
    fn test_bitv_set_symmetric_difference_with() {
        //a should grow to include larger elements
        let mut a = BitvSet::new();
        a.insert(0);
        a.insert(1);
        let mut b = BitvSet::new();
        b.insert(1);
        b.insert(5);
        let expected = BitvSet::from_bitv(from_bytes([0b10000100]));
        a.symmetric_difference_with(&b);
        assert_eq!(a, expected);

        let mut a = BitvSet::from_bitv(from_bytes([0b10100010]));
        let b = BitvSet::new();
        let c = a.clone();
        a.symmetric_difference_with(&b);
        assert_eq!(a, c);

        // Standard
        let mut a = BitvSet::from_bitv(from_bytes([0b11100010]));
        let mut b = BitvSet::from_bitv(from_bytes([0b01101010]));
        let c = a.clone();
        a.symmetric_difference_with(&b);
        b.symmetric_difference_with(&c);
        assert_eq!(a.len(), 2);
        assert_eq!(b.len(), 2);
    }

    #[test]
    fn test_bitv_set_eq() {
        let a = BitvSet::from_bitv(from_bytes([0b10100010]));
        let b = BitvSet::from_bitv(from_bytes([0b00000000]));
        let c = BitvSet::new();

        assert!(a == a);
        assert!(a != b);
        assert!(a != c);
        assert!(b == b);
        assert!(b == c);
        assert!(c == c);
    }

    #[test]
    fn test_bitv_set_cmp() {
        let a = BitvSet::from_bitv(from_bytes([0b10100010]));
        let b = BitvSet::from_bitv(from_bytes([0b00000000]));
        let c = BitvSet::new();

        assert_eq!(a.cmp(&b), Greater);
        assert_eq!(a.cmp(&c), Greater);
        assert_eq!(b.cmp(&a), Less);
        assert_eq!(b.cmp(&c), Equal);
        assert_eq!(c.cmp(&a), Less);
        assert_eq!(c.cmp(&b), Equal);
    }

    #[test]
    fn test_bitv_remove() {
        let mut a = BitvSet::new();

        assert!(a.insert(1));
        assert!(a.remove(&1));

        assert!(a.insert(100));
        assert!(a.remove(&100));

        assert!(a.insert(1000));
        assert!(a.remove(&1000));
        a.shrink_to_fit();
        assert_eq!(a.capacity(), u32::BITS);
    }

    #[test]
    fn test_bitv_lt() {
        let mut a = Bitv::with_capacity(5u, false);
        let mut b = Bitv::with_capacity(5u, false);

        assert!(!(a < b) && !(b < a));
        b.set(2, true);
        assert!(a < b);
        a.set(3, true);
        assert!(a < b);
        a.set(2, true);
        assert!(!(a < b) && b < a);
        b.set(0, true);
        assert!(a < b);
    }

    #[test]
    fn test_ord() {
        let mut a = Bitv::with_capacity(5u, false);
        let mut b = Bitv::with_capacity(5u, false);

        assert!(a <= b && a >= b);
        a.set(1, true);
        assert!(a > b && a >= b);
        assert!(b < a && b <= a);
        b.set(1, true);
        b.set(2, true);
        assert!(b > a && b >= a);
        assert!(a < b && a <= b);
    }

    #[test]
    fn test_bitv_clone() {
        let mut a = BitvSet::new();

        assert!(a.insert(1));
        assert!(a.insert(100));
        assert!(a.insert(1000));

        let mut b = a.clone();

        assert!(a == b);

        assert!(b.remove(&1));
        assert!(a.contains(&1));

        assert!(a.remove(&1000));
        assert!(b.contains(&1000));
    }

    #[test]
    fn test_small_bitv_tests() {
        let v = from_bytes([0]);
        assert!(!v.all());
        assert!(!v.any());
        assert!(v.none());

        let v = from_bytes([0b00010100]);
        assert!(!v.all());
        assert!(v.any());
        assert!(!v.none());

        let v = from_bytes([0xFF]);
        assert!(v.all());
        assert!(v.any());
        assert!(!v.none());
    }

    #[test]
    fn test_big_bitv_tests() {
        let v = from_bytes([ // 88 bits
            0, 0, 0, 0,
            0, 0, 0, 0,
            0, 0, 0]);
        assert!(!v.all());
        assert!(!v.any());
        assert!(v.none());

        let v = from_bytes([ // 88 bits
            0, 0, 0b00010100, 0,
            0, 0, 0, 0b00110100,
            0, 0, 0]);
        assert!(!v.all());
        assert!(v.any());
        assert!(!v.none());

        let v = from_bytes([ // 88 bits
            0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF]);
        assert!(v.all());
        assert!(v.any());
        assert!(!v.none());
    }

    #[test]
    fn test_bitv_push_pop() {
        let mut s = Bitv::with_capacity(5 * u32::BITS - 2, false);
        assert_eq!(s.len(), 5 * u32::BITS - 2);
        assert_eq!(s.get(5 * u32::BITS - 3), false);
        s.push(true);
        s.push(true);
        assert_eq!(s.get(5 * u32::BITS - 2), true);
        assert_eq!(s.get(5 * u32::BITS - 1), true);
        // Here the internal vector will need to be extended
        s.push(false);
        assert_eq!(s.get(5 * u32::BITS), false);
        s.push(false);
        assert_eq!(s.get(5 * u32::BITS + 1), false);
        assert_eq!(s.len(), 5 * u32::BITS + 2);
        // Pop it all off
        assert_eq!(s.pop(), false);
        assert_eq!(s.pop(), false);
        assert_eq!(s.pop(), true);
        assert_eq!(s.pop(), true);
        assert_eq!(s.len(), 5 * u32::BITS - 2);
    }

    #[test]
    fn test_bitv_truncate() {
        let mut s = Bitv::with_capacity(5 * u32::BITS, true);

        assert_eq!(s, Bitv::with_capacity(5 * u32::BITS, true));
        assert_eq!(s.len(), 5 * u32::BITS);
        s.truncate(4 * u32::BITS);
        assert_eq!(s, Bitv::with_capacity(4 * u32::BITS, true));
        assert_eq!(s.len(), 4 * u32::BITS);
        // Truncating to a size > s.len() should be a noop
        s.truncate(5 * u32::BITS);
        assert_eq!(s, Bitv::with_capacity(4 * u32::BITS, true));
        assert_eq!(s.len(), 4 * u32::BITS);
        s.truncate(3 * u32::BITS - 10);
        assert_eq!(s, Bitv::with_capacity(3 * u32::BITS - 10, true));
        assert_eq!(s.len(), 3 * u32::BITS - 10);
        s.truncate(0);
        assert_eq!(s, Bitv::with_capacity(0, true));
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn test_bitv_reserve() {
        let mut s = Bitv::with_capacity(5 * u32::BITS, true);
        // Check capacity
        assert_eq!(s.capacity(), 5 * u32::BITS);
        s.reserve(2 * u32::BITS);
        assert_eq!(s.capacity(), 5 * u32::BITS);
        s.reserve(7 * u32::BITS);
        assert_eq!(s.capacity(), 7 * u32::BITS);
        s.reserve(7 * u32::BITS);
        assert_eq!(s.capacity(), 7 * u32::BITS);
        s.reserve(7 * u32::BITS + 1);
        assert_eq!(s.capacity(), 8 * u32::BITS);
        // Check that length hasn't changed
        assert_eq!(s.len(), 5 * u32::BITS);
        s.push(true);
        s.push(false);
        s.push(true);
        assert_eq!(s.get(5 * u32::BITS - 1), true);
        assert_eq!(s.get(5 * u32::BITS - 0), true);
        assert_eq!(s.get(5 * u32::BITS + 1), false);
        assert_eq!(s.get(5 * u32::BITS + 2), true);
    }

    #[test]
    fn test_bitv_grow() {
        let mut bitv = from_bytes([0b10110110, 0b00000000, 0b10101010]);
        bitv.grow(32, true);
        assert_eq!(bitv, from_bytes([0b10110110, 0b00000000, 0b10101010,
                                     0xFF, 0xFF, 0xFF, 0xFF]));
        bitv.grow(64, false);
        assert_eq!(bitv, from_bytes([0b10110110, 0b00000000, 0b10101010,
                                     0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0]));
        bitv.grow(16, true);
        assert_eq!(bitv, from_bytes([0b10110110, 0b00000000, 0b10101010,
                                     0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0, 0xFF, 0xFF]));
    }

    #[test]
    fn test_bitv_extend() {
        let mut bitv = from_bytes([0b10110110, 0b00000000, 0b11111111]);
        let ext = from_bytes([0b01001001, 0b10010010, 0b10111101]);
        bitv.extend(ext.iter());
        assert_eq!(bitv, from_bytes([0b10110110, 0b00000000, 0b11111111,
                                     0b01001001, 0b10010010, 0b10111101]));
    }

    #[test]
    fn test_bitv_set_show() {
        let mut s = BitvSet::new();
        s.insert(1);
        s.insert(10);
        s.insert(50);
        s.insert(2);
        assert_eq!("{1, 2, 10, 50}".to_string(), s.to_string());
    }

    fn rng() -> rand::IsaacRng {
        let seed: &[_] = &[1, 2, 3, 4, 5, 6, 7, 8, 9, 0];
        rand::SeedableRng::from_seed(seed)
    }

    #[bench]
    fn bench_uint_small(b: &mut Bencher) {
        let mut r = rng();
        let mut bitv = 0 as uint;
        b.iter(|| {
            for _ in range(0u, 100) {
                bitv |= 1 << ((r.next_u32() as uint) % u32::BITS);
            }
            &bitv
        })
    }

    #[bench]
    fn bench_bitv_set_big_fixed(b: &mut Bencher) {
        let mut r = rng();
        let mut bitv = Bitv::with_capacity(BENCH_BITS, false);
        b.iter(|| {
            for _ in range(0u, 100) {
                bitv.set((r.next_u32() as uint) % BENCH_BITS, true);
            }
            &bitv
        })
    }

    #[bench]
    fn bench_bitv_set_big_variable(b: &mut Bencher) {
        let mut r = rng();
        let mut bitv = Bitv::with_capacity(BENCH_BITS, false);
        b.iter(|| {
            for _ in range(0u, 100) {
                bitv.set((r.next_u32() as uint) % BENCH_BITS, r.gen());
            }
            &bitv
        })
    }

    #[bench]
    fn bench_bitv_set_small(b: &mut Bencher) {
        let mut r = rng();
        let mut bitv = Bitv::with_capacity(u32::BITS, false);
        b.iter(|| {
            for _ in range(0u, 100) {
                bitv.set((r.next_u32() as uint) % u32::BITS, true);
            }
            &bitv
        })
    }

    #[bench]
    fn bench_bitvset_small(b: &mut Bencher) {
        let mut r = rng();
        let mut bitv = BitvSet::new();
        b.iter(|| {
            for _ in range(0u, 100) {
                bitv.insert((r.next_u32() as uint) % u32::BITS);
            }
            &bitv
        })
    }

    #[bench]
    fn bench_bitvset_big(b: &mut Bencher) {
        let mut r = rng();
        let mut bitv = BitvSet::new();
        b.iter(|| {
            for _ in range(0u, 100) {
                bitv.insert((r.next_u32() as uint) % BENCH_BITS);
            }
            &bitv
        })
    }

    #[bench]
    fn bench_bitv_big_union(b: &mut Bencher) {
        let mut b1 = Bitv::with_capacity(BENCH_BITS, false);
        let b2 = Bitv::with_capacity(BENCH_BITS, false);
        b.iter(|| {
            b1.union(&b2)
        })
    }

    #[bench]
    fn bench_bitv_small_iter(b: &mut Bencher) {
        let bitv = Bitv::with_capacity(u32::BITS, false);
        b.iter(|| {
            let mut sum = 0u;
            for _ in range(0u, 10) {
                for pres in bitv.iter() {
                    sum += pres as uint;
                }
            }
            sum
        })
    }

    #[bench]
    fn bench_bitv_big_iter(b: &mut Bencher) {
        let bitv = Bitv::with_capacity(BENCH_BITS, false);
        b.iter(|| {
            let mut sum = 0u;
            for pres in bitv.iter() {
                sum += pres as uint;
            }
            sum
        })
    }

    #[bench]
    fn bench_bitvset_iter(b: &mut Bencher) {
        let bitv = BitvSet::from_bitv(from_fn(BENCH_BITS,
                                              |idx| {idx % 3 == 0}));
        b.iter(|| {
            let mut sum = 0u;
            for idx in bitv.iter() {
                sum += idx as uint;
            }
            sum
        })
    }
}
