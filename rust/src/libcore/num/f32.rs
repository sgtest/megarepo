// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! This module provides constants which are specific to the implementation
//! of the `f32` floating point data type.
//!
//! Mathematically significant numbers are provided in the `consts` sub-module.
//!
//! *[See also the `f32` primitive type](../../std/primitive.f32.html).*

#![stable(feature = "rust1", since = "1.0.0")]

use intrinsics;
use mem;
use num::Float;
use num::FpCategory as Fp;

/// The radix or base of the internal representation of `f32`.
#[stable(feature = "rust1", since = "1.0.0")]
pub const RADIX: u32 = 2;

/// Number of significant digits in base 2.
#[stable(feature = "rust1", since = "1.0.0")]
pub const MANTISSA_DIGITS: u32 = 24;
/// Approximate number of significant digits in base 10.
#[stable(feature = "rust1", since = "1.0.0")]
pub const DIGITS: u32 = 6;

/// Difference between `1.0` and the next largest representable number.
#[stable(feature = "rust1", since = "1.0.0")]
pub const EPSILON: f32 = 1.19209290e-07_f32;

/// Smallest finite `f32` value.
#[stable(feature = "rust1", since = "1.0.0")]
pub const MIN: f32 = -3.40282347e+38_f32;
/// Smallest positive normal `f32` value.
#[stable(feature = "rust1", since = "1.0.0")]
pub const MIN_POSITIVE: f32 = 1.17549435e-38_f32;
/// Largest finite `f32` value.
#[stable(feature = "rust1", since = "1.0.0")]
pub const MAX: f32 = 3.40282347e+38_f32;

/// One greater than the minimum possible normal power of 2 exponent.
#[stable(feature = "rust1", since = "1.0.0")]
pub const MIN_EXP: i32 = -125;
/// Maximum possible power of 2 exponent.
#[stable(feature = "rust1", since = "1.0.0")]
pub const MAX_EXP: i32 = 128;

/// Minimum possible normal power of 10 exponent.
#[stable(feature = "rust1", since = "1.0.0")]
pub const MIN_10_EXP: i32 = -37;
/// Maximum possible power of 10 exponent.
#[stable(feature = "rust1", since = "1.0.0")]
pub const MAX_10_EXP: i32 = 38;

/// Not a Number (NaN).
#[stable(feature = "rust1", since = "1.0.0")]
pub const NAN: f32 = 0.0_f32 / 0.0_f32;
/// Infinity (∞).
#[stable(feature = "rust1", since = "1.0.0")]
pub const INFINITY: f32 = 1.0_f32 / 0.0_f32;
/// Negative infinity (-∞).
#[stable(feature = "rust1", since = "1.0.0")]
pub const NEG_INFINITY: f32 = -1.0_f32 / 0.0_f32;

/// Basic mathematical constants.
#[stable(feature = "rust1", since = "1.0.0")]
pub mod consts {
    // FIXME: replace with mathematical constants from cmath.

    /// Archimedes' constant (π)
    #[stable(feature = "rust1", since = "1.0.0")]
    pub const PI: f32 = 3.14159265358979323846264338327950288_f32;

    /// π/2
    #[stable(feature = "rust1", since = "1.0.0")]
    pub const FRAC_PI_2: f32 = 1.57079632679489661923132169163975144_f32;

    /// π/3
    #[stable(feature = "rust1", since = "1.0.0")]
    pub const FRAC_PI_3: f32 = 1.04719755119659774615421446109316763_f32;

    /// π/4
    #[stable(feature = "rust1", since = "1.0.0")]
    pub const FRAC_PI_4: f32 = 0.785398163397448309615660845819875721_f32;

    /// π/6
    #[stable(feature = "rust1", since = "1.0.0")]
    pub const FRAC_PI_6: f32 = 0.52359877559829887307710723054658381_f32;

    /// π/8
    #[stable(feature = "rust1", since = "1.0.0")]
    pub const FRAC_PI_8: f32 = 0.39269908169872415480783042290993786_f32;

    /// 1/π
    #[stable(feature = "rust1", since = "1.0.0")]
    pub const FRAC_1_PI: f32 = 0.318309886183790671537767526745028724_f32;

    /// 2/π
    #[stable(feature = "rust1", since = "1.0.0")]
    pub const FRAC_2_PI: f32 = 0.636619772367581343075535053490057448_f32;

    /// 2/sqrt(π)
    #[stable(feature = "rust1", since = "1.0.0")]
    pub const FRAC_2_SQRT_PI: f32 = 1.12837916709551257389615890312154517_f32;

    /// sqrt(2)
    #[stable(feature = "rust1", since = "1.0.0")]
    pub const SQRT_2: f32 = 1.41421356237309504880168872420969808_f32;

    /// 1/sqrt(2)
    #[stable(feature = "rust1", since = "1.0.0")]
    pub const FRAC_1_SQRT_2: f32 = 0.707106781186547524400844362104849039_f32;

    /// Euler's number (e)
    #[stable(feature = "rust1", since = "1.0.0")]
    pub const E: f32 = 2.71828182845904523536028747135266250_f32;

    /// log<sub>2</sub>(e)
    #[stable(feature = "rust1", since = "1.0.0")]
    pub const LOG2_E: f32 = 1.44269504088896340735992468100189214_f32;

    /// log<sub>10</sub>(e)
    #[stable(feature = "rust1", since = "1.0.0")]
    pub const LOG10_E: f32 = 0.434294481903251827651128918916605082_f32;

    /// ln(2)
    #[stable(feature = "rust1", since = "1.0.0")]
    pub const LN_2: f32 = 0.693147180559945309417232121458176568_f32;

    /// ln(10)
    #[stable(feature = "rust1", since = "1.0.0")]
    pub const LN_10: f32 = 2.30258509299404568401799145468436421_f32;
}

#[unstable(feature = "core_float",
           reason = "stable interface is via `impl f{32,64}` in later crates",
           issue = "32110")]
impl Float for f32 {
    /// Returns `true` if the number is NaN.
    #[inline]
    fn is_nan(self) -> bool {
        self != self
    }

    /// Returns `true` if the number is infinite.
    #[inline]
    fn is_infinite(self) -> bool {
        self == INFINITY || self == NEG_INFINITY
    }

    /// Returns `true` if the number is neither infinite or NaN.
    #[inline]
    fn is_finite(self) -> bool {
        !(self.is_nan() || self.is_infinite())
    }

    /// Returns `true` if the number is neither zero, infinite, subnormal or NaN.
    #[inline]
    fn is_normal(self) -> bool {
        self.classify() == Fp::Normal
    }

    /// Returns the floating point category of the number. If only one property
    /// is going to be tested, it is generally faster to use the specific
    /// predicate instead.
    fn classify(self) -> Fp {
        const EXP_MASK: u32 = 0x7f800000;
        const MAN_MASK: u32 = 0x007fffff;

        let bits: u32 = unsafe { mem::transmute(self) };
        match (bits & MAN_MASK, bits & EXP_MASK) {
            (0, 0) => Fp::Zero,
            (_, 0) => Fp::Subnormal,
            (0, EXP_MASK) => Fp::Infinite,
            (_, EXP_MASK) => Fp::Nan,
            _ => Fp::Normal,
        }
    }

    /// Computes the absolute value of `self`. Returns `Float::nan()` if the
    /// number is `Float::nan()`.
    #[inline]
    fn abs(self) -> f32 {
        unsafe { intrinsics::fabsf32(self) }
    }

    /// Returns a number that represents the sign of `self`.
    ///
    /// - `1.0` if the number is positive, `+0.0` or `Float::infinity()`
    /// - `-1.0` if the number is negative, `-0.0` or `Float::neg_infinity()`
    /// - `Float::nan()` if the number is `Float::nan()`
    #[inline]
    fn signum(self) -> f32 {
        if self.is_nan() {
            NAN
        } else {
            unsafe { intrinsics::copysignf32(1.0, self) }
        }
    }

    /// Returns `true` if and only if `self` has a positive sign, including `+0.0`, `NaN`s with
    /// positive sign bit and positive infinity.
    #[inline]
    fn is_sign_positive(self) -> bool {
        !self.is_sign_negative()
    }

    /// Returns `true` if and only if `self` has a negative sign, including `-0.0`, `NaN`s with
    /// negative sign bit and negative infinity.
    #[inline]
    fn is_sign_negative(self) -> bool {
        // IEEE754 says: isSignMinus(x) is true if and only if x has negative sign. isSignMinus
        // applies to zeros and NaNs as well.
        #[repr(C)]
        union F32Bytes {
            f: f32,
            b: u32
        }
        unsafe { F32Bytes { f: self }.b & 0x8000_0000 != 0 }
    }

    /// Returns the reciprocal (multiplicative inverse) of the number.
    #[inline]
    fn recip(self) -> f32 {
        1.0 / self
    }

    #[inline]
    fn powi(self, n: i32) -> f32 {
        unsafe { intrinsics::powif32(self, n) }
    }

    /// Converts to degrees, assuming the number is in radians.
    #[inline]
    fn to_degrees(self) -> f32 {
        self * (180.0f32 / consts::PI)
    }

    /// Converts to radians, assuming the number is in degrees.
    #[inline]
    fn to_radians(self) -> f32 {
        let value: f32 = consts::PI;
        self * (value / 180.0f32)
    }

    /// Returns the maximum of the two numbers.
    #[inline]
    fn max(self, other: f32) -> f32 {
        // IEEE754 says: maxNum(x, y) is the canonicalized number y if x < y, x if y < x, the
        // canonicalized number if one operand is a number and the other a quiet NaN. Otherwise it
        // is either x or y, canonicalized (this means results might differ among implementations).
        // When either x or y is a signalingNaN, then the result is according to 6.2.
        //
        // Since we do not support sNaN in Rust yet, we do not need to handle them.
        // FIXME(nagisa): due to https://bugs.llvm.org/show_bug.cgi?id=33303 we canonicalize by
        // multiplying by 1.0. Should switch to the `canonicalize` when it works.
        (if self < other || self.is_nan() { other } else { self }) * 1.0
    }

    /// Returns the minimum of the two numbers.
    #[inline]
    fn min(self, other: f32) -> f32 {
        // IEEE754 says: minNum(x, y) is the canonicalized number x if x < y, y if y < x, the
        // canonicalized number if one operand is a number and the other a quiet NaN. Otherwise it
        // is either x or y, canonicalized (this means results might differ among implementations).
        // When either x or y is a signalingNaN, then the result is according to 6.2.
        //
        // Since we do not support sNaN in Rust yet, we do not need to handle them.
        // FIXME(nagisa): due to https://bugs.llvm.org/show_bug.cgi?id=33303 we canonicalize by
        // multiplying by 1.0. Should switch to the `canonicalize` when it works.
        (if self < other || other.is_nan() { self } else { other }) * 1.0
    }
}
