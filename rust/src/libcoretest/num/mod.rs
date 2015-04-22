// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::cmp::PartialEq;
use core::fmt::Debug;
use core::ops::{Add, Sub, Mul, Div, Rem};
use core::marker::Copy;

#[macro_use]
mod int_macros;

mod i8;
mod i16;
mod i32;
mod i64;

#[macro_use]
mod uint_macros;

mod u8;
mod u16;
mod u32;
mod u64;

/// Helper function for testing numeric operations
pub fn test_num<T>(ten: T, two: T) where
    T: PartialEq
     + Add<Output=T> + Sub<Output=T>
     + Mul<Output=T> + Div<Output=T>
     + Rem<Output=T> + Debug
     + Copy
{
    assert_eq!(ten.add(two),  ten + two);
    assert_eq!(ten.sub(two),  ten - two);
    assert_eq!(ten.mul(two),  ten * two);
    assert_eq!(ten.div(two),  ten / two);
    assert_eq!(ten.rem(two),  ten % two);
}

#[cfg(test)]
mod test {
    use core::option::Option;
    use core::option::Option::{Some, None};
    use core::num::Float;

    #[test]
    fn from_str_issue7588() {
        let u : Option<u8> = u8::from_str_radix("1000", 10).ok();
        assert_eq!(u, None);
        let s : Option<i16> = i16::from_str_radix("80000", 10).ok();
        assert_eq!(s, None);
        let s = "10000000000000000000000000000000000000000";
        let f : Option<f32> = f32::from_str_radix(s, 10).ok();
        assert_eq!(f, Some(Float::infinity()));
        let fe : Option<f32> = f32::from_str_radix("1e40", 10).ok();
        assert_eq!(fe, Some(Float::infinity()));
    }

    #[test]
    fn test_from_str_radix_float() {
        let x1 : Option<f64> = f64::from_str_radix("-123.456", 10).ok();
        assert_eq!(x1, Some(-123.456));
        let x2 : Option<f32> = f32::from_str_radix("123.456", 10).ok();
        assert_eq!(x2, Some(123.456));
        let x3 : Option<f32> = f32::from_str_radix("-0.0", 10).ok();
        assert_eq!(x3, Some(-0.0));
        let x4 : Option<f32> = f32::from_str_radix("0.0", 10).ok();
        assert_eq!(x4, Some(0.0));
        let x4 : Option<f32> = f32::from_str_radix("1.0", 10).ok();
        assert_eq!(x4, Some(1.0));
        let x5 : Option<f32> = f32::from_str_radix("-1.0", 10).ok();
        assert_eq!(x5, Some(-1.0));
    }

    #[test]
    fn test_int_from_str_overflow() {
        let mut i8_val: i8 = 127;
        assert_eq!("127".parse::<i8>().ok(), Some(i8_val));
        assert_eq!("128".parse::<i8>().ok(), None);

        i8_val = i8_val.wrapping_add(1);
        assert_eq!("-128".parse::<i8>().ok(), Some(i8_val));
        assert_eq!("-129".parse::<i8>().ok(), None);

        let mut i16_val: i16 = 32_767;
        assert_eq!("32767".parse::<i16>().ok(), Some(i16_val));
        assert_eq!("32768".parse::<i16>().ok(), None);

        i16_val = i16_val.wrapping_add(1);
        assert_eq!("-32768".parse::<i16>().ok(), Some(i16_val));
        assert_eq!("-32769".parse::<i16>().ok(), None);

        let mut i32_val: i32 = 2_147_483_647;
        assert_eq!("2147483647".parse::<i32>().ok(), Some(i32_val));
        assert_eq!("2147483648".parse::<i32>().ok(), None);

        i32_val = i32_val.wrapping_add(1);
        assert_eq!("-2147483648".parse::<i32>().ok(), Some(i32_val));
        assert_eq!("-2147483649".parse::<i32>().ok(), None);

        let mut i64_val: i64 = 9_223_372_036_854_775_807;
        assert_eq!("9223372036854775807".parse::<i64>().ok(), Some(i64_val));
        assert_eq!("9223372036854775808".parse::<i64>().ok(), None);

        i64_val = i64_val.wrapping_add(1);
        assert_eq!("-9223372036854775808".parse::<i64>().ok(), Some(i64_val));
        assert_eq!("-9223372036854775809".parse::<i64>().ok(), None);
    }

    #[test]
    fn test_int_from_minus_sign() {
        assert_eq!("-".parse::<i32>().ok(), None);
    }
}
