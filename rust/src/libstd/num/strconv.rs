// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[allow(missing_doc)];

use clone::Clone;
use container::Container;
use std::cmp::{Ord, Eq};
use ops::{Add, Sub, Mul, Div, Rem, Neg};
use option::{None, Option, Some};
use char;
use str::{StrSlice};
use str;
use vec::{CopyableVector, ImmutableVector, MutableVector};
use vec::OwnedVector;
use num::{NumCast, Zero, One, cast, pow_with_uint, Integer};
use num::{Round, Float, FPNaN, FPInfinite};

pub enum ExponentFormat {
    ExpNone,
    ExpDec,
    ExpBin
}

pub enum SignificantDigits {
    DigAll,
    DigMax(uint),
    DigExact(uint)
}

pub enum SignFormat {
    SignNone,
    SignNeg,
    SignAll
}

pub trait NumStrConv {
    fn nan()      -> Option<Self>;
    fn inf()      -> Option<Self>;
    fn neg_inf()  -> Option<Self>;
    fn neg_zero() -> Option<Self>;

    fn round_to_zero(&self)   -> Self;
    fn fractional_part(&self) -> Self;
}

macro_rules! impl_NumStrConv_Floating (($t:ty) => (
    impl NumStrConv for $t {
        #[inline]
        fn nan()      -> Option<$t> { Some( 0.0 / 0.0) }
        #[inline]
        fn inf()      -> Option<$t> { Some( 1.0 / 0.0) }
        #[inline]
        fn neg_inf()  -> Option<$t> { Some(-1.0 / 0.0) }
        #[inline]
        fn neg_zero() -> Option<$t> { Some(-0.0      ) }

        #[inline]
        fn round_to_zero(&self) -> $t { self.trunc() }
        #[inline]
        fn fractional_part(&self) -> $t { self.fract() }
    }
))

macro_rules! impl_NumStrConv_Integer (($t:ty) => (
    impl NumStrConv for $t {
        #[inline] fn nan()      -> Option<$t> { None }
        #[inline] fn inf()      -> Option<$t> { None }
        #[inline] fn neg_inf()  -> Option<$t> { None }
        #[inline] fn neg_zero() -> Option<$t> { None }

        #[inline] fn round_to_zero(&self)   -> $t { *self }
        #[inline] fn fractional_part(&self) -> $t {     0 }
    }
))

// FIXME: #4955
// Replace by two generic impls for traits 'Integral' and 'Floating'
impl_NumStrConv_Floating!(f32)
impl_NumStrConv_Floating!(f64)

impl_NumStrConv_Integer!(int)
impl_NumStrConv_Integer!(i8)
impl_NumStrConv_Integer!(i16)
impl_NumStrConv_Integer!(i32)
impl_NumStrConv_Integer!(i64)

impl_NumStrConv_Integer!(uint)
impl_NumStrConv_Integer!(u8)
impl_NumStrConv_Integer!(u16)
impl_NumStrConv_Integer!(u32)
impl_NumStrConv_Integer!(u64)


// Special value strings as [u8] consts.
static INF_BUF:          [u8, ..3] = ['i' as u8, 'n' as u8, 'f' as u8];
static POS_INF_BUF: [u8, ..4] = ['+' as u8, 'i' as u8, 'n' as u8,
                                      'f' as u8];
static NEG_INF_BUF: [u8, ..4] = ['-' as u8, 'i' as u8, 'n' as u8,
                                      'f' as u8];
static NAN_BUF:          [u8, ..3] = ['N' as u8, 'a' as u8, 'N' as u8];

/**
 * Converts an integral number to its string representation as a byte vector.
 * This is meant to be a common base implementation for all integral string
 * conversion functions like `to_str()` or `to_str_radix()`.
 *
 * # Arguments
 * - `num`           - The number to convert. Accepts any number that
 *                     implements the numeric traits.
 * - `radix`         - Base to use. Accepts only the values 2-36.
 * - `sign`          - How to emit the sign. Options are:
 *     - `SignNone`: No sign at all. Basically emits `abs(num)`.
 *     - `SignNeg`:  Only `-` on negative values.
 *     - `SignAll`:  Both `+` on positive, and `-` on negative numbers.
 * - `f`             - a callback which will be invoked for each ascii character
 *                     which composes the string representation of this integer
 *
 * # Return value
 * A tuple containing the byte vector, and a boolean flag indicating
 * whether it represents a special value like `inf`, `-inf`, `NaN` or not.
 * It returns a tuple because there can be ambiguity between a special value
 * and a number representation at higher bases.
 *
 * # Failure
 * - Fails if `radix` < 2 or `radix` > 36.
 */
pub fn int_to_str_bytes_common<T:NumCast+Zero+Eq+Ord+Integer+
                                 Div<T,T>+Neg<T>+Rem<T,T>+Mul<T,T>>(
        num: T, radix: uint, sign: SignFormat, f: &fn(u8)) {
    assert!(2 <= radix && radix <= 36);

    let _0: T = Zero::zero();

    let neg = num < _0;
    let radix_gen: T = cast(radix);

    let mut deccum = num;
    // This is just for integral types, the largest of which is a u64. The
    // smallest base that we can have is 2, so the most number of digits we're
    // ever going to have is 64
    let mut buf = [0u8, ..64];
    let mut cur = 0;

    // Loop at least once to make sure at least a `0` gets emitted.
    loop {
        // Calculate the absolute value of each digit instead of only
        // doing it once for the whole number because a
        // representable negative number doesn't necessary have an
        // representable additive inverse of the same type
        // (See twos complement). But we assume that for the
        // numbers [-35 .. 0] we always have [0 .. 35].
        let current_digit_signed = deccum % radix_gen;
        let current_digit = if current_digit_signed < _0 {
            -current_digit_signed
        } else {
            current_digit_signed
        };
        buf[cur] = match current_digit.to_u8() {
            i @ 0..9 => '0' as u8 + i,
            i        => 'a' as u8 + (i - 10),
        };
        cur += 1;

        deccum = deccum / radix_gen;
        // No more digits to calculate for the non-fractional part -> break
        if deccum == _0 { break; }
    }

    // Decide what sign to put in front
    match sign {
        SignNeg | SignAll if neg => { f('-' as u8); }
        SignAll => { f('+' as u8); }
        _ => ()
    }

    // We built the number in reverse order, so un-reverse it here
    while cur > 0 {
        cur -= 1;
        f(buf[cur]);
    }
}

/**
 * Converts a number to its string representation as a byte vector.
 * This is meant to be a common base implementation for all numeric string
 * conversion functions like `to_str()` or `to_str_radix()`.
 *
 * # Arguments
 * - `num`           - The number to convert. Accepts any number that
 *                     implements the numeric traits.
 * - `radix`         - Base to use. Accepts only the values 2-36.
 * - `negative_zero` - Whether to treat the special value `-0` as
 *                     `-0` or as `+0`.
 * - `sign`          - How to emit the sign. Options are:
 *     - `SignNone`: No sign at all. Basically emits `abs(num)`.
 *     - `SignNeg`:  Only `-` on negative values.
 *     - `SignAll`:  Both `+` on positive, and `-` on negative numbers.
 * - `digits`        - The amount of digits to use for emitting the
 *                     fractional part, if any. Options are:
 *     - `DigAll`:         All calculatable digits. Beware of bignums or
 *                         fractions!
 *     - `DigMax(uint)`:   Maximum N digits, truncating any trailing zeros.
 *     - `DigExact(uint)`: Exactly N digits.
 *
 * # Return value
 * A tuple containing the byte vector, and a boolean flag indicating
 * whether it represents a special value like `inf`, `-inf`, `NaN` or not.
 * It returns a tuple because there can be ambiguity between a special value
 * and a number representation at higher bases.
 *
 * # Failure
 * - Fails if `radix` < 2 or `radix` > 36.
 */
pub fn float_to_str_bytes_common<T:NumCast+Zero+One+Eq+Ord+Float+Round+
                                  Div<T,T>+Neg<T>+Rem<T,T>+Mul<T,T>>(
        num: T, radix: uint, negative_zero: bool,
        sign: SignFormat, digits: SignificantDigits) -> (~[u8], bool) {
    assert!(2 <= radix && radix <= 36);

    let _0: T = Zero::zero();
    let _1: T = One::one();

    match num.classify() {
        FPNaN => { return ("NaN".as_bytes().to_owned(), true); }
        FPInfinite if num > _0 => {
            return match sign {
                SignAll => ("+inf".as_bytes().to_owned(), true),
                _       => ("inf".as_bytes().to_owned(), true)
            };
        }
        FPInfinite if num < _0 => {
            return match sign {
                SignNone => ("inf".as_bytes().to_owned(), true),
                _        => ("-inf".as_bytes().to_owned(), true),
            };
        }
        _ => {}
    }

    let neg = num < _0 || (negative_zero && _1 / num == Float::neg_infinity());
    let mut buf: ~[u8] = ~[];
    let radix_gen: T   = cast(radix as int);

    // First emit the non-fractional part, looping at least once to make
    // sure at least a `0` gets emitted.
    let mut deccum = num.trunc();
    loop {
        // Calculate the absolute value of each digit instead of only
        // doing it once for the whole number because a
        // representable negative number doesn't necessary have an
        // representable additive inverse of the same type
        // (See twos complement). But we assume that for the
        // numbers [-35 .. 0] we always have [0 .. 35].
        let current_digit = (deccum % radix_gen).abs();

        // Decrease the deccumulator one digit at a time
        deccum = deccum / radix_gen;
        deccum = deccum.trunc();

        buf.push(char::from_digit(current_digit.to_int() as uint, radix)
             .unwrap() as u8);

        // No more digits to calculate for the non-fractional part -> break
        if deccum == _0 { break; }
    }

    // If limited digits, calculate one digit more for rounding.
    let (limit_digits, digit_count, exact) = match digits {
        DigAll          => (false, 0u,      false),
        DigMax(count)   => (true,  count+1, false),
        DigExact(count) => (true,  count+1, true)
    };

    // Decide what sign to put in front
    match sign {
        SignNeg | SignAll if neg => {
            buf.push('-' as u8);
        }
        SignAll => {
            buf.push('+' as u8);
        }
        _ => ()
    }

    buf.reverse();

    // Remember start of the fractional digits.
    // Points one beyond end of buf if none get generated,
    // or at the '.' otherwise.
    let start_fractional_digits = buf.len();

    // Now emit the fractional part, if any
    deccum = num.fract();
    if deccum != _0 || (limit_digits && exact && digit_count > 0) {
        buf.push('.' as u8);
        let mut dig = 0u;

        // calculate new digits while
        // - there is no limit and there are digits left
        // - or there is a limit, it's not reached yet and
        //   - it's exact
        //   - or it's a maximum, and there are still digits left
        while (!limit_digits && deccum != _0)
           || (limit_digits && dig < digit_count && (
                   exact
                || (!exact && deccum != _0)
              )
        ) {
            // Shift first fractional digit into the integer part
            deccum = deccum * radix_gen;

            // Calculate the absolute value of each digit.
            // See note in first loop.
            let current_digit = deccum.trunc().abs();

            buf.push(char::from_digit(
                current_digit.to_int() as uint, radix).unwrap() as u8);

            // Decrease the deccumulator one fractional digit at a time
            deccum = deccum.fract();
            dig += 1u;
        }

        // If digits are limited, and that limit has been reached,
        // cut off the one extra digit, and depending on its value
        // round the remaining ones.
        if limit_digits && dig == digit_count {
            let ascii2value = |chr: u8| {
                char::to_digit(chr as char, radix).unwrap() as uint
            };
            let value2ascii = |val: uint| {
                char::from_digit(val, radix).unwrap() as u8
            };

            let extra_digit = ascii2value(buf.pop());
            if extra_digit >= radix / 2 { // -> need to round
                let mut i: int = buf.len() as int - 1;
                loop {
                    // If reached left end of number, have to
                    // insert additional digit:
                    if i < 0
                    || buf[i] == '-' as u8
                    || buf[i] == '+' as u8 {
                        buf.insert((i + 1) as uint, value2ascii(1));
                        break;
                    }

                    // Skip the '.'
                    if buf[i] == '.' as u8 { i -= 1; loop; }

                    // Either increment the digit,
                    // or set to 0 if max and carry the 1.
                    let current_digit = ascii2value(buf[i]);
                    if current_digit < (radix - 1) {
                        buf[i] = value2ascii(current_digit+1);
                        break;
                    } else {
                        buf[i] = value2ascii(0);
                        i -= 1;
                    }
                }
            }
        }
    }

    // if number of digits is not exact, remove all trailing '0's up to
    // and including the '.'
    if !exact {
        let buf_max_i = buf.len() - 1;

        // index to truncate from
        let mut i = buf_max_i;

        // discover trailing zeros of fractional part
        while i > start_fractional_digits && buf[i] == '0' as u8 {
            i -= 1;
        }

        // Only attempt to truncate digits if buf has fractional digits
        if i >= start_fractional_digits {
            // If buf ends with '.', cut that too.
            if buf[i] == '.' as u8 { i -= 1 }

            // only resize buf if we actually remove digits
            if i < buf_max_i {
                buf = buf.slice(0, i + 1).to_owned();
            }
        }
    } // If exact and trailing '.', just cut that
    else {
        let max_i = buf.len() - 1;
        if buf[max_i] == '.' as u8 {
            buf = buf.slice(0, max_i).to_owned();
        }
    }

    (buf, false)
}

/**
 * Converts a number to its string representation. This is a wrapper for
 * `to_str_bytes_common()`, for details see there.
 */
#[inline]
pub fn float_to_str_common<T:NumCast+Zero+One+Eq+Ord+NumStrConv+Float+Round+
                             Div<T,T>+Neg<T>+Rem<T,T>+Mul<T,T>>(
        num: T, radix: uint, negative_zero: bool,
        sign: SignFormat, digits: SignificantDigits) -> (~str, bool) {
    let (bytes, special) = float_to_str_bytes_common(num, radix,
                               negative_zero, sign, digits);
    (str::from_utf8(bytes), special)
}

// Some constants for from_str_bytes_common's input validation,
// they define minimum radix values for which the character is a valid digit.
static DIGIT_P_RADIX: uint = ('p' as uint) - ('a' as uint) + 11u;
static DIGIT_I_RADIX: uint = ('i' as uint) - ('a' as uint) + 11u;
static DIGIT_E_RADIX: uint = ('e' as uint) - ('a' as uint) + 11u;

/**
 * Parses a byte slice as a number. This is meant to
 * be a common base implementation for all numeric string conversion
 * functions like `from_str()` or `from_str_radix()`.
 *
 * # Arguments
 * - `buf`        - The byte slice to parse.
 * - `radix`      - Which base to parse the number as. Accepts 2-36.
 * - `negative`   - Whether to accept negative numbers.
 * - `fractional` - Whether to accept numbers with fractional parts.
 * - `special`    - Whether to accept special values like `inf`
 *                  and `NaN`. Can conflict with `radix`, see Failure.
 * - `exponent`   - Which exponent format to accept. Options are:
 *     - `ExpNone`: No Exponent, accepts just plain numbers like `42` or
 *                  `-8.2`.
 *     - `ExpDec`:  Accepts numbers with a decimal exponent like `42e5` or
 *                  `8.2E-2`. The exponent string itself is always base 10.
 *                  Can conflict with `radix`, see Failure.
 *     - `ExpBin`:  Accepts numbers with a binary exponent like `42P-8` or
 *                  `FFp128`. The exponent string itself is always base 10.
 *                  Can conflict with `radix`, see Failure.
 * - `empty_zero` - Whether to accept a empty `buf` as a 0 or not.
 * - `ignore_underscores` - Whether all underscores within the string should
 *                          be ignored.
 *
 * # Return value
 * Returns `Some(n)` if `buf` parses to a number n without overflowing, and
 * `None` otherwise, depending on the constraints set by the remaining
 * arguments.
 *
 * # Failure
 * - Fails if `radix` < 2 or `radix` > 36.
 * - Fails if `radix` > 14 and `exponent` is `ExpDec` due to conflict
 *   between digit and exponent sign `'e'`.
 * - Fails if `radix` > 25 and `exponent` is `ExpBin` due to conflict
 *   between digit and exponent sign `'p'`.
 * - Fails if `radix` > 18 and `special == true` due to conflict
 *   between digit and lowest first character in `inf` and `NaN`, the `'i'`.
 */
pub fn from_str_bytes_common<T:NumCast+Zero+One+Eq+Ord+Div<T,T>+
                                    Mul<T,T>+Sub<T,T>+Neg<T>+Add<T,T>+
                                    NumStrConv+Clone>(
        buf: &[u8], radix: uint, negative: bool, fractional: bool,
        special: bool, exponent: ExponentFormat, empty_zero: bool,
        ignore_underscores: bool
        ) -> Option<T> {
    match exponent {
        ExpDec if radix >= DIGIT_E_RADIX       // decimal exponent 'e'
          => fail2!("from_str_bytes_common: radix {:?} incompatible with \
                    use of 'e' as decimal exponent", radix),
        ExpBin if radix >= DIGIT_P_RADIX       // binary exponent 'p'
          => fail2!("from_str_bytes_common: radix {:?} incompatible with \
                    use of 'p' as binary exponent", radix),
        _ if special && radix >= DIGIT_I_RADIX // first digit of 'inf'
          => fail2!("from_str_bytes_common: radix {:?} incompatible with \
                    special values 'inf' and 'NaN'", radix),
        _ if (radix as int) < 2
          => fail2!("from_str_bytes_common: radix {:?} to low, \
                    must lie in the range [2, 36]", radix),
        _ if (radix as int) > 36
          => fail2!("from_str_bytes_common: radix {:?} to high, \
                    must lie in the range [2, 36]", radix),
        _ => ()
    }

    let _0: T = Zero::zero();
    let _1: T = One::one();
    let radix_gen: T = cast(radix as int);

    let len = buf.len();

    if len == 0 {
        if empty_zero {
            return Some(_0);
        } else {
            return None;
        }
    }

    if special {
        if buf == INF_BUF || buf == POS_INF_BUF {
            return NumStrConv::inf();
        } else if buf == NEG_INF_BUF {
            if negative {
                return NumStrConv::neg_inf();
            } else {
                return None;
            }
        } else if buf == NAN_BUF {
            return NumStrConv::nan();
        }
    }

    let (start, accum_positive) = match buf[0] as char {
      '-' if !negative => return None,
      '-' => (1u, false),
      '+' => (1u, true),
       _  => (0u, true)
    };

    // Initialize accumulator with signed zero for floating point parsing to
    // work
    let mut accum      = if accum_positive { _0.clone() } else { -_1 * _0};
    let mut last_accum = accum.clone(); // Necessary to detect overflow
    let mut i          = start;
    let mut exp_found  = false;

    // Parse integer part of number
    while i < len {
        let c = buf[i] as char;

        match char::to_digit(c, radix) {
            Some(digit) => {
                // shift accum one digit left
                accum = accum * radix_gen.clone();

                // add/subtract current digit depending on sign
                if accum_positive {
                    accum = accum + cast(digit as int);
                } else {
                    accum = accum - cast(digit as int);
                }

                // Detect overflow by comparing to last value, except
                // if we've not seen any non-zero digits.
                if last_accum != _0 {
                    if accum_positive && accum <= last_accum { return NumStrConv::inf(); }
                    if !accum_positive && accum >= last_accum { return NumStrConv::neg_inf(); }

                    // Detect overflow by reversing the shift-and-add proccess
                    if accum_positive &&
                        (last_accum != ((accum - cast(digit as int))/radix_gen.clone())) {
                        return NumStrConv::inf();
                    }
                    if !accum_positive &&
                        (last_accum != ((accum + cast(digit as int))/radix_gen.clone())) {
                        return NumStrConv::neg_inf();
                    }
                }
                last_accum = accum.clone();
            }
            None => match c {
                '_' if ignore_underscores => {}
                'e' | 'E' | 'p' | 'P' => {
                    exp_found = true;
                    break;                       // start of exponent
                }
                '.' if fractional => {
                    i += 1u;                     // skip the '.'
                    break;                       // start of fractional part
                }
                _ => return None                 // invalid number
            }
        }

        i += 1u;
    }

    // Parse fractional part of number
    // Skip if already reached start of exponent
    if !exp_found {
        let mut power = _1.clone();

        while i < len {
            let c = buf[i] as char;

            match char::to_digit(c, radix) {
                Some(digit) => {
                    // Decrease power one order of magnitude
                    power = power / radix_gen;

                    let digit_t: T = cast(digit);

                    // add/subtract current digit depending on sign
                    if accum_positive {
                        accum = accum + digit_t * power;
                    } else {
                        accum = accum - digit_t * power;
                    }

                    // Detect overflow by comparing to last value
                    if accum_positive && accum < last_accum { return NumStrConv::inf(); }
                    if !accum_positive && accum > last_accum { return NumStrConv::neg_inf(); }
                    last_accum = accum.clone();
                }
                None => match c {
                    '_' if ignore_underscores => {}
                    'e' | 'E' | 'p' | 'P' => {
                        exp_found = true;
                        break;                   // start of exponent
                    }
                    _ => return None             // invalid number
                }
            }

            i += 1u;
        }
    }

    // Special case: buf not empty, but does not contain any digit in front
    // of the exponent sign -> number is empty string
    if i == start {
        if empty_zero {
            return Some(_0);
        } else {
            return None;
        }
    }

    let mut multiplier = _1.clone();

    if exp_found {
        let c = buf[i] as char;
        let base = match (c, exponent) {
            // c is never _ so don't need to handle specially
            ('e', ExpDec) | ('E', ExpDec) => 10u,
            ('p', ExpBin) | ('P', ExpBin) => 2u,
            _ => return None // char doesn't fit given exponent format
        };

        // parse remaining bytes as decimal integer,
        // skipping the exponent char
        let exp: Option<int> = from_str_bytes_common(
            buf.slice(i+1, len), 10, true, false, false, ExpNone, false,
            ignore_underscores);

        match exp {
            Some(exp_pow) => {
                multiplier = if exp_pow < 0 {
                    _1 / pow_with_uint::<T>(base, (-exp_pow.to_int()) as uint)
                } else {
                    pow_with_uint::<T>(base, exp_pow.to_int() as uint)
                }
            }
            None => return None // invalid exponent -> invalid number
        }
    }

    Some(accum * multiplier)
}

/**
 * Parses a string as a number. This is a wrapper for
 * `from_str_bytes_common()`, for details see there.
 */
#[inline]
pub fn from_str_common<T:NumCast+Zero+One+Eq+Ord+Div<T,T>+Mul<T,T>+
                              Sub<T,T>+Neg<T>+Add<T,T>+NumStrConv+Clone>(
        buf: &str, radix: uint, negative: bool, fractional: bool,
        special: bool, exponent: ExponentFormat, empty_zero: bool,
        ignore_underscores: bool
        ) -> Option<T> {
    from_str_bytes_common(buf.as_bytes(), radix, negative,
                          fractional, special, exponent, empty_zero,
                          ignore_underscores)
}

#[cfg(test)]
mod test {
    use super::*;
    use option::*;

    #[test]
    fn from_str_ignore_underscores() {
        let s : Option<u8> = from_str_common("__1__", 2, false, false, false,
                                             ExpNone, false, true);
        assert_eq!(s, Some(1u8));

        let n : Option<u8> = from_str_common("__1__", 2, false, false, false,
                                             ExpNone, false, false);
        assert_eq!(n, None);

        let f : Option<f32> = from_str_common("_1_._5_e_1_", 10, false, true, false,
                                              ExpDec, false, true);
        assert_eq!(f, Some(1.5e1f32));
    }

    #[test]
    fn from_str_issue5770() {
        // try to parse 0b1_1111_1111 = 511 as a u8. Caused problems
        // since 255*2+1 == 255 (mod 256) so the overflow wasn't
        // detected.
        let n : Option<u8> = from_str_common("111111111", 2, false, false, false,
                                             ExpNone, false, false);
        assert_eq!(n, None);
    }

    #[test]
    fn from_str_issue7588() {
        let u : Option<u8> = from_str_common("1000", 10, false, false, false,
                                            ExpNone, false, false);
        assert_eq!(u, None);
        let s : Option<i16> = from_str_common("80000", 10, false, false, false,
                                             ExpNone, false, false);
        assert_eq!(s, None);
        let f : Option<f32> = from_str_common(
            "10000000000000000000000000000000000000000", 10, false, false, false,
            ExpNone, false, false);
        assert_eq!(f, NumStrConv::inf())
        let fe : Option<f32> = from_str_common("1e40", 10, false, false, false,
                                            ExpDec, false, false);
        assert_eq!(fe, NumStrConv::inf())
    }
}

#[cfg(test)]
mod bench {
    use extra::test::BenchHarness;
    use rand::{XorShiftRng, Rng};
    use to_str::ToStr;
    use f64;

    #[bench]
    fn uint_to_str_rand(bh: &mut BenchHarness) {
        let mut rng = XorShiftRng::new();
        do bh.iter {
            rng.gen::<uint>().to_str();
        }
    }

    #[bench]
    fn float_to_str_rand(bh: &mut BenchHarness) {
        let mut rng = XorShiftRng::new();
        do bh.iter {
            f64::to_str(rng.gen());
        }
    }
}
