// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// FIXME(#4375): this shouldn't have to be a nested module named 'generated'

#[macro_escape];

macro_rules! uint_module (($T:ty, $T_SIGNED:ty, $bits:expr) => (mod generated {

use num::BitCount;
use num::{ToStrRadix, FromStrRadix};
use num::{Zero, One, strconv};
use prelude::*;
use str;

pub use cmp::{min, max};

pub static bits : uint = $bits;
pub static bytes : uint = ($bits / 8);

pub static min_value: $T = 0 as $T;
pub static max_value: $T = 0 as $T - 1 as $T;

/// Calculates the sum of two numbers
#[inline]
pub fn add(x: $T, y: $T) -> $T { x + y }
/// Subtracts the second number from the first
#[inline]
pub fn sub(x: $T, y: $T) -> $T { x - y }
/// Multiplies two numbers together
#[inline]
pub fn mul(x: $T, y: $T) -> $T { x * y }
/// Divides the first argument by the second argument (using integer division)
#[inline]
pub fn div(x: $T, y: $T) -> $T { x / y }
/// Calculates the integer remainder when x is divided by y (equivalent to the
/// '%' operator)
#[inline]
pub fn rem(x: $T, y: $T) -> $T { x % y }

/// Returns true iff `x < y`
#[inline]
pub fn lt(x: $T, y: $T) -> bool { x < y }
/// Returns true iff `x <= y`
#[inline]
pub fn le(x: $T, y: $T) -> bool { x <= y }
/// Returns true iff `x == y`
#[inline]
pub fn eq(x: $T, y: $T) -> bool { x == y }
/// Returns true iff `x != y`
#[inline]
pub fn ne(x: $T, y: $T) -> bool { x != y }
/// Returns true iff `x >= y`
#[inline]
pub fn ge(x: $T, y: $T) -> bool { x >= y }
/// Returns true iff `x > y`
#[inline]
pub fn gt(x: $T, y: $T) -> bool { x > y }

#[inline]
/**
 * Iterate through a range with a given step value.
 *
 * # Examples
 * ~~~ {.rust}
 * let nums = [1,2,3,4,5,6,7];
 *
 * for uint::range_step(0, nums.len() - 1, 2) |i| {
 *     println(fmt!("%d & %d", nums[i], nums[i+1]));
 * }
 * ~~~
 */
pub fn range_step(start: $T, stop: $T, step: $T_SIGNED, it: &fn($T) -> bool) -> bool {
    let mut i = start;
    if step == 0 {
        fail!("range_step called with step == 0");
    }
    if step >= 0 {
        while i < stop {
            if !it(i) { return false; }
            // avoiding overflow. break if i + step > max_value
            if i > max_value - (step as $T) { return true; }
            i += step as $T;
        }
    } else {
        while i > stop {
            if !it(i) { return false; }
            // avoiding underflow. break if i + step < min_value
            if i < min_value + ((-step) as $T) { return true; }
            i -= -step as $T;
        }
    }
    return true;
}

#[inline]
/// Iterate over the range [`lo`..`hi`)
pub fn range(lo: $T, hi: $T, it: &fn($T) -> bool) -> bool {
    range_step(lo, hi, 1 as $T_SIGNED, it)
}

#[inline]
/// Iterate over the range [`hi`..`lo`)
pub fn range_rev(hi: $T, lo: $T, it: &fn($T) -> bool) -> bool {
    range_step(hi, lo, -1 as $T_SIGNED, it)
}

/// Computes the bitwise complement
#[inline]
pub fn compl(i: $T) -> $T {
    max_value ^ i
}

impl Num for $T {}

#[cfg(not(test))]
impl Ord for $T {
    #[inline]
    fn lt(&self, other: &$T) -> bool { (*self) < (*other) }
    #[inline]
    fn le(&self, other: &$T) -> bool { (*self) <= (*other) }
    #[inline]
    fn ge(&self, other: &$T) -> bool { (*self) >= (*other) }
    #[inline]
    fn gt(&self, other: &$T) -> bool { (*self) > (*other) }
}

#[cfg(not(test))]
impl Eq for $T {
    #[inline]
    fn eq(&self, other: &$T) -> bool { return (*self) == (*other); }
    #[inline]
    fn ne(&self, other: &$T) -> bool { return (*self) != (*other); }
}

impl Orderable for $T {
    #[inline]
    fn min(&self, other: &$T) -> $T {
        if *self < *other { *self } else { *other }
    }

    #[inline]
    fn max(&self, other: &$T) -> $T {
        if *self > *other { *self } else { *other }
    }

    /// Returns the number constrained within the range `mn <= self <= mx`.
    #[inline]
    fn clamp(&self, mn: &$T, mx: &$T) -> $T {
        cond!(
            (*self > *mx) { *mx   }
            (*self < *mn) { *mn   }
            _             { *self }
        )
    }
}

impl Zero for $T {
    #[inline]
    fn zero() -> $T { 0 }

    #[inline]
    fn is_zero(&self) -> bool { *self == 0 }
}

impl One for $T {
    #[inline]
    fn one() -> $T { 1 }
}

#[cfg(not(test))]
impl Add<$T,$T> for $T {
    #[inline]
    fn add(&self, other: &$T) -> $T { *self + *other }
}

#[cfg(not(test))]
impl Sub<$T,$T> for $T {
    #[inline]
    fn sub(&self, other: &$T) -> $T { *self - *other }
}

#[cfg(not(test))]
impl Mul<$T,$T> for $T {
    #[inline]
    fn mul(&self, other: &$T) -> $T { *self * *other }
}

#[cfg(not(test))]
impl Div<$T,$T> for $T {
    #[inline]
    fn div(&self, other: &$T) -> $T { *self / *other }
}

#[cfg(not(test))]
impl Rem<$T,$T> for $T {
    #[inline]
    fn rem(&self, other: &$T) -> $T { *self % *other }
}

#[cfg(not(test))]
impl Neg<$T> for $T {
    #[inline]
    fn neg(&self) -> $T { -*self }
}

impl Unsigned for $T {}

impl Integer for $T {
    /// Calculates `div` (`\`) and `rem` (`%`) simultaneously
    #[inline]
    fn div_rem(&self, other: &$T) -> ($T,$T) {
        (*self / *other, *self % *other)
    }

    /// Unsigned integer division. Returns the same result as `div` (`/`).
    #[inline]
    fn div_floor(&self, other: &$T) -> $T { *self / *other }

    /// Unsigned integer modulo operation. Returns the same result as `rem` (`%`).
    #[inline]
    fn mod_floor(&self, other: &$T) -> $T { *self / *other }

    /// Calculates `div_floor` and `modulo_floor` simultaneously
    #[inline]
    fn div_mod_floor(&self, other: &$T) -> ($T,$T) {
        (*self / *other, *self % *other)
    }

    /// Calculates the Greatest Common Divisor (GCD) of the number and `other`
    #[inline]
    fn gcd(&self, other: &$T) -> $T {
        // Use Euclid's algorithm
        let mut m = *self;
        let mut n = *other;
        while m != 0 {
            let temp = m;
            m = n % temp;
            n = temp;
        }
        n
    }

    /// Calculates the Lowest Common Multiple (LCM) of the number and `other`
    #[inline]
    fn lcm(&self, other: &$T) -> $T {
        (*self * *other) / self.gcd(other)
    }

    /// Returns `true` if the number can be divided by `other` without leaving a remainder
    #[inline]
    fn is_multiple_of(&self, other: &$T) -> bool { *self % *other == 0 }

    /// Returns `true` if the number is divisible by `2`
    #[inline]
    fn is_even(&self) -> bool { self.is_multiple_of(&2) }

    /// Returns `true` if the number is not divisible by `2`
    #[inline]
    fn is_odd(&self) -> bool { !self.is_even() }
}

impl Bitwise for $T {}

#[cfg(not(test))]
impl BitOr<$T,$T> for $T {
    #[inline]
    fn bitor(&self, other: &$T) -> $T { *self | *other }
}

#[cfg(not(test))]
impl BitAnd<$T,$T> for $T {
    #[inline]
    fn bitand(&self, other: &$T) -> $T { *self & *other }
}

#[cfg(not(test))]
impl BitXor<$T,$T> for $T {
    #[inline]
    fn bitxor(&self, other: &$T) -> $T { *self ^ *other }
}

#[cfg(not(test))]
impl Shl<$T,$T> for $T {
    #[inline]
    fn shl(&self, other: &$T) -> $T { *self << *other }
}

#[cfg(not(test))]
impl Shr<$T,$T> for $T {
    #[inline]
    fn shr(&self, other: &$T) -> $T { *self >> *other }
}

#[cfg(not(test))]
impl Not<$T> for $T {
    #[inline]
    fn not(&self) -> $T { !*self }
}

impl Bounded for $T {
    #[inline]
    fn min_value() -> $T { min_value }

    #[inline]
    fn max_value() -> $T { max_value }
}

impl Int for $T {}

// String conversion functions and impl str -> num

/// Parse a string as a number in base 10.
#[inline]
pub fn from_str(s: &str) -> Option<$T> {
    strconv::from_str_common(s, 10u, false, false, false,
                             strconv::ExpNone, false, false)
}

/// Parse a string as a number in the given base.
#[inline]
pub fn from_str_radix(s: &str, radix: uint) -> Option<$T> {
    strconv::from_str_common(s, radix, false, false, false,
                             strconv::ExpNone, false, false)
}

/// Parse a byte slice as a number in the given base.
#[inline]
pub fn parse_bytes(buf: &[u8], radix: uint) -> Option<$T> {
    strconv::from_str_bytes_common(buf, radix, false, false, false,
                                   strconv::ExpNone, false, false)
}

impl FromStr for $T {
    #[inline]
    fn from_str(s: &str) -> Option<$T> {
        from_str(s)
    }
}

impl FromStrRadix for $T {
    #[inline]
    fn from_str_radix(s: &str, radix: uint) -> Option<$T> {
        from_str_radix(s, radix)
    }
}

// String conversion functions and impl num -> str

/// Convert to a string as a byte slice in a given base.
#[inline]
pub fn to_str_bytes<U>(n: $T, radix: uint, f: &fn(v: &[u8]) -> U) -> U {
    // The radix can be as low as 2, so we need at least 64 characters for a
    // base 2 number.
    let mut buf = [0u8, ..64];
    let mut cur = 0;
    do strconv::int_to_str_bytes_common(n, radix, strconv::SignNone) |i| {
        buf[cur] = i;
        cur += 1;
    }
    f(buf.slice(0, cur))
}

/// Convert to a string in base 10.
#[inline]
pub fn to_str(num: $T) -> ~str {
    to_str_radix(num, 10u)
}

/// Convert to a string in a given base.
#[inline]
pub fn to_str_radix(num: $T, radix: uint) -> ~str {
    let mut buf = ~[];
    do strconv::int_to_str_bytes_common(num, radix, strconv::SignNone) |i| {
        buf.push(i);
    }
    // We know we generated valid utf-8, so we don't need to go through that
    // check.
    unsafe { str::raw::from_bytes_owned(buf) }
}

impl ToStr for $T {
    #[inline]
    fn to_str(&self) -> ~str {
        to_str(*self)
    }
}

impl ToStrRadix for $T {
    #[inline]
    fn to_str_radix(&self, radix: uint) -> ~str {
        to_str_radix(*self, radix)
    }
}

impl Primitive for $T {
    #[inline]
    fn bits() -> uint { bits }

    #[inline]
    fn bytes() -> uint { bits / 8 }
}

impl BitCount for $T {
    /// Counts the number of bits set. Wraps LLVM's `ctpop` intrinsic.
    #[inline]
    fn population_count(&self) -> $T {
        (*self as $T_SIGNED).population_count() as $T
    }

    /// Counts the number of leading zeros. Wraps LLVM's `ctlz` intrinsic.
    #[inline]
    fn leading_zeros(&self) -> $T {
        (*self as $T_SIGNED).leading_zeros() as $T
    }

    /// Counts the number of trailing zeros. Wraps LLVM's `cttz` intrinsic.
    #[inline]
    fn trailing_zeros(&self) -> $T {
        (*self as $T_SIGNED).trailing_zeros() as $T
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prelude::*;

    use num;
    use sys;
    use u16;
    use u32;
    use u64;
    use u8;
    use uint;

    #[test]
    fn test_num() {
        num::test_num(10 as $T, 2 as $T);
    }

    #[test]
    fn test_orderable() {
        assert_eq!((1 as $T).min(&(2 as $T)), 1 as $T);
        assert_eq!((2 as $T).min(&(1 as $T)), 1 as $T);
        assert_eq!((1 as $T).max(&(2 as $T)), 2 as $T);
        assert_eq!((2 as $T).max(&(1 as $T)), 2 as $T);
        assert_eq!((1 as $T).clamp(&(2 as $T), &(4 as $T)), 2 as $T);
        assert_eq!((8 as $T).clamp(&(2 as $T), &(4 as $T)), 4 as $T);
        assert_eq!((3 as $T).clamp(&(2 as $T), &(4 as $T)), 3 as $T);
    }

    #[test]
    fn test_gcd() {
        assert_eq!((10 as $T).gcd(&2), 2 as $T);
        assert_eq!((10 as $T).gcd(&3), 1 as $T);
        assert_eq!((0 as $T).gcd(&3), 3 as $T);
        assert_eq!((3 as $T).gcd(&3), 3 as $T);
        assert_eq!((56 as $T).gcd(&42), 14 as $T);
    }

    #[test]
    fn test_lcm() {
        assert_eq!((1 as $T).lcm(&0), 0 as $T);
        assert_eq!((0 as $T).lcm(&1), 0 as $T);
        assert_eq!((1 as $T).lcm(&1), 1 as $T);
        assert_eq!((8 as $T).lcm(&9), 72 as $T);
        assert_eq!((11 as $T).lcm(&5), 55 as $T);
        assert_eq!((99 as $T).lcm(&17), 1683 as $T);
    }

    #[test]
    fn test_multiple_of() {
        assert!((6 as $T).is_multiple_of(&(6 as $T)));
        assert!((6 as $T).is_multiple_of(&(3 as $T)));
        assert!((6 as $T).is_multiple_of(&(1 as $T)));
    }

    #[test]
    fn test_even() {
        assert_eq!((0 as $T).is_even(), true);
        assert_eq!((1 as $T).is_even(), false);
        assert_eq!((2 as $T).is_even(), true);
        assert_eq!((3 as $T).is_even(), false);
        assert_eq!((4 as $T).is_even(), true);
    }

    #[test]
    fn test_odd() {
        assert_eq!((0 as $T).is_odd(), false);
        assert_eq!((1 as $T).is_odd(), true);
        assert_eq!((2 as $T).is_odd(), false);
        assert_eq!((3 as $T).is_odd(), true);
        assert_eq!((4 as $T).is_odd(), false);
    }

    #[test]
    fn test_bitwise() {
        assert_eq!(0b1110 as $T, (0b1100 as $T).bitor(&(0b1010 as $T)));
        assert_eq!(0b1000 as $T, (0b1100 as $T).bitand(&(0b1010 as $T)));
        assert_eq!(0b0110 as $T, (0b1100 as $T).bitxor(&(0b1010 as $T)));
        assert_eq!(0b1110 as $T, (0b0111 as $T).shl(&(1 as $T)));
        assert_eq!(0b0111 as $T, (0b1110 as $T).shr(&(1 as $T)));
        assert_eq!(max_value - (0b1011 as $T), (0b1011 as $T).not());
    }

    #[test]
    fn test_bitcount() {
        assert_eq!((0b010101 as $T).population_count(), 3);
    }

    #[test]
    fn test_primitive() {
        assert_eq!(Primitive::bits::<$T>(), sys::size_of::<$T>() * 8);
        assert_eq!(Primitive::bytes::<$T>(), sys::size_of::<$T>());
    }

    #[test]
    pub fn test_to_str() {
        assert_eq!(to_str_radix(0 as $T, 10u), ~"0");
        assert_eq!(to_str_radix(1 as $T, 10u), ~"1");
        assert_eq!(to_str_radix(2 as $T, 10u), ~"2");
        assert_eq!(to_str_radix(11 as $T, 10u), ~"11");
        assert_eq!(to_str_radix(11 as $T, 16u), ~"b");
        assert_eq!(to_str_radix(255 as $T, 16u), ~"ff");
        assert_eq!(to_str_radix(0xff as $T, 10u), ~"255");
    }

    #[test]
    pub fn test_from_str() {
        assert_eq!(from_str("0"), Some(0u as $T));
        assert_eq!(from_str("3"), Some(3u as $T));
        assert_eq!(from_str("10"), Some(10u as $T));
        assert_eq!(u32::from_str("123456789"), Some(123456789 as u32));
        assert_eq!(from_str("00100"), Some(100u as $T));

        assert!(from_str("").is_none());
        assert!(from_str(" ").is_none());
        assert!(from_str("x").is_none());
    }

    #[test]
    pub fn test_parse_bytes() {
        use str::StrSlice;
        assert_eq!(parse_bytes("123".as_bytes(), 10u), Some(123u as $T));
        assert_eq!(parse_bytes("1001".as_bytes(), 2u), Some(9u as $T));
        assert_eq!(parse_bytes("123".as_bytes(), 8u), Some(83u as $T));
        assert_eq!(u16::parse_bytes("123".as_bytes(), 16u), Some(291u as u16));
        assert_eq!(u16::parse_bytes("ffff".as_bytes(), 16u), Some(65535u as u16));
        assert_eq!(parse_bytes("z".as_bytes(), 36u), Some(35u as $T));

        assert!(parse_bytes("Z".as_bytes(), 10u).is_none());
        assert!(parse_bytes("_".as_bytes(), 2u).is_none());
    }

    #[test]
    fn test_uint_to_str_overflow() {
        let mut u8_val: u8 = 255_u8;
        assert_eq!(u8::to_str(u8_val), ~"255");

        u8_val += 1 as u8;
        assert_eq!(u8::to_str(u8_val), ~"0");

        let mut u16_val: u16 = 65_535_u16;
        assert_eq!(u16::to_str(u16_val), ~"65535");

        u16_val += 1 as u16;
        assert_eq!(u16::to_str(u16_val), ~"0");

        let mut u32_val: u32 = 4_294_967_295_u32;
        assert_eq!(u32::to_str(u32_val), ~"4294967295");

        u32_val += 1 as u32;
        assert_eq!(u32::to_str(u32_val), ~"0");

        let mut u64_val: u64 = 18_446_744_073_709_551_615_u64;
        assert_eq!(u64::to_str(u64_val), ~"18446744073709551615");

        u64_val += 1 as u64;
        assert_eq!(u64::to_str(u64_val), ~"0");
    }

    #[test]
    fn test_uint_from_str_overflow() {
        let mut u8_val: u8 = 255_u8;
        assert_eq!(u8::from_str("255"), Some(u8_val));
        assert!(u8::from_str("256").is_none());

        u8_val += 1 as u8;
        assert_eq!(u8::from_str("0"), Some(u8_val));
        assert!(u8::from_str("-1").is_none());

        let mut u16_val: u16 = 65_535_u16;
        assert_eq!(u16::from_str("65535"), Some(u16_val));
        assert!(u16::from_str("65536").is_none());

        u16_val += 1 as u16;
        assert_eq!(u16::from_str("0"), Some(u16_val));
        assert!(u16::from_str("-1").is_none());

        let mut u32_val: u32 = 4_294_967_295_u32;
        assert_eq!(u32::from_str("4294967295"), Some(u32_val));
        assert!(u32::from_str("4294967296").is_none());

        u32_val += 1 as u32;
        assert_eq!(u32::from_str("0"), Some(u32_val));
        assert!(u32::from_str("-1").is_none());

        let mut u64_val: u64 = 18_446_744_073_709_551_615_u64;
        assert_eq!(u64::from_str("18446744073709551615"), Some(u64_val));
        assert!(u64::from_str("18446744073709551616").is_none());

        u64_val += 1 as u64;
        assert_eq!(u64::from_str("0"), Some(u64_val));
        assert!(u64::from_str("-1").is_none());
    }

    #[test]
    #[should_fail]
    #[ignore(cfg(windows))]
    pub fn to_str_radix1() {
        uint::to_str_radix(100u, 1u);
    }

    #[test]
    #[should_fail]
    #[ignore(cfg(windows))]
    pub fn to_str_radix37() {
        uint::to_str_radix(100u, 37u);
    }

    #[test]
    pub fn test_ranges() {
        let mut l = ~[];

        for range(0,3) |i| {
            l.push(i);
        }
        for range_rev(13,10) |i| {
            l.push(i);
        }
        for range_step(20,26,2) |i| {
            l.push(i);
        }
        for range_step(36,30,-2) |i| {
            l.push(i);
        }
        for range_step(max_value - 2, max_value, 2) |i| {
            l.push(i);
        }
        for range_step(max_value - 3, max_value, 2) |i| {
            l.push(i);
        }
        for range_step(min_value + 2, min_value, -2) |i| {
            l.push(i);
        }
        for range_step(min_value + 3, min_value, -2) |i| {
            l.push(i);
        }

        assert_eq!(l, ~[0,1,2,
                        13,12,11,
                        20,22,24,
                        36,34,32,
                        max_value-2,
                        max_value-3,max_value-1,
                        min_value+2,
                        min_value+3,min_value+1]);

        // None of the `fail`s should execute.
        for range(0,0) |_i| {
            fail!("unreachable");
        }
        for range_rev(0,0) |_i| {
            fail!("unreachable");
        }
        for range_step(10,0,1) |_i| {
            fail!("unreachable");
        }
        for range_step(0,1,-10) |_i| {
            fail!("unreachable");
        }
    }

    #[test]
    #[should_fail]
    #[ignore(cfg(windows))]
    fn test_range_step_zero_step_up() {
        for range_step(0,10,0) |_i| {}
    }
    #[test]
    #[should_fail]
    #[ignore(cfg(windows))]
    fn test_range_step_zero_step_down() {
        for range_step(0,-10,0) |_i| {}
    }
}

}))
