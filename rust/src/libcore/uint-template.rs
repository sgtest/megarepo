// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// NB: transitionary, de-mode-ing.
#[forbid(deprecated_mode)];
#[forbid(deprecated_pattern)];

use T = self::inst::T;

use char;
use cmp::{Eq, Ord};
use from_str::FromStr;
use iter;
use num;
use option::{None, Option, Some};
use str;
use uint;
use vec;

pub const bits : uint = inst::bits;
pub const bytes : uint = (inst::bits / 8);

pub const min_value: T = 0 as T;
pub const max_value: T = 0 as T - 1 as T;

#[inline(always)]
pub pure fn min(x: T, y: T) -> T { if x < y { x } else { y } }
#[inline(always)]
pub pure fn max(x: T, y: T) -> T { if x > y { x } else { y } }

#[inline(always)]
pub pure fn add(x: T, y: T) -> T { x + y }
#[inline(always)]
pub pure fn sub(x: T, y: T) -> T { x - y }
#[inline(always)]
pub pure fn mul(x: T, y: T) -> T { x * y }
#[inline(always)]
pub pure fn div(x: T, y: T) -> T { x / y }
#[inline(always)]
pub pure fn rem(x: T, y: T) -> T { x % y }

#[inline(always)]
pub pure fn lt(x: T, y: T) -> bool { x < y }
#[inline(always)]
pub pure fn le(x: T, y: T) -> bool { x <= y }
#[inline(always)]
pub pure fn eq(x: T, y: T) -> bool { x == y }
#[inline(always)]
pub pure fn ne(x: T, y: T) -> bool { x != y }
#[inline(always)]
pub pure fn ge(x: T, y: T) -> bool { x >= y }
#[inline(always)]
pub pure fn gt(x: T, y: T) -> bool { x > y }

#[inline(always)]
pub pure fn is_positive(x: T) -> bool { x > 0 as T }
#[inline(always)]
pub pure fn is_negative(x: T) -> bool { x < 0 as T }
#[inline(always)]
pub pure fn is_nonpositive(x: T) -> bool { x <= 0 as T }
#[inline(always)]
pub pure fn is_nonnegative(x: T) -> bool { x >= 0 as T }

#[inline(always)]
/**
 * Iterate over the range [`start`,`start`+`step`..`stop`)
 *
 * Note that `uint` requires separate `range_step` functions for each
 * direction.
 *
 */
pub pure fn range_step_up(start: T, stop: T, step: T, it: fn(T) -> bool) {
    let mut i = start;
    if step == 0 {
        fail ~"range_step_up called with step == 0";
    }
    while i < stop {
        if !it(i) { break }
        i += step;
    }
}

#[inline(always)]
/**
 * Iterate over the range [`start`,`start`-`step`..`stop`)
 *
 * Note that `uint` requires separate `range_step` functions for each
 * direction.
 *
 */
pub pure fn range_step_down(start: T, stop: T, step: T, it: fn(T) -> bool) {
    let mut i = start;
    if step == 0 {
        fail ~"range_step_down called with step == 0";
    }
    while i > stop {
        if !it(i) { break }
        i -= step;
    }
}

#[inline(always)]
/// Iterate over the range [`lo`..`hi`)
pub pure fn range(lo: T, hi: T, it: fn(T) -> bool) {
    range_step_up(lo, hi, 1 as T, it);
}

#[inline(always)]
/// Iterate over the range [`hi`..`lo`)
pub pure fn range_rev(hi: T, lo: T, it: fn(T) -> bool) {
    range_step_down(hi, lo, 1 as T, it);
}

/// Computes the bitwise complement
#[inline(always)]
pub pure fn compl(i: T) -> T {
    max_value ^ i
}

#[cfg(notest)]
impl T : Ord {
    #[inline(always)]
    pure fn lt(&self, other: &T) -> bool { (*self) < (*other) }
    #[inline(always)]
    pure fn le(&self, other: &T) -> bool { (*self) <= (*other) }
    #[inline(always)]
    pure fn ge(&self, other: &T) -> bool { (*self) >= (*other) }
    #[inline(always)]
    pure fn gt(&self, other: &T) -> bool { (*self) > (*other) }
}

#[cfg(notest)]
impl T : Eq {
    #[inline(always)]
    pure fn eq(&self, other: &T) -> bool { return (*self) == (*other); }
    #[inline(always)]
    pure fn ne(&self, other: &T) -> bool { return (*self) != (*other); }
}

impl T: num::Num {
    #[inline(always)]
    pure fn add(&self, other: &T)    -> T { return *self + *other; }
    #[inline(always)]
    pure fn sub(&self, other: &T)    -> T { return *self - *other; }
    #[inline(always)]
    pure fn mul(&self, other: &T)    -> T { return *self * *other; }
    #[inline(always)]
    pure fn div(&self, other: &T)    -> T { return *self / *other; }
    #[inline(always)]
    pure fn modulo(&self, other: &T) -> T { return *self % *other; }
    #[inline(always)]
    pure fn neg(&self)              -> T { return -*self;        }

    #[inline(always)]
    pure fn to_int(&self)         -> int { return *self as int; }
    #[inline(always)]
    static pure fn from_int(n: int) -> T   { return n as T;      }
}

impl T: num::Zero {
    #[inline(always)]
    static pure fn zero() -> T { 0 }
}

impl T: num::One {
    #[inline(always)]
    static pure fn one() -> T { 1 }
}

impl T: iter::Times {
    #[inline(always)]
    #[doc = "A convenience form for basic iteration. Given a variable `x` \
        of any numeric type, the expression `for x.times { /* anything */ }` \
        will execute the given function exactly x times. If we assume that \
        `x` is an int, this is functionally equivalent to \
        `for int::range(0, x) |_i| { /* anything */ }`."]
    pure fn times(&self, it: fn() -> bool) {
        let mut i = *self;
        while i > 0 {
            if !it() { break }
            i -= 1;
        }
    }
}

/**
 * Parse a buffer of bytes
 *
 * # Arguments
 *
 * * buf - A byte buffer
 * * radix - The base of the number
 *
 * # Failure
 *
 * `buf` must not be empty
 */
pub pure fn parse_bytes(buf: &[const u8], radix: uint) -> Option<T> {
    if vec::len(buf) == 0u { return None; }
    let mut i = vec::len(buf) - 1u;
    let mut power = 1u as T;
    let mut n = 0u as T;
    loop {
        match char::to_digit(buf[i] as char, radix) {
          Some(d) => n += d as T * power,
          None => return None
        }
        power *= radix as T;
        if i == 0u { return Some(n); }
        i -= 1u;
    };
}

/// Parse a string to an int
#[inline(always)]
pub pure fn from_str(s: &str) -> Option<T>
{
    parse_bytes(str::to_bytes(s), 10u)
}

impl T : FromStr {
    #[inline(always)]
    static pure fn from_str(s: &str) -> Option<T> { from_str(s) }
}

/// Parse a string as an unsigned integer.
pub fn from_str_radix(buf: &str, radix: u64) -> Option<u64> {
    if str::len(buf) == 0u { return None; }
    let mut i = str::len(buf) - 1u;
    let mut power = 1u64, n = 0u64;
    loop {
        match char::to_digit(buf[i] as char, radix as uint) {
          Some(d) => n += d as u64 * power,
          None => return None
        }
        power *= radix;
        if i == 0u { return Some(n); }
        i -= 1u;
    };
}

/**
 * Convert to a string in a given base
 *
 * # Failure
 *
 * Fails if `radix` < 2 or `radix` > 16
 */
#[inline(always)]
pub pure fn to_str(num: T, radix: uint) -> ~str {
    do to_str_bytes(false, num, radix) |slice| {
        do vec::as_imm_buf(slice) |p, len| {
            unsafe { str::raw::from_buf_len(p, len) }
        }
    }
}

/// Low-level helper routine for string conversion.
pub pure fn to_str_bytes<U>(neg: bool, num: T, radix: uint,
                   f: fn(v: &[u8]) -> U) -> U {

    #[inline(always)]
    pure fn digit(n: T) -> u8 {
        if n <= 9u as T {
            n as u8 + '0' as u8
        } else if n <= 15u as T {
            (n - 10 as T) as u8 + 'a' as u8
        } else {
            fail;
        }
    }

    assert (1u < radix && radix <= 16u);

    // Enough room to hold any number in any radix.
    // Worst case: 64-bit number, binary-radix, with
    // a leading negative sign = 65 bytes.
    let buf : [mut u8 * 65] = [mut 0u8, ..65];
    let len = buf.len();

    let mut i = len;
    let mut n = num;
    let radix = radix as T;
    loop {
        i -= 1u;
        assert 0u < i && i < len;
        buf[i] = digit(n % radix);
        n /= radix;
        if n == 0 as T { break; }
    }

    assert 0u < i && i < len;

    if neg {
        i -= 1u;
        buf[i] = '-' as u8;
    }

    f(vec::view(buf, i, len))
}

/// Convert to a string
#[inline(always)]
pub pure fn str(i: T) -> ~str { return to_str(i, 10u); }

#[test]
pub fn test_to_str() {
    assert to_str(0 as T, 10u) == ~"0";
    assert to_str(1 as T, 10u) == ~"1";
    assert to_str(2 as T, 10u) == ~"2";
    assert to_str(11 as T, 10u) == ~"11";
    assert to_str(11 as T, 16u) == ~"b";
    assert to_str(255 as T, 16u) == ~"ff";
    assert to_str(0xff as T, 10u) == ~"255";
}

#[test]
pub fn test_from_str() {
    assert from_str(~"0") == Some(0u as T);
    assert from_str(~"3") == Some(3u as T);
    assert from_str(~"10") == Some(10u as T);
    assert from_str(~"123456789") == Some(123456789u as T);
    assert from_str(~"00100") == Some(100u as T);

    assert from_str(~"").is_none();
    assert from_str(~" ").is_none();
    assert from_str(~"x").is_none();
}

#[test]
pub fn test_parse_bytes() {
    use str::to_bytes;
    assert parse_bytes(to_bytes(~"123"), 10u) == Some(123u as T);
    assert parse_bytes(to_bytes(~"1001"), 2u) == Some(9u as T);
    assert parse_bytes(to_bytes(~"123"), 8u) == Some(83u as T);
    assert parse_bytes(to_bytes(~"123"), 16u) == Some(291u as T);
    assert parse_bytes(to_bytes(~"ffff"), 16u) == Some(65535u as T);
    assert parse_bytes(to_bytes(~"z"), 36u) == Some(35u as T);

    assert parse_bytes(to_bytes(~"Z"), 10u).is_none();
    assert parse_bytes(to_bytes(~"_"), 2u).is_none();
}

#[test]
#[should_fail]
#[ignore(cfg(windows))]
pub fn to_str_radix1() {
    uint::to_str(100u, 1u);
}

#[test]
#[should_fail]
#[ignore(cfg(windows))]
pub fn to_str_radix17() {
    uint::to_str(100u, 17u);
}

#[test]
pub fn test_times() {
    use iter::Times;
    let ten = 10 as T;
    let mut accum = 0;
    for ten.times { accum += 1; }
    assert (accum == 10);
}
use io;
#[test]
pub fn test_ranges() {
    let mut l = ~[];

    for range(0,3) |i| {
        l.push(i);
    }
    for range_rev(13,10) |i| {
        l.push(i);
    }
    for range_step_up(20,26,2) |i| {
        l.push(i);
    }
    for range_step_down(36,30,2) |i| {
        l.push(i);
    }

    assert l == ~[0,1,2,
                  13,12,11,
                  20,22,24,
                  36,34,32];

    // None of the `fail`s should execute.
    for range(0,0) |_i| {
        fail ~"unreachable";
    }
    for range_rev(0,0) |_i| {
        fail ~"unreachable";
    }
    for range_step_up(10,0,1) |_i| {
        fail ~"unreachable";
    }
    for range_step_down(0,10,1) |_i| {
        fail ~"unreachable";
    }
}

#[test]
#[should_fail]
fn test_range_step_up_zero_step() {
    for range_step_up(0,10,0) |_i| {}
}
#[test]
#[should_fail]
fn test_range_step_down_zero_step() {
    for range_step_down(0,10,0) |_i| {}
}
