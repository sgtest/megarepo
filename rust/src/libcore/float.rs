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

//! Operations and constants for `float`

// Even though this module exports everything defined in it,
// because it contains re-exports, we also have to explicitly
// export locally defined things. That's a bit annoying.


// export when m_float == c_double


// PORT this must match in width according to architecture

use m_float = f64;

use cmp::{Eq, Ord};
use cmp;
use f64;
use num;
use num::Num::from_int;
use option::{None, Option, Some};
use str;
use uint;

pub use f64::{add, sub, mul, div, rem, lt, le, eq, ne, ge, gt};
pub use f64::logarithm;
pub use f64::{acos, asin, atan2, cbrt, ceil, copysign, cosh, floor};
pub use f64::{erf, erfc, exp, expm1, exp2, abs_sub};
pub use f64::{mul_add, fmax, fmin, nextafter, frexp, hypot, ldexp};
pub use f64::{lgamma, ln, log_radix, ln1p, log10, log2, ilog_radix};
pub use f64::{modf, pow, round, sinh, tanh, tgamma, trunc};
pub use f64::signbit;
pub use f64::{j0, j1, jn, y0, y1, yn};

pub const NaN: float = 0.0/0.0;

pub const infinity: float = 1.0/0.0;

pub const neg_infinity: float = -1.0/0.0;

/* Module: consts */
pub mod consts {
    // FIXME (requires Issue #1433 to fix): replace with mathematical
    // constants from cmath.
    /// Archimedes' constant
    pub const pi: float = 3.14159265358979323846264338327950288;

    /// pi/2.0
    pub const frac_pi_2: float = 1.57079632679489661923132169163975144;

    /// pi/4.0
    pub const frac_pi_4: float = 0.785398163397448309615660845819875721;

    /// 1.0/pi
    pub const frac_1_pi: float = 0.318309886183790671537767526745028724;

    /// 2.0/pi
    pub const frac_2_pi: float = 0.636619772367581343075535053490057448;

    /// 2.0/sqrt(pi)
    pub const frac_2_sqrtpi: float = 1.12837916709551257389615890312154517;

    /// sqrt(2.0)
    pub const sqrt2: float = 1.41421356237309504880168872420969808;

    /// 1.0/sqrt(2.0)
    pub const frac_1_sqrt2: float = 0.707106781186547524400844362104849039;

    /// Euler's number
    pub const e: float = 2.71828182845904523536028747135266250;

    /// log2(e)
    pub const log2_e: float = 1.44269504088896340735992468100189214;

    /// log10(e)
    pub const log10_e: float = 0.434294481903251827651128918916605082;

    /// ln(2.0)
    pub const ln_2: float = 0.693147180559945309417232121458176568;

    /// ln(10.0)
    pub const ln_10: float = 2.30258509299404568401799145468436421;
}

/**
 * Section: String Conversions
 */

/**
 * Converts a float to a string
 *
 * # Arguments
 *
 * * num - The float value
 * * digits - The number of significant digits
 * * exact - Whether to enforce the exact number of significant digits
 */
pub pure fn to_str_common(num: float, digits: uint, exact: bool) -> ~str {
    if is_NaN(num) { return ~"NaN"; }
    if num == infinity { return ~"inf"; }
    if num == neg_infinity { return ~"-inf"; }

    let mut (num, sign) = if num < 0.0 { (-num, ~"-") } else { (num, ~"") };

    // truncated integer
    let trunc = num as uint;

    // decimal remainder
    let mut frac = num - (trunc as float);

    // stack of digits
    let mut fractionalParts = ~[];

    // FIXME: (#2608)
    // This used to return right away without rounding, as "~[-]num",
    // but given epsilon like in f64.rs, I don't see how the comparison
    // to epsilon did much when only used there.
    //    if (frac < epsilon && !exact) || digits == 0u { return accum; }
    //
    // With something better, possibly weird results like this can be avoided:
    //     assert "3.14158999999999988262" == my_to_str_exact(3.14159, 20u);

    let mut ii = digits;
    let mut epsilon_prime = 1.0 / pow_with_uint(10u, ii);

    // while we still need digits
    // build stack of digits
    while ii > 0 && (frac >= epsilon_prime || exact) {
        // store the next digit
        frac *= 10.0;
        let digit = frac as uint;
        // Bleh: not really unsafe.
        unsafe { fractionalParts.push(digit); }

        // calculate the next frac
        frac -= digit as float;
        epsilon_prime *= 10.0;
        ii -= 1u;
    }

    let mut acc;
    let mut racc = ~"";
    let mut carry = if frac * 10.0 as uint >= 5 { 1 } else { 0 };

    // turn digits into string
    // using stack of digits
    while fractionalParts.is_not_empty() {
        // Bleh; shouldn't need to be unsafe
        let mut adjusted_digit = carry + unsafe { fractionalParts.pop() };

        if adjusted_digit == 10 {
            carry = 1;
            adjusted_digit %= 10
        } else {
            carry = 0;
        };

        racc = uint::str(adjusted_digit) + racc;
    }

    // pad decimals with trailing zeroes
    while racc.len() < digits && exact {
        racc += ~"0"
    }

    // combine ints and decimals
    let mut ones = uint::str(trunc + carry);
    if racc == ~"" {
        acc = sign + ones;
    } else {
        acc = sign + ones + ~"." + racc;
    }
    move acc
}

/**
 * Converts a float to a string with exactly the number of
 * provided significant digits
 *
 * # Arguments
 *
 * * num - The float value
 * * digits - The number of significant digits
 */
#[inline(always)]
pub pure fn to_str_exact(num: float, digits: uint) -> ~str {
    to_str_common(num, digits, true)
}

#[test]
pub fn test_to_str_exact_do_decimal() {
    let s = to_str_exact(5.0, 4u);
    assert s == ~"5.0000";
}


/**
 * Converts a float to a string with a maximum number of
 * significant digits
 *
 * # Arguments
 *
 * * num - The float value
 * * digits - The number of significant digits
 */
#[inline(always)]
pub pure fn to_str(num: float, digits: uint) -> ~str {
    to_str_common(num, digits, false)
}

/**
 * Convert a string to a float
 *
 * This function accepts strings such as
 *
 * * '3.14'
 * * '+3.14', equivalent to '3.14'
 * * '-3.14'
 * * '2.5E10', or equivalently, '2.5e10'
 * * '2.5E-10'
 * * '', or, equivalently, '.' (understood as 0)
 * * '5.'
 * * '.5', or, equivalently,  '0.5'
 * * 'inf', '-inf', 'NaN'
 *
 * Leading and trailing whitespace are ignored.
 *
 * # Arguments
 *
 * * num - A string
 *
 * # Return value
 *
 * `none` if the string did not represent a valid number.  Otherwise,
 * `Some(n)` where `n` is the floating-point number represented by `[num]`.
 */
pub pure fn from_str(num: &str) -> Option<float> {
   if num == "inf" {
       return Some(infinity as float);
   } else if num == "-inf" {
       return Some(neg_infinity as float);
   } else if num == "NaN" {
       return Some(NaN as float);
   }

   let mut pos = 0u;               //Current byte position in the string.
                                   //Used to walk the string in O(n).
   let len = str::len(num);        //Length of the string, in bytes.

   if len == 0u { return None; }
   let mut total = 0f;             //Accumulated result
   let mut c     = 'z';            //Latest char.

   //The string must start with one of the following characters.
   match str::char_at(num, 0u) {
      '-' | '+' | '0' .. '9' | '.' => (),
      _ => return None
   }

   //Determine if first char is '-'/'+'. Set [pos] and [neg] accordingly.
   let mut neg = false;               //Sign of the result
   match str::char_at(num, 0u) {
      '-' => {
          neg = true;
          pos = 1u;
      }
      '+' => {
          pos = 1u;
      }
      _ => ()
   }

   //Examine the following chars until '.', 'e', 'E'
   while(pos < len) {
       let char_range = str::char_range_at(num, pos);
       c   = char_range.ch;
       pos = char_range.next;
       match c {
         '0' .. '9' => {
           total = total * 10f;
           total += ((c as int) - ('0' as int)) as float;
         }
         '.' | 'e' | 'E' => break,
         _ => return None
       }
   }

   if c == '.' {//Examine decimal part
      let mut decimal = 1f;
      while(pos < len) {
         let char_range = str::char_range_at(num, pos);
         c = char_range.ch;
         pos = char_range.next;
         match c {
            '0' | '1' | '2' | '3' | '4' | '5' | '6'| '7' | '8' | '9'  => {
                 decimal /= 10f;
                 total += (((c as int) - ('0' as int)) as float)*decimal;
             }
             'e' | 'E' => break,
             _ => return None
         }
      }
   }

   if (c == 'e') || (c == 'E') { //Examine exponent
      let mut exponent = 0u;
      let mut neg_exponent = false;
      if(pos < len) {
          let char_range = str::char_range_at(num, pos);
          c   = char_range.ch;
          match c  {
             '+' => {
                pos = char_range.next;
             }
             '-' => {
                pos = char_range.next;
                neg_exponent = true;
             }
             _ => ()
          }
          while(pos < len) {
             let char_range = str::char_range_at(num, pos);
             c = char_range.ch;
             match c {
                 '0' | '1' | '2' | '3' | '4' | '5' | '6'| '7' | '8' | '9' => {
                     exponent *= 10u;
                     exponent += ((c as uint) - ('0' as uint));
                 }
                 _ => break
             }
             pos = char_range.next;
          }
          let multiplier = pow_with_uint(10u, exponent);
              //Note: not ~[int::pow], otherwise, we'll quickly
              //end up with a nice overflow
          if neg_exponent {
             total = total / multiplier;
          } else {
             total = total * multiplier;
          }
      } else {
         return None;
      }
   }

   if(pos < len) {
     return None;
   } else {
     if(neg) {
        total *= -1f;
     }
     return Some(total);
   }
}

/**
 * Section: Arithmetics
 */

/**
 * Compute the exponentiation of an integer by another integer as a float
 *
 * # Arguments
 *
 * * x - The base
 * * pow - The exponent
 *
 * # Return value
 *
 * `NaN` if both `x` and `pow` are `0u`, otherwise `x^pow`
 */
pub pure fn pow_with_uint(base: uint, pow: uint) -> float {
    if base == 0u {
        if pow == 0u {
            return NaN as float;
        }
        return 0.;
    }
    let mut my_pow     = pow;
    let mut total      = 1f;
    let mut multiplier = base as float;
    while (my_pow > 0u) {
        if my_pow % 2u == 1u {
            total = total * multiplier;
        }
        my_pow     /= 2u;
        multiplier *= multiplier;
    }
    return total;
}

#[inline(always)]
pub pure fn is_positive(x: float) -> bool { f64::is_positive(x as f64) }
#[inline(always)]
pub pure fn is_negative(x: float) -> bool { f64::is_negative(x as f64) }
#[inline(always)]
pub pure fn is_nonpositive(x: float) -> bool { f64::is_nonpositive(x as f64) }
#[inline(always)]
pub pure fn is_nonnegative(x: float) -> bool { f64::is_nonnegative(x as f64) }
#[inline(always)]
pub pure fn is_zero(x: float) -> bool { f64::is_zero(x as f64) }
#[inline(always)]
pub pure fn is_infinite(x: float) -> bool { f64::is_infinite(x as f64) }
#[inline(always)]
pub pure fn is_finite(x: float) -> bool { f64::is_finite(x as f64) }
#[inline(always)]
pub pure fn is_NaN(x: float) -> bool { f64::is_NaN(x as f64) }

#[inline(always)]
pub pure fn abs(x: float) -> float {
    unsafe { f64::abs(x as f64) as float }
}
#[inline(always)]
pub pure fn sqrt(x: float) -> float {
    unsafe { f64::sqrt(x as f64) as float }
}
#[inline(always)]
pub pure fn atan(x: float) -> float {
    unsafe { f64::atan(x as f64) as float }
}
#[inline(always)]
pub pure fn sin(x: float) -> float {
    unsafe { f64::sin(x as f64) as float }
}
#[inline(always)]
pub pure fn cos(x: float) -> float {
    unsafe { f64::cos(x as f64) as float }
}
#[inline(always)]
pub pure fn tan(x: float) -> float {
    unsafe { f64::tan(x as f64) as float }
}

#[cfg(notest)]
impl float : Eq {
    pure fn eq(&self, other: &float) -> bool { (*self) == (*other) }
    pure fn ne(&self, other: &float) -> bool { (*self) != (*other) }
}

#[cfg(notest)]
impl float : Ord {
    pure fn lt(&self, other: &float) -> bool { (*self) < (*other) }
    pure fn le(&self, other: &float) -> bool { (*self) <= (*other) }
    pure fn ge(&self, other: &float) -> bool { (*self) >= (*other) }
    pure fn gt(&self, other: &float) -> bool { (*self) > (*other) }
}

impl float: num::Num {
    #[inline(always)]
    pub pure fn add(&self, other: &float) -> float { return *self + *other; }
    #[inline(always)]
    pub pure fn sub(&self, other: &float) -> float { return *self - *other; }
    #[inline(always)]
    pub pure fn mul(&self, other: &float) -> float { return *self * *other; }
    #[inline(always)]
    pub pure fn div(&self, other: &float) -> float { return *self / *other; }
    #[inline(always)]
    pure fn modulo(&self, other: &float) -> float { return *self % *other; }
    #[inline(always)]
    pure fn neg(&self)                  -> float { return -*self;        }

    #[inline(always)]
    pure fn to_int(&self)         -> int   { return *self as int; }
    #[inline(always)]
    static pure fn from_int(&self, n: int) -> float { return n as float;  }
}

impl float: num::Zero {
    #[inline(always)]
    static pure fn zero() -> float { 0.0 }
}

impl float: num::One {
    #[inline(always)]
    static pure fn one() -> float { 1.0 }
}

#[test]
pub fn test_from_str() {
   assert from_str(~"3") == Some(3.);
   assert from_str(~"3") == Some(3.);
   assert from_str(~"3.14") == Some(3.14);
   assert from_str(~"+3.14") == Some(3.14);
   assert from_str(~"-3.14") == Some(-3.14);
   assert from_str(~"2.5E10") == Some(25000000000.);
   assert from_str(~"2.5e10") == Some(25000000000.);
   assert from_str(~"25000000000.E-10") == Some(2.5);
   assert from_str(~".") == Some(0.);
   assert from_str(~".e1") == Some(0.);
   assert from_str(~".e-1") == Some(0.);
   assert from_str(~"5.") == Some(5.);
   assert from_str(~".5") == Some(0.5);
   assert from_str(~"0.5") == Some(0.5);
   assert from_str(~"0.5") == Some(0.5);
   assert from_str(~"0.5") == Some(0.5);
   assert from_str(~"-.5") == Some(-0.5);
   assert from_str(~"-.5") == Some(-0.5);
   assert from_str(~"-5") == Some(-5.);
   assert from_str(~"-0") == Some(-0.);
   assert from_str(~"0") == Some(0.);
   assert from_str(~"inf") == Some(infinity);
   assert from_str(~"-inf") == Some(neg_infinity);
   // note: NaN != NaN, hence this slightly complex test
   match from_str(~"NaN") {
       Some(f) => assert is_NaN(f),
       None => fail
   }

   assert from_str(~"").is_none();
   assert from_str(~"x").is_none();
   assert from_str(~" ").is_none();
   assert from_str(~"   ").is_none();
   assert from_str(~"e").is_none();
   assert from_str(~"E").is_none();
   assert from_str(~"E1").is_none();
   assert from_str(~"1e1e1").is_none();
   assert from_str(~"1e1.1").is_none();
   assert from_str(~"1e1-1").is_none();
}

#[test]
pub fn test_positive() {
  assert(is_positive(infinity));
  assert(is_positive(1.));
  assert(is_positive(0.));
  assert(!is_positive(-1.));
  assert(!is_positive(neg_infinity));
  assert(!is_positive(1./neg_infinity));
  assert(!is_positive(NaN));
}

#[test]
pub fn test_negative() {
  assert(!is_negative(infinity));
  assert(!is_negative(1.));
  assert(!is_negative(0.));
  assert(is_negative(-1.));
  assert(is_negative(neg_infinity));
  assert(is_negative(1./neg_infinity));
  assert(!is_negative(NaN));
}

#[test]
pub fn test_nonpositive() {
  assert(!is_nonpositive(infinity));
  assert(!is_nonpositive(1.));
  assert(!is_nonpositive(0.));
  assert(is_nonpositive(-1.));
  assert(is_nonpositive(neg_infinity));
  assert(is_nonpositive(1./neg_infinity));
  assert(!is_nonpositive(NaN));
}

#[test]
pub fn test_nonnegative() {
  assert(is_nonnegative(infinity));
  assert(is_nonnegative(1.));
  assert(is_nonnegative(0.));
  assert(!is_nonnegative(-1.));
  assert(!is_nonnegative(neg_infinity));
  assert(!is_nonnegative(1./neg_infinity));
  assert(!is_nonnegative(NaN));
}

#[test]
pub fn test_to_str_inf() {
    assert to_str(infinity, 10u) == ~"inf";
    assert to_str(-infinity, 10u) == ~"-inf";
}

#[test]
pub fn test_traits() {
    fn test<U:num::Num cmp::Eq>(ten: &U) {
        assert (ten.to_int() == 10);

        let two: U = from_int(2);
        assert (two.to_int() == 2);

        assert (ten.add(&two) == from_int(12));
        assert (ten.sub(&two) == from_int(8));
        assert (ten.mul(&two) == from_int(20));
        assert (ten.div(&two) == from_int(5));
        assert (ten.modulo(&two) == from_int(0));
    }

    test(&10.0);
}


//
// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
//





