// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


//! Complex numbers.

use core::num::{Zero,One,ToStrRadix};
use core::prelude::*;

// FIXME #1284: handle complex NaN & infinity etc. This
// probably doesn't map to C's _Complex correctly.

// FIXME #5734:: Need generic sin/cos for .to/from_polar().
// FIXME #5735: Need generic sqrt to implement .norm().


/// A complex number in Cartesian form.
#[deriving(Eq,Clone)]
pub struct Cmplx<T> {
    re: T,
    im: T
}

pub type Complex = Cmplx<float>;
pub type Complex32 = Cmplx<f32>;
pub type Complex64 = Cmplx<f64>;

impl<T: Copy + Num> Cmplx<T> {
    /// Create a new Cmplx
    #[inline]
    pub fn new(re: T, im: T) -> Cmplx<T> {
        Cmplx { re: re, im: im }
    }

    /**
    Returns the square of the norm (since `T` doesn't necessarily
    have a sqrt function), i.e. `re^2 + im^2`.
    */
    #[inline]
    pub fn norm_sqr(&self) -> T {
        self.re * self.re + self.im * self.im
    }


    /// Returns the complex conjugate. i.e. `re - i im`
    #[inline]
    pub fn conj(&self) -> Cmplx<T> {
        Cmplx::new(self.re, -self.im)
    }


    /// Multiplies `self` by the scalar `t`.
    #[inline]
    pub fn scale(&self, t: T) -> Cmplx<T> {
        Cmplx::new(self.re * t, self.im * t)
    }

    /// Divides `self` by the scalar `t`.
    #[inline]
    pub fn unscale(&self, t: T) -> Cmplx<T> {
        Cmplx::new(self.re / t, self.im / t)
    }

    /// Returns `1/self`
    #[inline]
    pub fn inv(&self) -> Cmplx<T> {
        let norm_sqr = self.norm_sqr();
        Cmplx::new(self.re / norm_sqr,
                    -self.im / norm_sqr)
    }
}

/* arithmetic */
// (a + i b) + (c + i d) == (a + c) + i (b + d)
impl<T: Copy + Num> Add<Cmplx<T>, Cmplx<T>> for Cmplx<T> {
    #[inline]
    fn add(&self, other: &Cmplx<T>) -> Cmplx<T> {
        Cmplx::new(self.re + other.re, self.im + other.im)
    }
}
// (a + i b) - (c + i d) == (a - c) + i (b - d)
impl<T: Copy + Num> Sub<Cmplx<T>, Cmplx<T>> for Cmplx<T> {
    #[inline]
    fn sub(&self, other: &Cmplx<T>) -> Cmplx<T> {
        Cmplx::new(self.re - other.re, self.im - other.im)
    }
}
// (a + i b) * (c + i d) == (a*c - b*d) + i (a*d + b*c)
impl<T: Copy + Num> Mul<Cmplx<T>, Cmplx<T>> for Cmplx<T> {
    #[inline]
    fn mul(&self, other: &Cmplx<T>) -> Cmplx<T> {
        Cmplx::new(self.re*other.re - self.im*other.im,
                     self.re*other.im + self.im*other.re)
    }
}

// (a + i b) / (c + i d) == [(a + i b) * (c - i d)] / (c*c + d*d)
//   == [(a*c + b*d) / (c*c + d*d)] + i [(b*c - a*d) / (c*c + d*d)]
impl<T: Copy + Num> Quot<Cmplx<T>, Cmplx<T>> for Cmplx<T> {
    #[inline]
    fn quot(&self, other: &Cmplx<T>) -> Cmplx<T> {
        let norm_sqr = other.norm_sqr();
        Cmplx::new((self.re*other.re + self.im*other.im) / norm_sqr,
                     (self.im*other.re - self.re*other.im) / norm_sqr)
    }
}

impl<T: Copy + Num> Neg<Cmplx<T>> for Cmplx<T> {
    #[inline]
    fn neg(&self) -> Cmplx<T> {
        Cmplx::new(-self.re, -self.im)
    }
}

/* constants */
impl<T: Copy + Num> Zero for Cmplx<T> {
    #[inline]
    fn zero() -> Cmplx<T> {
        Cmplx::new(Zero::zero(), Zero::zero())
    }

    #[inline]
    fn is_zero(&self) -> bool {
        *self == Zero::zero()
    }
}

impl<T: Copy + Num> One for Cmplx<T> {
    #[inline]
    fn one() -> Cmplx<T> {
        Cmplx::new(One::one(), Zero::zero())
    }
}

/* string conversions */
impl<T: ToStr + Num + Ord> ToStr for Cmplx<T> {
    fn to_str(&self) -> ~str {
        if self.im < Zero::zero() {
            fmt!("%s-%si", self.re.to_str(), (-self.im).to_str())
        } else {
            fmt!("%s+%si", self.re.to_str(), self.im.to_str())
        }
    }
}

impl<T: ToStrRadix + Num + Ord> ToStrRadix for Cmplx<T> {
    fn to_str_radix(&self, radix: uint) -> ~str {
        if self.im < Zero::zero() {
            fmt!("%s-%si", self.re.to_str_radix(radix), (-self.im).to_str_radix(radix))
        } else {
            fmt!("%s+%si", self.re.to_str_radix(radix), self.im.to_str_radix(radix))
        }
    }
}

#[cfg(test)]
mod test {
    use core::prelude::*;
    use super::*;
    use core::num::{Zero,One};

    pub static _0_0i : Complex = Cmplx { re: 0f, im: 0f };
    pub static _1_0i : Complex = Cmplx { re: 1f, im: 0f };
    pub static _1_1i : Complex = Cmplx { re: 1f, im: 1f };
    pub static _0_1i : Complex = Cmplx { re: 0f, im: 1f };
    pub static _neg1_1i : Complex = Cmplx { re: -1f, im: 1f };
    pub static _05_05i : Complex = Cmplx { re: 0.5f, im: 0.5f };
    pub static all_consts : [Complex, .. 5] = [_0_0i, _1_0i, _1_1i, _neg1_1i, _05_05i];

    #[test]
    fn test_consts() {
        // check our constants are what Cmplx::new creates
        fn test(c : Complex, r : float, i: float) {
            assert_eq!(c, Cmplx::new(r,i));
        }
        test(_0_0i, 0f, 0f);
        test(_1_0i, 1f, 0f);
        test(_1_1i, 1f, 1f);
        test(_neg1_1i, -1f, 1f);
        test(_05_05i, 0.5f, 0.5f);

        assert_eq!(_0_0i, Zero::zero());
        assert_eq!(_1_0i, One::one());
    }

    #[test]
    fn test_norm_sqr() {
        fn test(c: Complex, ns: float) {
            assert_eq!(c.norm_sqr(), ns);
        }
        test(_0_0i, 0f);
        test(_1_0i, 1f);
        test(_1_1i, 2f);
        test(_neg1_1i, 2f);
        test(_05_05i, 0.5f);
    }

    #[test]
    fn test_scale_unscale() {
        assert_eq!(_05_05i.scale(2f), _1_1i);
        assert_eq!(_1_1i.unscale(2f), _05_05i);
        for all_consts.each |&c| {
            assert_eq!(c.scale(2f).unscale(2f), c);
        }
    }

    #[test]
    fn test_conj() {
        for all_consts.each |&c| {
            assert_eq!(c.conj(), Cmplx::new(c.re, -c.im));
            assert_eq!(c.conj().conj(), c);
        }
    }

    #[test]
    fn test_inv() {
        assert_eq!(_1_1i.inv(), _05_05i.conj());
        assert_eq!(_1_0i.inv(), _1_0i.inv());
    }

    #[test]
    #[should_fail]
    #[ignore]
    fn test_inv_zero() {
        // FIXME #5736: should this really fail, or just NaN?
        _0_0i.inv();
    }


    mod arith {
        use super::*;
        use core::num::Zero;

        #[test]
        fn test_add() {
            assert_eq!(_05_05i + _05_05i, _1_1i);
            assert_eq!(_0_1i + _1_0i, _1_1i);
            assert_eq!(_1_0i + _neg1_1i, _0_1i);

            for all_consts.each |&c| {
                assert_eq!(_0_0i + c, c);
                assert_eq!(c + _0_0i, c);
            }
        }

        #[test]
        fn test_sub() {
            assert_eq!(_05_05i - _05_05i, _0_0i);
            assert_eq!(_0_1i - _1_0i, _neg1_1i);
            assert_eq!(_0_1i - _neg1_1i, _1_0i);

            for all_consts.each |&c| {
                assert_eq!(c - _0_0i, c);
                assert_eq!(c - c, _0_0i);
            }
        }

        #[test]
        fn test_mul() {
            assert_eq!(_05_05i * _05_05i, _0_1i.unscale(2f));
            assert_eq!(_1_1i * _0_1i, _neg1_1i);

            // i^2 & i^4
            assert_eq!(_0_1i * _0_1i, -_1_0i);
            assert_eq!(_0_1i * _0_1i * _0_1i * _0_1i, _1_0i);

            for all_consts.each |&c| {
                assert_eq!(c * _1_0i, c);
                assert_eq!(_1_0i * c, c);
            }
        }
        #[test]
        fn test_quot() {
            assert_eq!(_neg1_1i / _0_1i, _1_1i);
            for all_consts.each |&c| {
                if c != Zero::zero() {
                    assert_eq!(c / c, _1_0i);
                }
            }
        }
        #[test]
        fn test_neg() {
            assert_eq!(-_1_0i + _0_1i, _neg1_1i);
            assert_eq!((-_0_1i) * _0_1i, _1_0i);
            for all_consts.each |&c| {
                assert_eq!(-(-c), c);
            }
        }
    }

    #[test]
    fn test_to_str() {
        fn test(c : Complex, s: ~str) {
            assert_eq!(c.to_str(), s);
        }
        test(_0_0i, ~"0+0i");
        test(_1_0i, ~"1+0i");
        test(_0_1i, ~"0+1i");
        test(_1_1i, ~"1+1i");
        test(_neg1_1i, ~"-1+1i");
        test(-_neg1_1i, ~"1-1i");
        test(_05_05i, ~"0.5+0.5i");
    }
}
