// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Numeric traits and functions for generic mathematics
//!
//! These are implemented for the primitive numeric types in `std::{u8, u16,
//! u32, u64, uint, i8, i16, i32, i64, int, f32, f64}`.

#![stable]
#![allow(missing_docs)]

#[cfg(test)] use cmp::PartialEq;
#[cfg(test)] use fmt::Show;
#[cfg(test)] use ops::{Add, Sub, Mul, Div, Rem};
#[cfg(test)] use kinds::Copy;

pub use core::num::{Num, div_rem, Zero, zero, One, one};
pub use core::num::{Unsigned, pow, Bounded};
pub use core::num::{Primitive, Int, SignedInt, UnsignedInt};
pub use core::num::{cast, FromPrimitive, NumCast, ToPrimitive};
pub use core::num::{next_power_of_two, is_power_of_two};
pub use core::num::{checked_next_power_of_two};
pub use core::num::{from_int, from_i8, from_i16, from_i32, from_i64};
pub use core::num::{from_uint, from_u8, from_u16, from_u32, from_u64};
pub use core::num::{from_f32, from_f64};
pub use core::num::{FromStrRadix, from_str_radix};
pub use core::num::{FpCategory, Float};

#[experimental = "may be removed or relocated"]
pub mod strconv;

/// Mathematical operations on primitive floating point numbers.
#[unstable = "may be altered to inline the Float trait"]
pub trait FloatMath: Float {
    /// Constructs a floating point number created by multiplying `x` by 2
    /// raised to the power of `exp`.
    fn ldexp(x: Self, exp: int) -> Self;
    /// Breaks the number into a normalized fraction and a base-2 exponent,
    /// satisfying:
    ///
    ///  * `self = x * pow(2, exp)`
    ///
    ///  * `0.5 <= abs(x) < 1.0`
    fn frexp(self) -> (Self, int);

    /// Returns the next representable floating-point value in the direction of
    /// `other`.
    fn next_after(self, other: Self) -> Self;

    /// Returns the maximum of the two numbers.
    fn max(self, other: Self) -> Self;
    /// Returns the minimum of the two numbers.
    fn min(self, other: Self) -> Self;

    /// The positive difference of two numbers. Returns `0.0` if the number is
    /// less than or equal to `other`, otherwise the difference between`self`
    /// and `other` is returned.
    fn abs_sub(self, other: Self) -> Self;

    /// Take the cubic root of a number.
    fn cbrt(self) -> Self;
    /// Calculate the length of the hypotenuse of a right-angle triangle given
    /// legs of length `x` and `y`.
    fn hypot(self, other: Self) -> Self;

    /// Computes the sine of a number (in radians).
    fn sin(self) -> Self;
    /// Computes the cosine of a number (in radians).
    fn cos(self) -> Self;
    /// Computes the tangent of a number (in radians).
    fn tan(self) -> Self;

    /// Computes the arcsine of a number. Return value is in radians in
    /// the range [-pi/2, pi/2] or NaN if the number is outside the range
    /// [-1, 1].
    fn asin(self) -> Self;
    /// Computes the arccosine of a number. Return value is in radians in
    /// the range [0, pi] or NaN if the number is outside the range
    /// [-1, 1].
    fn acos(self) -> Self;
    /// Computes the arctangent of a number. Return value is in radians in the
    /// range [-pi/2, pi/2];
    fn atan(self) -> Self;
    /// Computes the four quadrant arctangent of a number, `y`, and another
    /// number `x`. Return value is in radians in the range [-pi, pi].
    fn atan2(self, other: Self) -> Self;
    /// Simultaneously computes the sine and cosine of the number, `x`. Returns
    /// `(sin(x), cos(x))`.
    fn sin_cos(self) -> (Self, Self);

    /// Returns the exponential of the number, minus 1, in a way that is
    /// accurate even if the number is close to zero.
    fn exp_m1(self) -> Self;
    /// Returns the natural logarithm of the number plus 1 (`ln(1+n)`) more
    /// accurately than if the operations were performed separately.
    fn ln_1p(self) -> Self;

    /// Hyperbolic sine function.
    fn sinh(self) -> Self;
    /// Hyperbolic cosine function.
    fn cosh(self) -> Self;
    /// Hyperbolic tangent function.
    fn tanh(self) -> Self;
    /// Inverse hyperbolic sine function.
    fn asinh(self) -> Self;
    /// Inverse hyperbolic cosine function.
    fn acosh(self) -> Self;
    /// Inverse hyperbolic tangent function.
    fn atanh(self) -> Self;
}

// DEPRECATED

#[deprecated = "Use `FloatMath::abs_sub`"]
pub fn abs_sub<T: FloatMath>(x: T, y: T) -> T {
    x.abs_sub(y)
}

/// Helper function for testing numeric operations
#[cfg(test)]
pub fn test_num<T>(ten: T, two: T) where
    T: PartialEq + NumCast
     + Add<T, T> + Sub<T, T>
     + Mul<T, T> + Div<T, T>
     + Rem<T, T> + Show
     + Copy
{
    assert_eq!(ten.add(two),  cast(12i).unwrap());
    assert_eq!(ten.sub(two),  cast(8i).unwrap());
    assert_eq!(ten.mul(two),  cast(20i).unwrap());
    assert_eq!(ten.div(two),  cast(5i).unwrap());
    assert_eq!(ten.rem(two),  cast(0i).unwrap());

    assert_eq!(ten.add(two),  ten + two);
    assert_eq!(ten.sub(two),  ten - two);
    assert_eq!(ten.mul(two),  ten * two);
    assert_eq!(ten.div(two),  ten / two);
    assert_eq!(ten.rem(two),  ten % two);
}

#[cfg(test)]
mod tests {
    use prelude::*;
    use super::*;
    use i8;
    use i16;
    use i32;
    use i64;
    use int;
    use u8;
    use u16;
    use u32;
    use u64;
    use uint;

    macro_rules! test_cast_20 {
        ($_20:expr) => ({
            let _20 = $_20;

            assert_eq!(20u,   _20.to_uint().unwrap());
            assert_eq!(20u8,  _20.to_u8().unwrap());
            assert_eq!(20u16, _20.to_u16().unwrap());
            assert_eq!(20u32, _20.to_u32().unwrap());
            assert_eq!(20u64, _20.to_u64().unwrap());
            assert_eq!(20i,   _20.to_int().unwrap());
            assert_eq!(20i8,  _20.to_i8().unwrap());
            assert_eq!(20i16, _20.to_i16().unwrap());
            assert_eq!(20i32, _20.to_i32().unwrap());
            assert_eq!(20i64, _20.to_i64().unwrap());
            assert_eq!(20f32, _20.to_f32().unwrap());
            assert_eq!(20f64, _20.to_f64().unwrap());

            assert_eq!(_20, NumCast::from(20u).unwrap());
            assert_eq!(_20, NumCast::from(20u8).unwrap());
            assert_eq!(_20, NumCast::from(20u16).unwrap());
            assert_eq!(_20, NumCast::from(20u32).unwrap());
            assert_eq!(_20, NumCast::from(20u64).unwrap());
            assert_eq!(_20, NumCast::from(20i).unwrap());
            assert_eq!(_20, NumCast::from(20i8).unwrap());
            assert_eq!(_20, NumCast::from(20i16).unwrap());
            assert_eq!(_20, NumCast::from(20i32).unwrap());
            assert_eq!(_20, NumCast::from(20i64).unwrap());
            assert_eq!(_20, NumCast::from(20f32).unwrap());
            assert_eq!(_20, NumCast::from(20f64).unwrap());

            assert_eq!(_20, cast(20u).unwrap());
            assert_eq!(_20, cast(20u8).unwrap());
            assert_eq!(_20, cast(20u16).unwrap());
            assert_eq!(_20, cast(20u32).unwrap());
            assert_eq!(_20, cast(20u64).unwrap());
            assert_eq!(_20, cast(20i).unwrap());
            assert_eq!(_20, cast(20i8).unwrap());
            assert_eq!(_20, cast(20i16).unwrap());
            assert_eq!(_20, cast(20i32).unwrap());
            assert_eq!(_20, cast(20i64).unwrap());
            assert_eq!(_20, cast(20f32).unwrap());
            assert_eq!(_20, cast(20f64).unwrap());
        })
    }

    #[test] fn test_u8_cast()    { test_cast_20!(20u8)  }
    #[test] fn test_u16_cast()   { test_cast_20!(20u16) }
    #[test] fn test_u32_cast()   { test_cast_20!(20u32) }
    #[test] fn test_u64_cast()   { test_cast_20!(20u64) }
    #[test] fn test_uint_cast()  { test_cast_20!(20u)   }
    #[test] fn test_i8_cast()    { test_cast_20!(20i8)  }
    #[test] fn test_i16_cast()   { test_cast_20!(20i16) }
    #[test] fn test_i32_cast()   { test_cast_20!(20i32) }
    #[test] fn test_i64_cast()   { test_cast_20!(20i64) }
    #[test] fn test_int_cast()   { test_cast_20!(20i)   }
    #[test] fn test_f32_cast()   { test_cast_20!(20f32) }
    #[test] fn test_f64_cast()   { test_cast_20!(20f64) }

    #[test]
    fn test_cast_range_int_min() {
        assert_eq!(int::MIN.to_int(),  Some(int::MIN as int));
        assert_eq!(int::MIN.to_i8(),   None);
        assert_eq!(int::MIN.to_i16(),  None);
        // int::MIN.to_i32() is word-size specific
        assert_eq!(int::MIN.to_i64(),  Some(int::MIN as i64));
        assert_eq!(int::MIN.to_uint(), None);
        assert_eq!(int::MIN.to_u8(),   None);
        assert_eq!(int::MIN.to_u16(),  None);
        assert_eq!(int::MIN.to_u32(),  None);
        assert_eq!(int::MIN.to_u64(),  None);

        #[cfg(target_word_size = "32")]
        fn check_word_size() {
            assert_eq!(int::MIN.to_i32(), Some(int::MIN as i32));
        }

        #[cfg(target_word_size = "64")]
        fn check_word_size() {
            assert_eq!(int::MIN.to_i32(), None);
        }

        check_word_size();
    }

    #[test]
    fn test_cast_range_i8_min() {
        assert_eq!(i8::MIN.to_int(),  Some(i8::MIN as int));
        assert_eq!(i8::MIN.to_i8(),   Some(i8::MIN as i8));
        assert_eq!(i8::MIN.to_i16(),  Some(i8::MIN as i16));
        assert_eq!(i8::MIN.to_i32(),  Some(i8::MIN as i32));
        assert_eq!(i8::MIN.to_i64(),  Some(i8::MIN as i64));
        assert_eq!(i8::MIN.to_uint(), None);
        assert_eq!(i8::MIN.to_u8(),   None);
        assert_eq!(i8::MIN.to_u16(),  None);
        assert_eq!(i8::MIN.to_u32(),  None);
        assert_eq!(i8::MIN.to_u64(),  None);
    }

    #[test]
    fn test_cast_range_i16_min() {
        assert_eq!(i16::MIN.to_int(),  Some(i16::MIN as int));
        assert_eq!(i16::MIN.to_i8(),   None);
        assert_eq!(i16::MIN.to_i16(),  Some(i16::MIN as i16));
        assert_eq!(i16::MIN.to_i32(),  Some(i16::MIN as i32));
        assert_eq!(i16::MIN.to_i64(),  Some(i16::MIN as i64));
        assert_eq!(i16::MIN.to_uint(), None);
        assert_eq!(i16::MIN.to_u8(),   None);
        assert_eq!(i16::MIN.to_u16(),  None);
        assert_eq!(i16::MIN.to_u32(),  None);
        assert_eq!(i16::MIN.to_u64(),  None);
    }

    #[test]
    fn test_cast_range_i32_min() {
        assert_eq!(i32::MIN.to_int(),  Some(i32::MIN as int));
        assert_eq!(i32::MIN.to_i8(),   None);
        assert_eq!(i32::MIN.to_i16(),  None);
        assert_eq!(i32::MIN.to_i32(),  Some(i32::MIN as i32));
        assert_eq!(i32::MIN.to_i64(),  Some(i32::MIN as i64));
        assert_eq!(i32::MIN.to_uint(), None);
        assert_eq!(i32::MIN.to_u8(),   None);
        assert_eq!(i32::MIN.to_u16(),  None);
        assert_eq!(i32::MIN.to_u32(),  None);
        assert_eq!(i32::MIN.to_u64(),  None);
    }

    #[test]
    fn test_cast_range_i64_min() {
        // i64::MIN.to_int() is word-size specific
        assert_eq!(i64::MIN.to_i8(),   None);
        assert_eq!(i64::MIN.to_i16(),  None);
        assert_eq!(i64::MIN.to_i32(),  None);
        assert_eq!(i64::MIN.to_i64(),  Some(i64::MIN as i64));
        assert_eq!(i64::MIN.to_uint(), None);
        assert_eq!(i64::MIN.to_u8(),   None);
        assert_eq!(i64::MIN.to_u16(),  None);
        assert_eq!(i64::MIN.to_u32(),  None);
        assert_eq!(i64::MIN.to_u64(),  None);

        #[cfg(target_word_size = "32")]
        fn check_word_size() {
            assert_eq!(i64::MIN.to_int(), None);
        }

        #[cfg(target_word_size = "64")]
        fn check_word_size() {
            assert_eq!(i64::MIN.to_int(), Some(i64::MIN as int));
        }

        check_word_size();
    }

    #[test]
    fn test_cast_range_int_max() {
        assert_eq!(int::MAX.to_int(),  Some(int::MAX as int));
        assert_eq!(int::MAX.to_i8(),   None);
        assert_eq!(int::MAX.to_i16(),  None);
        // int::MAX.to_i32() is word-size specific
        assert_eq!(int::MAX.to_i64(),  Some(int::MAX as i64));
        assert_eq!(int::MAX.to_u8(),   None);
        assert_eq!(int::MAX.to_u16(),  None);
        // int::MAX.to_u32() is word-size specific
        assert_eq!(int::MAX.to_u64(),  Some(int::MAX as u64));

        #[cfg(target_word_size = "32")]
        fn check_word_size() {
            assert_eq!(int::MAX.to_i32(), Some(int::MAX as i32));
            assert_eq!(int::MAX.to_u32(), Some(int::MAX as u32));
        }

        #[cfg(target_word_size = "64")]
        fn check_word_size() {
            assert_eq!(int::MAX.to_i32(), None);
            assert_eq!(int::MAX.to_u32(), None);
        }

        check_word_size();
    }

    #[test]
    fn test_cast_range_i8_max() {
        assert_eq!(i8::MAX.to_int(),  Some(i8::MAX as int));
        assert_eq!(i8::MAX.to_i8(),   Some(i8::MAX as i8));
        assert_eq!(i8::MAX.to_i16(),  Some(i8::MAX as i16));
        assert_eq!(i8::MAX.to_i32(),  Some(i8::MAX as i32));
        assert_eq!(i8::MAX.to_i64(),  Some(i8::MAX as i64));
        assert_eq!(i8::MAX.to_uint(), Some(i8::MAX as uint));
        assert_eq!(i8::MAX.to_u8(),   Some(i8::MAX as u8));
        assert_eq!(i8::MAX.to_u16(),  Some(i8::MAX as u16));
        assert_eq!(i8::MAX.to_u32(),  Some(i8::MAX as u32));
        assert_eq!(i8::MAX.to_u64(),  Some(i8::MAX as u64));
    }

    #[test]
    fn test_cast_range_i16_max() {
        assert_eq!(i16::MAX.to_int(),  Some(i16::MAX as int));
        assert_eq!(i16::MAX.to_i8(),   None);
        assert_eq!(i16::MAX.to_i16(),  Some(i16::MAX as i16));
        assert_eq!(i16::MAX.to_i32(),  Some(i16::MAX as i32));
        assert_eq!(i16::MAX.to_i64(),  Some(i16::MAX as i64));
        assert_eq!(i16::MAX.to_uint(), Some(i16::MAX as uint));
        assert_eq!(i16::MAX.to_u8(),   None);
        assert_eq!(i16::MAX.to_u16(),  Some(i16::MAX as u16));
        assert_eq!(i16::MAX.to_u32(),  Some(i16::MAX as u32));
        assert_eq!(i16::MAX.to_u64(),  Some(i16::MAX as u64));
    }

    #[test]
    fn test_cast_range_i32_max() {
        assert_eq!(i32::MAX.to_int(),  Some(i32::MAX as int));
        assert_eq!(i32::MAX.to_i8(),   None);
        assert_eq!(i32::MAX.to_i16(),  None);
        assert_eq!(i32::MAX.to_i32(),  Some(i32::MAX as i32));
        assert_eq!(i32::MAX.to_i64(),  Some(i32::MAX as i64));
        assert_eq!(i32::MAX.to_uint(), Some(i32::MAX as uint));
        assert_eq!(i32::MAX.to_u8(),   None);
        assert_eq!(i32::MAX.to_u16(),  None);
        assert_eq!(i32::MAX.to_u32(),  Some(i32::MAX as u32));
        assert_eq!(i32::MAX.to_u64(),  Some(i32::MAX as u64));
    }

    #[test]
    fn test_cast_range_i64_max() {
        // i64::MAX.to_int() is word-size specific
        assert_eq!(i64::MAX.to_i8(),   None);
        assert_eq!(i64::MAX.to_i16(),  None);
        assert_eq!(i64::MAX.to_i32(),  None);
        assert_eq!(i64::MAX.to_i64(),  Some(i64::MAX as i64));
        // i64::MAX.to_uint() is word-size specific
        assert_eq!(i64::MAX.to_u8(),   None);
        assert_eq!(i64::MAX.to_u16(),  None);
        assert_eq!(i64::MAX.to_u32(),  None);
        assert_eq!(i64::MAX.to_u64(),  Some(i64::MAX as u64));

        #[cfg(target_word_size = "32")]
        fn check_word_size() {
            assert_eq!(i64::MAX.to_int(),  None);
            assert_eq!(i64::MAX.to_uint(), None);
        }

        #[cfg(target_word_size = "64")]
        fn check_word_size() {
            assert_eq!(i64::MAX.to_int(),  Some(i64::MAX as int));
            assert_eq!(i64::MAX.to_uint(), Some(i64::MAX as uint));
        }

        check_word_size();
    }

    #[test]
    fn test_cast_range_uint_min() {
        assert_eq!(uint::MIN.to_int(),  Some(uint::MIN as int));
        assert_eq!(uint::MIN.to_i8(),   Some(uint::MIN as i8));
        assert_eq!(uint::MIN.to_i16(),  Some(uint::MIN as i16));
        assert_eq!(uint::MIN.to_i32(),  Some(uint::MIN as i32));
        assert_eq!(uint::MIN.to_i64(),  Some(uint::MIN as i64));
        assert_eq!(uint::MIN.to_uint(), Some(uint::MIN as uint));
        assert_eq!(uint::MIN.to_u8(),   Some(uint::MIN as u8));
        assert_eq!(uint::MIN.to_u16(),  Some(uint::MIN as u16));
        assert_eq!(uint::MIN.to_u32(),  Some(uint::MIN as u32));
        assert_eq!(uint::MIN.to_u64(),  Some(uint::MIN as u64));
    }

    #[test]
    fn test_cast_range_u8_min() {
        assert_eq!(u8::MIN.to_int(),  Some(u8::MIN as int));
        assert_eq!(u8::MIN.to_i8(),   Some(u8::MIN as i8));
        assert_eq!(u8::MIN.to_i16(),  Some(u8::MIN as i16));
        assert_eq!(u8::MIN.to_i32(),  Some(u8::MIN as i32));
        assert_eq!(u8::MIN.to_i64(),  Some(u8::MIN as i64));
        assert_eq!(u8::MIN.to_uint(), Some(u8::MIN as uint));
        assert_eq!(u8::MIN.to_u8(),   Some(u8::MIN as u8));
        assert_eq!(u8::MIN.to_u16(),  Some(u8::MIN as u16));
        assert_eq!(u8::MIN.to_u32(),  Some(u8::MIN as u32));
        assert_eq!(u8::MIN.to_u64(),  Some(u8::MIN as u64));
    }

    #[test]
    fn test_cast_range_u16_min() {
        assert_eq!(u16::MIN.to_int(),  Some(u16::MIN as int));
        assert_eq!(u16::MIN.to_i8(),   Some(u16::MIN as i8));
        assert_eq!(u16::MIN.to_i16(),  Some(u16::MIN as i16));
        assert_eq!(u16::MIN.to_i32(),  Some(u16::MIN as i32));
        assert_eq!(u16::MIN.to_i64(),  Some(u16::MIN as i64));
        assert_eq!(u16::MIN.to_uint(), Some(u16::MIN as uint));
        assert_eq!(u16::MIN.to_u8(),   Some(u16::MIN as u8));
        assert_eq!(u16::MIN.to_u16(),  Some(u16::MIN as u16));
        assert_eq!(u16::MIN.to_u32(),  Some(u16::MIN as u32));
        assert_eq!(u16::MIN.to_u64(),  Some(u16::MIN as u64));
    }

    #[test]
    fn test_cast_range_u32_min() {
        assert_eq!(u32::MIN.to_int(),  Some(u32::MIN as int));
        assert_eq!(u32::MIN.to_i8(),   Some(u32::MIN as i8));
        assert_eq!(u32::MIN.to_i16(),  Some(u32::MIN as i16));
        assert_eq!(u32::MIN.to_i32(),  Some(u32::MIN as i32));
        assert_eq!(u32::MIN.to_i64(),  Some(u32::MIN as i64));
        assert_eq!(u32::MIN.to_uint(), Some(u32::MIN as uint));
        assert_eq!(u32::MIN.to_u8(),   Some(u32::MIN as u8));
        assert_eq!(u32::MIN.to_u16(),  Some(u32::MIN as u16));
        assert_eq!(u32::MIN.to_u32(),  Some(u32::MIN as u32));
        assert_eq!(u32::MIN.to_u64(),  Some(u32::MIN as u64));
    }

    #[test]
    fn test_cast_range_u64_min() {
        assert_eq!(u64::MIN.to_int(),  Some(u64::MIN as int));
        assert_eq!(u64::MIN.to_i8(),   Some(u64::MIN as i8));
        assert_eq!(u64::MIN.to_i16(),  Some(u64::MIN as i16));
        assert_eq!(u64::MIN.to_i32(),  Some(u64::MIN as i32));
        assert_eq!(u64::MIN.to_i64(),  Some(u64::MIN as i64));
        assert_eq!(u64::MIN.to_uint(), Some(u64::MIN as uint));
        assert_eq!(u64::MIN.to_u8(),   Some(u64::MIN as u8));
        assert_eq!(u64::MIN.to_u16(),  Some(u64::MIN as u16));
        assert_eq!(u64::MIN.to_u32(),  Some(u64::MIN as u32));
        assert_eq!(u64::MIN.to_u64(),  Some(u64::MIN as u64));
    }

    #[test]
    fn test_cast_range_uint_max() {
        assert_eq!(uint::MAX.to_int(),  None);
        assert_eq!(uint::MAX.to_i8(),   None);
        assert_eq!(uint::MAX.to_i16(),  None);
        assert_eq!(uint::MAX.to_i32(),  None);
        // uint::MAX.to_i64() is word-size specific
        assert_eq!(uint::MAX.to_u8(),   None);
        assert_eq!(uint::MAX.to_u16(),  None);
        // uint::MAX.to_u32() is word-size specific
        assert_eq!(uint::MAX.to_u64(),  Some(uint::MAX as u64));

        #[cfg(target_word_size = "32")]
        fn check_word_size() {
            assert_eq!(uint::MAX.to_u32(), Some(uint::MAX as u32));
            assert_eq!(uint::MAX.to_i64(), Some(uint::MAX as i64));
        }

        #[cfg(target_word_size = "64")]
        fn check_word_size() {
            assert_eq!(uint::MAX.to_u32(), None);
            assert_eq!(uint::MAX.to_i64(), None);
        }

        check_word_size();
    }

    #[test]
    fn test_cast_range_u8_max() {
        assert_eq!(u8::MAX.to_int(),  Some(u8::MAX as int));
        assert_eq!(u8::MAX.to_i8(),   None);
        assert_eq!(u8::MAX.to_i16(),  Some(u8::MAX as i16));
        assert_eq!(u8::MAX.to_i32(),  Some(u8::MAX as i32));
        assert_eq!(u8::MAX.to_i64(),  Some(u8::MAX as i64));
        assert_eq!(u8::MAX.to_uint(), Some(u8::MAX as uint));
        assert_eq!(u8::MAX.to_u8(),   Some(u8::MAX as u8));
        assert_eq!(u8::MAX.to_u16(),  Some(u8::MAX as u16));
        assert_eq!(u8::MAX.to_u32(),  Some(u8::MAX as u32));
        assert_eq!(u8::MAX.to_u64(),  Some(u8::MAX as u64));
    }

    #[test]
    fn test_cast_range_u16_max() {
        assert_eq!(u16::MAX.to_int(),  Some(u16::MAX as int));
        assert_eq!(u16::MAX.to_i8(),   None);
        assert_eq!(u16::MAX.to_i16(),  None);
        assert_eq!(u16::MAX.to_i32(),  Some(u16::MAX as i32));
        assert_eq!(u16::MAX.to_i64(),  Some(u16::MAX as i64));
        assert_eq!(u16::MAX.to_uint(), Some(u16::MAX as uint));
        assert_eq!(u16::MAX.to_u8(),   None);
        assert_eq!(u16::MAX.to_u16(),  Some(u16::MAX as u16));
        assert_eq!(u16::MAX.to_u32(),  Some(u16::MAX as u32));
        assert_eq!(u16::MAX.to_u64(),  Some(u16::MAX as u64));
    }

    #[test]
    fn test_cast_range_u32_max() {
        // u32::MAX.to_int() is word-size specific
        assert_eq!(u32::MAX.to_i8(),   None);
        assert_eq!(u32::MAX.to_i16(),  None);
        assert_eq!(u32::MAX.to_i32(),  None);
        assert_eq!(u32::MAX.to_i64(),  Some(u32::MAX as i64));
        assert_eq!(u32::MAX.to_uint(), Some(u32::MAX as uint));
        assert_eq!(u32::MAX.to_u8(),   None);
        assert_eq!(u32::MAX.to_u16(),  None);
        assert_eq!(u32::MAX.to_u32(),  Some(u32::MAX as u32));
        assert_eq!(u32::MAX.to_u64(),  Some(u32::MAX as u64));

        #[cfg(target_word_size = "32")]
        fn check_word_size() {
            assert_eq!(u32::MAX.to_int(),  None);
        }

        #[cfg(target_word_size = "64")]
        fn check_word_size() {
            assert_eq!(u32::MAX.to_int(),  Some(u32::MAX as int));
        }

        check_word_size();
    }

    #[test]
    fn test_cast_range_u64_max() {
        assert_eq!(u64::MAX.to_int(),  None);
        assert_eq!(u64::MAX.to_i8(),   None);
        assert_eq!(u64::MAX.to_i16(),  None);
        assert_eq!(u64::MAX.to_i32(),  None);
        assert_eq!(u64::MAX.to_i64(),  None);
        // u64::MAX.to_uint() is word-size specific
        assert_eq!(u64::MAX.to_u8(),   None);
        assert_eq!(u64::MAX.to_u16(),  None);
        assert_eq!(u64::MAX.to_u32(),  None);
        assert_eq!(u64::MAX.to_u64(),  Some(u64::MAX as u64));

        #[cfg(target_word_size = "32")]
        fn check_word_size() {
            assert_eq!(u64::MAX.to_uint(), None);
        }

        #[cfg(target_word_size = "64")]
        fn check_word_size() {
            assert_eq!(u64::MAX.to_uint(), Some(u64::MAX as uint));
        }

        check_word_size();
    }

    #[test]
    fn test_saturating_add_uint() {
        use uint::MAX;
        assert_eq!(3u.saturating_add(5u), 8u);
        assert_eq!(3u.saturating_add(MAX-1), MAX);
        assert_eq!(MAX.saturating_add(MAX), MAX);
        assert_eq!((MAX-2).saturating_add(1), MAX-1);
    }

    #[test]
    fn test_saturating_sub_uint() {
        use uint::MAX;
        assert_eq!(5u.saturating_sub(3u), 2u);
        assert_eq!(3u.saturating_sub(5u), 0u);
        assert_eq!(0u.saturating_sub(1u), 0u);
        assert_eq!((MAX-1).saturating_sub(MAX), 0);
    }

    #[test]
    fn test_saturating_add_int() {
        use int::{MIN,MAX};
        assert_eq!(3i.saturating_add(5i), 8i);
        assert_eq!(3i.saturating_add(MAX-1), MAX);
        assert_eq!(MAX.saturating_add(MAX), MAX);
        assert_eq!((MAX-2).saturating_add(1), MAX-1);
        assert_eq!(3i.saturating_add(-5i), -2i);
        assert_eq!(MIN.saturating_add(-1i), MIN);
        assert_eq!((-2i).saturating_add(-MAX), MIN);
    }

    #[test]
    fn test_saturating_sub_int() {
        use int::{MIN,MAX};
        assert_eq!(3i.saturating_sub(5i), -2i);
        assert_eq!(MIN.saturating_sub(1i), MIN);
        assert_eq!((-2i).saturating_sub(MAX), MIN);
        assert_eq!(3i.saturating_sub(-5i), 8i);
        assert_eq!(3i.saturating_sub(-(MAX-1)), MAX);
        assert_eq!(MAX.saturating_sub(-MAX), MAX);
        assert_eq!((MAX-2).saturating_sub(-1), MAX-1);
    }

    #[test]
    fn test_checked_add() {
        let five_less = uint::MAX - 5;
        assert_eq!(five_less.checked_add(0), Some(uint::MAX - 5));
        assert_eq!(five_less.checked_add(1), Some(uint::MAX - 4));
        assert_eq!(five_less.checked_add(2), Some(uint::MAX - 3));
        assert_eq!(five_less.checked_add(3), Some(uint::MAX - 2));
        assert_eq!(five_less.checked_add(4), Some(uint::MAX - 1));
        assert_eq!(five_less.checked_add(5), Some(uint::MAX));
        assert_eq!(five_less.checked_add(6), None);
        assert_eq!(five_less.checked_add(7), None);
    }

    #[test]
    fn test_checked_sub() {
        assert_eq!(5u.checked_sub(0), Some(5));
        assert_eq!(5u.checked_sub(1), Some(4));
        assert_eq!(5u.checked_sub(2), Some(3));
        assert_eq!(5u.checked_sub(3), Some(2));
        assert_eq!(5u.checked_sub(4), Some(1));
        assert_eq!(5u.checked_sub(5), Some(0));
        assert_eq!(5u.checked_sub(6), None);
        assert_eq!(5u.checked_sub(7), None);
    }

    #[test]
    fn test_checked_mul() {
        let third = uint::MAX / 3;
        assert_eq!(third.checked_mul(0), Some(0));
        assert_eq!(third.checked_mul(1), Some(third));
        assert_eq!(third.checked_mul(2), Some(third * 2));
        assert_eq!(third.checked_mul(3), Some(third * 3));
        assert_eq!(third.checked_mul(4), None);
    }

    macro_rules! test_is_power_of_two {
        ($test_name:ident, $T:ident) => (
            fn $test_name() {
                #![test]
                assert_eq!((0 as $T).is_power_of_two(), false);
                assert_eq!((1 as $T).is_power_of_two(), true);
                assert_eq!((2 as $T).is_power_of_two(), true);
                assert_eq!((3 as $T).is_power_of_two(), false);
                assert_eq!((4 as $T).is_power_of_two(), true);
                assert_eq!((5 as $T).is_power_of_two(), false);
                assert!(($T::MAX / 2 + 1).is_power_of_two(), true);
            }
        )
    }

    test_is_power_of_two!{ test_is_power_of_two_u8, u8 }
    test_is_power_of_two!{ test_is_power_of_two_u16, u16 }
    test_is_power_of_two!{ test_is_power_of_two_u32, u32 }
    test_is_power_of_two!{ test_is_power_of_two_u64, u64 }
    test_is_power_of_two!{ test_is_power_of_two_uint, uint }

    macro_rules! test_next_power_of_two {
        ($test_name:ident, $T:ident) => (
            fn $test_name() {
                #![test]
                assert_eq!((0 as $T).next_power_of_two(), 1);
                let mut next_power = 1;
                for i in range::<$T>(1, 40) {
                     assert_eq!(i.next_power_of_two(), next_power);
                     if i == next_power { next_power *= 2 }
                }
            }
        )
    }

    test_next_power_of_two! { test_next_power_of_two_u8, u8 }
    test_next_power_of_two! { test_next_power_of_two_u16, u16 }
    test_next_power_of_two! { test_next_power_of_two_u32, u32 }
    test_next_power_of_two! { test_next_power_of_two_u64, u64 }
    test_next_power_of_two! { test_next_power_of_two_uint, uint }

    macro_rules! test_checked_next_power_of_two {
        ($test_name:ident, $T:ident) => (
            fn $test_name() {
                #![test]
                assert_eq!((0 as $T).checked_next_power_of_two(), Some(1));
                assert!(($T::MAX / 2).checked_next_power_of_two().is_some());
                assert_eq!(($T::MAX - 1).checked_next_power_of_two(), None);
                assert_eq!($T::MAX.checked_next_power_of_two(), None);
                let mut next_power = 1;
                for i in range::<$T>(1, 40) {
                     assert_eq!(i.checked_next_power_of_two(), Some(next_power));
                     if i == next_power { next_power *= 2 }
                }
            }
        )
    }

    test_checked_next_power_of_two! { test_checked_next_power_of_two_u8, u8 }
    test_checked_next_power_of_two! { test_checked_next_power_of_two_u16, u16 }
    test_checked_next_power_of_two! { test_checked_next_power_of_two_u32, u32 }
    test_checked_next_power_of_two! { test_checked_next_power_of_two_u64, u64 }
    test_checked_next_power_of_two! { test_checked_next_power_of_two_uint, uint }

    #[deriving(PartialEq, Show)]
    struct Value { x: int }

    impl ToPrimitive for Value {
        fn to_i64(&self) -> Option<i64> { self.x.to_i64() }
        fn to_u64(&self) -> Option<u64> { self.x.to_u64() }
    }

    impl FromPrimitive for Value {
        fn from_i64(n: i64) -> Option<Value> { Some(Value { x: n as int }) }
        fn from_u64(n: u64) -> Option<Value> { Some(Value { x: n as int }) }
    }

    #[test]
    fn test_to_primitive() {
        let value = Value { x: 5 };
        assert_eq!(value.to_int(),  Some(5));
        assert_eq!(value.to_i8(),   Some(5));
        assert_eq!(value.to_i16(),  Some(5));
        assert_eq!(value.to_i32(),  Some(5));
        assert_eq!(value.to_i64(),  Some(5));
        assert_eq!(value.to_uint(), Some(5));
        assert_eq!(value.to_u8(),   Some(5));
        assert_eq!(value.to_u16(),  Some(5));
        assert_eq!(value.to_u32(),  Some(5));
        assert_eq!(value.to_u64(),  Some(5));
        assert_eq!(value.to_f32(),  Some(5f32));
        assert_eq!(value.to_f64(),  Some(5f64));
    }

    #[test]
    fn test_from_primitive() {
        assert_eq!(from_int(5),    Some(Value { x: 5 }));
        assert_eq!(from_i8(5),     Some(Value { x: 5 }));
        assert_eq!(from_i16(5),    Some(Value { x: 5 }));
        assert_eq!(from_i32(5),    Some(Value { x: 5 }));
        assert_eq!(from_i64(5),    Some(Value { x: 5 }));
        assert_eq!(from_uint(5),   Some(Value { x: 5 }));
        assert_eq!(from_u8(5),     Some(Value { x: 5 }));
        assert_eq!(from_u16(5),    Some(Value { x: 5 }));
        assert_eq!(from_u32(5),    Some(Value { x: 5 }));
        assert_eq!(from_u64(5),    Some(Value { x: 5 }));
        assert_eq!(from_f32(5f32), Some(Value { x: 5 }));
        assert_eq!(from_f64(5f64), Some(Value { x: 5 }));
    }

    #[test]
    fn test_pow() {
        fn naive_pow<T: Int>(base: T, exp: uint) -> T {
            let one: T = Int::one();
            range(0, exp).fold(one, |acc, _| acc * base)
        }
        macro_rules! assert_pow {
            (($num:expr, $exp:expr) => $expected:expr) => {{
                let result = $num.pow($exp);
                assert_eq!(result, $expected);
                assert_eq!(result, naive_pow($num, $exp));
            }}
        }
        assert_pow!((3i,     0 ) => 1);
        assert_pow!((5i,     1 ) => 5);
        assert_pow!((-4i,    2 ) => 16);
        assert_pow!((8i,     3 ) => 512);
        assert_pow!((2u64,   50) => 1125899906842624);
    }
}


#[cfg(test)]
mod bench {
    extern crate test;
    use self::test::Bencher;
    use num::Int;
    use prelude::*;

    #[bench]
    fn bench_pow_function(b: &mut Bencher) {
        let v = Vec::from_fn(1024u, |n| n);
        b.iter(|| {v.iter().fold(0u, |old, new| old.pow(*new));});
    }
}
