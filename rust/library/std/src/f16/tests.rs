#![cfg(not(bootstrap))]
// FIXME(f16_f128): only tested on platforms that have symbols and aren't buggy
#![cfg(reliable_f16)]

use crate::f16::consts;
use crate::num::*;

// We run out of precision pretty quickly with f16
// const F16_APPROX_L1: f16 = 0.001;
const F16_APPROX_L2: f16 = 0.01;
// const F16_APPROX_L3: f16 = 0.1;
const F16_APPROX_L4: f16 = 0.5;

/// Smallest number
const TINY_BITS: u16 = 0x1;

/// Next smallest number
const TINY_UP_BITS: u16 = 0x2;

/// Exponent = 0b11...10, Sifnificand 0b1111..10. Min val > 0
const MAX_DOWN_BITS: u16 = 0x7bfe;

/// Zeroed exponent, full significant
const LARGEST_SUBNORMAL_BITS: u16 = 0x03ff;

/// Exponent = 0b1, zeroed significand
const SMALLEST_NORMAL_BITS: u16 = 0x0400;

/// First pattern over the mantissa
const NAN_MASK1: u16 = 0x02aa;

/// Second pattern over the mantissa
const NAN_MASK2: u16 = 0x0155;

/// Compare by representation
#[allow(unused_macros)]
macro_rules! assert_f16_biteq {
    ($a:expr, $b:expr) => {
        let (l, r): (&f16, &f16) = (&$a, &$b);
        let lb = l.to_bits();
        let rb = r.to_bits();
        assert_eq!(lb, rb, "float {l:?} ({lb:#04x}) is not bitequal to {r:?} ({rb:#04x})");
    };
}

#[test]
fn test_num_f16() {
    test_num(10f16, 2f16);
}

// FIXME(f16_f128): add min and max tests when available

#[test]
fn test_nan() {
    let nan: f16 = f16::NAN;
    assert!(nan.is_nan());
    assert!(!nan.is_infinite());
    assert!(!nan.is_finite());
    assert!(nan.is_sign_positive());
    assert!(!nan.is_sign_negative());
    // FIXME(f16_f128): classify
    // assert!(!nan.is_normal());
    // assert_eq!(Fp::Nan, nan.classify());
}

#[test]
fn test_infinity() {
    let inf: f16 = f16::INFINITY;
    assert!(inf.is_infinite());
    assert!(!inf.is_finite());
    assert!(inf.is_sign_positive());
    assert!(!inf.is_sign_negative());
    assert!(!inf.is_nan());
    // FIXME(f16_f128): classify
    // assert!(!inf.is_normal());
    // assert_eq!(Fp::Infinite, inf.classify());
}

#[test]
fn test_neg_infinity() {
    let neg_inf: f16 = f16::NEG_INFINITY;
    assert!(neg_inf.is_infinite());
    assert!(!neg_inf.is_finite());
    assert!(!neg_inf.is_sign_positive());
    assert!(neg_inf.is_sign_negative());
    assert!(!neg_inf.is_nan());
    // FIXME(f16_f128): classify
    // assert!(!neg_inf.is_normal());
    // assert_eq!(Fp::Infinite, neg_inf.classify());
}

#[test]
fn test_zero() {
    let zero: f16 = 0.0f16;
    assert_eq!(0.0, zero);
    assert!(!zero.is_infinite());
    assert!(zero.is_finite());
    assert!(zero.is_sign_positive());
    assert!(!zero.is_sign_negative());
    assert!(!zero.is_nan());
    // FIXME(f16_f128): classify
    // assert!(!zero.is_normal());
    // assert_eq!(Fp::Zero, zero.classify());
}

#[test]
fn test_neg_zero() {
    let neg_zero: f16 = -0.0;
    assert_eq!(0.0, neg_zero);
    assert!(!neg_zero.is_infinite());
    assert!(neg_zero.is_finite());
    assert!(!neg_zero.is_sign_positive());
    assert!(neg_zero.is_sign_negative());
    assert!(!neg_zero.is_nan());
    // FIXME(f16_f128): classify
    // assert!(!neg_zero.is_normal());
    // assert_eq!(Fp::Zero, neg_zero.classify());
}

#[test]
fn test_one() {
    let one: f16 = 1.0f16;
    assert_eq!(1.0, one);
    assert!(!one.is_infinite());
    assert!(one.is_finite());
    assert!(one.is_sign_positive());
    assert!(!one.is_sign_negative());
    assert!(!one.is_nan());
    // FIXME(f16_f128): classify
    // assert!(one.is_normal());
    // assert_eq!(Fp::Normal, one.classify());
}

#[test]
fn test_is_nan() {
    let nan: f16 = f16::NAN;
    let inf: f16 = f16::INFINITY;
    let neg_inf: f16 = f16::NEG_INFINITY;
    assert!(nan.is_nan());
    assert!(!0.0f16.is_nan());
    assert!(!5.3f16.is_nan());
    assert!(!(-10.732f16).is_nan());
    assert!(!inf.is_nan());
    assert!(!neg_inf.is_nan());
}

#[test]
fn test_is_infinite() {
    let nan: f16 = f16::NAN;
    let inf: f16 = f16::INFINITY;
    let neg_inf: f16 = f16::NEG_INFINITY;
    assert!(!nan.is_infinite());
    assert!(inf.is_infinite());
    assert!(neg_inf.is_infinite());
    assert!(!0.0f16.is_infinite());
    assert!(!42.8f16.is_infinite());
    assert!(!(-109.2f16).is_infinite());
}

#[test]
fn test_is_finite() {
    let nan: f16 = f16::NAN;
    let inf: f16 = f16::INFINITY;
    let neg_inf: f16 = f16::NEG_INFINITY;
    assert!(!nan.is_finite());
    assert!(!inf.is_finite());
    assert!(!neg_inf.is_finite());
    assert!(0.0f16.is_finite());
    assert!(42.8f16.is_finite());
    assert!((-109.2f16).is_finite());
}

// FIXME(f16_f128): add `test_is_normal` and `test_classify` when classify is working
// FIXME(f16_f128): add missing math functions when available

#[test]
fn test_abs() {
    assert_eq!(f16::INFINITY.abs(), f16::INFINITY);
    assert_eq!(1f16.abs(), 1f16);
    assert_eq!(0f16.abs(), 0f16);
    assert_eq!((-0f16).abs(), 0f16);
    assert_eq!((-1f16).abs(), 1f16);
    assert_eq!(f16::NEG_INFINITY.abs(), f16::INFINITY);
    assert_eq!((1f16 / f16::NEG_INFINITY).abs(), 0f16);
    assert!(f16::NAN.abs().is_nan());
}

#[test]
fn test_is_sign_positive() {
    assert!(f16::INFINITY.is_sign_positive());
    assert!(1f16.is_sign_positive());
    assert!(0f16.is_sign_positive());
    assert!(!(-0f16).is_sign_positive());
    assert!(!(-1f16).is_sign_positive());
    assert!(!f16::NEG_INFINITY.is_sign_positive());
    assert!(!(1f16 / f16::NEG_INFINITY).is_sign_positive());
    assert!(f16::NAN.is_sign_positive());
    assert!(!(-f16::NAN).is_sign_positive());
}

#[test]
fn test_is_sign_negative() {
    assert!(!f16::INFINITY.is_sign_negative());
    assert!(!1f16.is_sign_negative());
    assert!(!0f16.is_sign_negative());
    assert!((-0f16).is_sign_negative());
    assert!((-1f16).is_sign_negative());
    assert!(f16::NEG_INFINITY.is_sign_negative());
    assert!((1f16 / f16::NEG_INFINITY).is_sign_negative());
    assert!(!f16::NAN.is_sign_negative());
    assert!((-f16::NAN).is_sign_negative());
}

#[test]
fn test_next_up() {
    let tiny = f16::from_bits(TINY_BITS);
    let tiny_up = f16::from_bits(TINY_UP_BITS);
    let max_down = f16::from_bits(MAX_DOWN_BITS);
    let largest_subnormal = f16::from_bits(LARGEST_SUBNORMAL_BITS);
    let smallest_normal = f16::from_bits(SMALLEST_NORMAL_BITS);
    assert_f16_biteq!(f16::NEG_INFINITY.next_up(), f16::MIN);
    assert_f16_biteq!(f16::MIN.next_up(), -max_down);
    assert_f16_biteq!((-1.0 - f16::EPSILON).next_up(), -1.0);
    assert_f16_biteq!((-smallest_normal).next_up(), -largest_subnormal);
    assert_f16_biteq!((-tiny_up).next_up(), -tiny);
    assert_f16_biteq!((-tiny).next_up(), -0.0f16);
    assert_f16_biteq!((-0.0f16).next_up(), tiny);
    assert_f16_biteq!(0.0f16.next_up(), tiny);
    assert_f16_biteq!(tiny.next_up(), tiny_up);
    assert_f16_biteq!(largest_subnormal.next_up(), smallest_normal);
    assert_f16_biteq!(1.0f16.next_up(), 1.0 + f16::EPSILON);
    assert_f16_biteq!(f16::MAX.next_up(), f16::INFINITY);
    assert_f16_biteq!(f16::INFINITY.next_up(), f16::INFINITY);

    // Check that NaNs roundtrip.
    let nan0 = f16::NAN;
    let nan1 = f16::from_bits(f16::NAN.to_bits() ^ NAN_MASK1);
    let nan2 = f16::from_bits(f16::NAN.to_bits() ^ NAN_MASK2);
    assert_f16_biteq!(nan0.next_up(), nan0);
    assert_f16_biteq!(nan1.next_up(), nan1);
    assert_f16_biteq!(nan2.next_up(), nan2);
}

#[test]
fn test_next_down() {
    let tiny = f16::from_bits(TINY_BITS);
    let tiny_up = f16::from_bits(TINY_UP_BITS);
    let max_down = f16::from_bits(MAX_DOWN_BITS);
    let largest_subnormal = f16::from_bits(LARGEST_SUBNORMAL_BITS);
    let smallest_normal = f16::from_bits(SMALLEST_NORMAL_BITS);
    assert_f16_biteq!(f16::NEG_INFINITY.next_down(), f16::NEG_INFINITY);
    assert_f16_biteq!(f16::MIN.next_down(), f16::NEG_INFINITY);
    assert_f16_biteq!((-max_down).next_down(), f16::MIN);
    assert_f16_biteq!((-1.0f16).next_down(), -1.0 - f16::EPSILON);
    assert_f16_biteq!((-largest_subnormal).next_down(), -smallest_normal);
    assert_f16_biteq!((-tiny).next_down(), -tiny_up);
    assert_f16_biteq!((-0.0f16).next_down(), -tiny);
    assert_f16_biteq!((0.0f16).next_down(), -tiny);
    assert_f16_biteq!(tiny.next_down(), 0.0f16);
    assert_f16_biteq!(tiny_up.next_down(), tiny);
    assert_f16_biteq!(smallest_normal.next_down(), largest_subnormal);
    assert_f16_biteq!((1.0 + f16::EPSILON).next_down(), 1.0f16);
    assert_f16_biteq!(f16::MAX.next_down(), max_down);
    assert_f16_biteq!(f16::INFINITY.next_down(), f16::MAX);

    // Check that NaNs roundtrip.
    let nan0 = f16::NAN;
    let nan1 = f16::from_bits(f16::NAN.to_bits() ^ NAN_MASK1);
    let nan2 = f16::from_bits(f16::NAN.to_bits() ^ NAN_MASK2);
    assert_f16_biteq!(nan0.next_down(), nan0);
    assert_f16_biteq!(nan1.next_down(), nan1);
    assert_f16_biteq!(nan2.next_down(), nan2);
}

#[test]
fn test_recip() {
    let nan: f16 = f16::NAN;
    let inf: f16 = f16::INFINITY;
    let neg_inf: f16 = f16::NEG_INFINITY;
    assert_eq!(1.0f16.recip(), 1.0);
    assert_eq!(2.0f16.recip(), 0.5);
    assert_eq!((-0.4f16).recip(), -2.5);
    assert_eq!(0.0f16.recip(), inf);
    assert!(nan.recip().is_nan());
    assert_eq!(inf.recip(), 0.0);
    assert_eq!(neg_inf.recip(), 0.0);
}

#[test]
fn test_to_degrees() {
    let pi: f16 = consts::PI;
    let nan: f16 = f16::NAN;
    let inf: f16 = f16::INFINITY;
    let neg_inf: f16 = f16::NEG_INFINITY;
    assert_eq!(0.0f16.to_degrees(), 0.0);
    assert_approx_eq!((-5.8f16).to_degrees(), -332.315521);
    assert_approx_eq!(pi.to_degrees(), 180.0, F16_APPROX_L4);
    assert!(nan.to_degrees().is_nan());
    assert_eq!(inf.to_degrees(), inf);
    assert_eq!(neg_inf.to_degrees(), neg_inf);
    assert_eq!(1_f16.to_degrees(), 57.2957795130823208767981548141051703);
}

#[test]
fn test_to_radians() {
    let pi: f16 = consts::PI;
    let nan: f16 = f16::NAN;
    let inf: f16 = f16::INFINITY;
    let neg_inf: f16 = f16::NEG_INFINITY;
    assert_eq!(0.0f16.to_radians(), 0.0);
    assert_approx_eq!(154.6f16.to_radians(), 2.698279);
    assert_approx_eq!((-332.31f16).to_radians(), -5.799903);
    assert_approx_eq!(180.0f16.to_radians(), pi, F16_APPROX_L2);
    assert!(nan.to_radians().is_nan());
    assert_eq!(inf.to_radians(), inf);
    assert_eq!(neg_inf.to_radians(), neg_inf);
}

#[test]
fn test_real_consts() {
    // FIXME(f16_f128): add math tests when available
    use super::consts;

    let pi: f16 = consts::PI;
    let frac_pi_2: f16 = consts::FRAC_PI_2;
    let frac_pi_3: f16 = consts::FRAC_PI_3;
    let frac_pi_4: f16 = consts::FRAC_PI_4;
    let frac_pi_6: f16 = consts::FRAC_PI_6;
    let frac_pi_8: f16 = consts::FRAC_PI_8;
    let frac_1_pi: f16 = consts::FRAC_1_PI;
    let frac_2_pi: f16 = consts::FRAC_2_PI;
    // let frac_2_sqrtpi: f16 = consts::FRAC_2_SQRT_PI;
    // let sqrt2: f16 = consts::SQRT_2;
    // let frac_1_sqrt2: f16 = consts::FRAC_1_SQRT_2;
    // let e: f16 = consts::E;
    // let log2_e: f16 = consts::LOG2_E;
    // let log10_e: f16 = consts::LOG10_E;
    // let ln_2: f16 = consts::LN_2;
    // let ln_10: f16 = consts::LN_10;

    assert_approx_eq!(frac_pi_2, pi / 2f16);
    assert_approx_eq!(frac_pi_3, pi / 3f16);
    assert_approx_eq!(frac_pi_4, pi / 4f16);
    assert_approx_eq!(frac_pi_6, pi / 6f16);
    assert_approx_eq!(frac_pi_8, pi / 8f16);
    assert_approx_eq!(frac_1_pi, 1f16 / pi);
    assert_approx_eq!(frac_2_pi, 2f16 / pi);
    // assert_approx_eq!(frac_2_sqrtpi, 2f16 / pi.sqrt());
    // assert_approx_eq!(sqrt2, 2f16.sqrt());
    // assert_approx_eq!(frac_1_sqrt2, 1f16 / 2f16.sqrt());
    // assert_approx_eq!(log2_e, e.log2());
    // assert_approx_eq!(log10_e, e.log10());
    // assert_approx_eq!(ln_2, 2f16.ln());
    // assert_approx_eq!(ln_10, 10f16.ln());
}

#[test]
fn test_float_bits_conv() {
    assert_eq!((1f16).to_bits(), 0x3c00);
    assert_eq!((12.5f16).to_bits(), 0x4a40);
    assert_eq!((1337f16).to_bits(), 0x6539);
    assert_eq!((-14.25f16).to_bits(), 0xcb20);
    assert_approx_eq!(f16::from_bits(0x3c00), 1.0);
    assert_approx_eq!(f16::from_bits(0x4a40), 12.5);
    assert_approx_eq!(f16::from_bits(0x6539), 1337.0);
    assert_approx_eq!(f16::from_bits(0xcb20), -14.25);

    // Check that NaNs roundtrip their bits regardless of signaling-ness
    let masked_nan1 = f16::NAN.to_bits() ^ NAN_MASK1;
    let masked_nan2 = f16::NAN.to_bits() ^ NAN_MASK2;
    assert!(f16::from_bits(masked_nan1).is_nan());
    assert!(f16::from_bits(masked_nan2).is_nan());

    assert_eq!(f16::from_bits(masked_nan1).to_bits(), masked_nan1);
    assert_eq!(f16::from_bits(masked_nan2).to_bits(), masked_nan2);
}

#[test]
#[should_panic]
fn test_clamp_min_greater_than_max() {
    let _ = 1.0f16.clamp(3.0, 1.0);
}

#[test]
#[should_panic]
fn test_clamp_min_is_nan() {
    let _ = 1.0f16.clamp(f16::NAN, 1.0);
}

#[test]
#[should_panic]
fn test_clamp_max_is_nan() {
    let _ = 1.0f16.clamp(3.0, f16::NAN);
}

#[test]
fn test_total_cmp() {
    use core::cmp::Ordering;

    fn quiet_bit_mask() -> u16 {
        1 << (f16::MANTISSA_DIGITS - 2)
    }

    // FIXME(f16_f128): test subnormals when powf is available
    // fn min_subnorm() -> f16 {
    //     f16::MIN_POSITIVE / f16::powf(2.0, f16::MANTISSA_DIGITS as f16 - 1.0)
    // }

    // fn max_subnorm() -> f16 {
    //     f16::MIN_POSITIVE - min_subnorm()
    // }

    fn q_nan() -> f16 {
        f16::from_bits(f16::NAN.to_bits() | quiet_bit_mask())
    }

    fn s_nan() -> f16 {
        f16::from_bits((f16::NAN.to_bits() & !quiet_bit_mask()) + 42)
    }

    assert_eq!(Ordering::Equal, (-q_nan()).total_cmp(&-q_nan()));
    assert_eq!(Ordering::Equal, (-s_nan()).total_cmp(&-s_nan()));
    assert_eq!(Ordering::Equal, (-f16::INFINITY).total_cmp(&-f16::INFINITY));
    assert_eq!(Ordering::Equal, (-f16::MAX).total_cmp(&-f16::MAX));
    assert_eq!(Ordering::Equal, (-2.5_f16).total_cmp(&-2.5));
    assert_eq!(Ordering::Equal, (-1.0_f16).total_cmp(&-1.0));
    assert_eq!(Ordering::Equal, (-1.5_f16).total_cmp(&-1.5));
    assert_eq!(Ordering::Equal, (-0.5_f16).total_cmp(&-0.5));
    assert_eq!(Ordering::Equal, (-f16::MIN_POSITIVE).total_cmp(&-f16::MIN_POSITIVE));
    // assert_eq!(Ordering::Equal, (-max_subnorm()).total_cmp(&-max_subnorm()));
    // assert_eq!(Ordering::Equal, (-min_subnorm()).total_cmp(&-min_subnorm()));
    assert_eq!(Ordering::Equal, (-0.0_f16).total_cmp(&-0.0));
    assert_eq!(Ordering::Equal, 0.0_f16.total_cmp(&0.0));
    // assert_eq!(Ordering::Equal, min_subnorm().total_cmp(&min_subnorm()));
    // assert_eq!(Ordering::Equal, max_subnorm().total_cmp(&max_subnorm()));
    assert_eq!(Ordering::Equal, f16::MIN_POSITIVE.total_cmp(&f16::MIN_POSITIVE));
    assert_eq!(Ordering::Equal, 0.5_f16.total_cmp(&0.5));
    assert_eq!(Ordering::Equal, 1.0_f16.total_cmp(&1.0));
    assert_eq!(Ordering::Equal, 1.5_f16.total_cmp(&1.5));
    assert_eq!(Ordering::Equal, 2.5_f16.total_cmp(&2.5));
    assert_eq!(Ordering::Equal, f16::MAX.total_cmp(&f16::MAX));
    assert_eq!(Ordering::Equal, f16::INFINITY.total_cmp(&f16::INFINITY));
    assert_eq!(Ordering::Equal, s_nan().total_cmp(&s_nan()));
    assert_eq!(Ordering::Equal, q_nan().total_cmp(&q_nan()));

    assert_eq!(Ordering::Less, (-q_nan()).total_cmp(&-s_nan()));
    assert_eq!(Ordering::Less, (-s_nan()).total_cmp(&-f16::INFINITY));
    assert_eq!(Ordering::Less, (-f16::INFINITY).total_cmp(&-f16::MAX));
    assert_eq!(Ordering::Less, (-f16::MAX).total_cmp(&-2.5));
    assert_eq!(Ordering::Less, (-2.5_f16).total_cmp(&-1.5));
    assert_eq!(Ordering::Less, (-1.5_f16).total_cmp(&-1.0));
    assert_eq!(Ordering::Less, (-1.0_f16).total_cmp(&-0.5));
    assert_eq!(Ordering::Less, (-0.5_f16).total_cmp(&-f16::MIN_POSITIVE));
    // assert_eq!(Ordering::Less, (-f16::MIN_POSITIVE).total_cmp(&-max_subnorm()));
    // assert_eq!(Ordering::Less, (-max_subnorm()).total_cmp(&-min_subnorm()));
    // assert_eq!(Ordering::Less, (-min_subnorm()).total_cmp(&-0.0));
    assert_eq!(Ordering::Less, (-0.0_f16).total_cmp(&0.0));
    // assert_eq!(Ordering::Less, 0.0_f16.total_cmp(&min_subnorm()));
    // assert_eq!(Ordering::Less, min_subnorm().total_cmp(&max_subnorm()));
    // assert_eq!(Ordering::Less, max_subnorm().total_cmp(&f16::MIN_POSITIVE));
    assert_eq!(Ordering::Less, f16::MIN_POSITIVE.total_cmp(&0.5));
    assert_eq!(Ordering::Less, 0.5_f16.total_cmp(&1.0));
    assert_eq!(Ordering::Less, 1.0_f16.total_cmp(&1.5));
    assert_eq!(Ordering::Less, 1.5_f16.total_cmp(&2.5));
    assert_eq!(Ordering::Less, 2.5_f16.total_cmp(&f16::MAX));
    assert_eq!(Ordering::Less, f16::MAX.total_cmp(&f16::INFINITY));
    assert_eq!(Ordering::Less, f16::INFINITY.total_cmp(&s_nan()));
    assert_eq!(Ordering::Less, s_nan().total_cmp(&q_nan()));

    assert_eq!(Ordering::Greater, (-s_nan()).total_cmp(&-q_nan()));
    assert_eq!(Ordering::Greater, (-f16::INFINITY).total_cmp(&-s_nan()));
    assert_eq!(Ordering::Greater, (-f16::MAX).total_cmp(&-f16::INFINITY));
    assert_eq!(Ordering::Greater, (-2.5_f16).total_cmp(&-f16::MAX));
    assert_eq!(Ordering::Greater, (-1.5_f16).total_cmp(&-2.5));
    assert_eq!(Ordering::Greater, (-1.0_f16).total_cmp(&-1.5));
    assert_eq!(Ordering::Greater, (-0.5_f16).total_cmp(&-1.0));
    assert_eq!(Ordering::Greater, (-f16::MIN_POSITIVE).total_cmp(&-0.5));
    // assert_eq!(Ordering::Greater, (-max_subnorm()).total_cmp(&-f16::MIN_POSITIVE));
    // assert_eq!(Ordering::Greater, (-min_subnorm()).total_cmp(&-max_subnorm()));
    // assert_eq!(Ordering::Greater, (-0.0_f16).total_cmp(&-min_subnorm()));
    assert_eq!(Ordering::Greater, 0.0_f16.total_cmp(&-0.0));
    // assert_eq!(Ordering::Greater, min_subnorm().total_cmp(&0.0));
    // assert_eq!(Ordering::Greater, max_subnorm().total_cmp(&min_subnorm()));
    // assert_eq!(Ordering::Greater, f16::MIN_POSITIVE.total_cmp(&max_subnorm()));
    assert_eq!(Ordering::Greater, 0.5_f16.total_cmp(&f16::MIN_POSITIVE));
    assert_eq!(Ordering::Greater, 1.0_f16.total_cmp(&0.5));
    assert_eq!(Ordering::Greater, 1.5_f16.total_cmp(&1.0));
    assert_eq!(Ordering::Greater, 2.5_f16.total_cmp(&1.5));
    assert_eq!(Ordering::Greater, f16::MAX.total_cmp(&2.5));
    assert_eq!(Ordering::Greater, f16::INFINITY.total_cmp(&f16::MAX));
    assert_eq!(Ordering::Greater, s_nan().total_cmp(&f16::INFINITY));
    assert_eq!(Ordering::Greater, q_nan().total_cmp(&s_nan()));

    assert_eq!(Ordering::Less, (-q_nan()).total_cmp(&-s_nan()));
    assert_eq!(Ordering::Less, (-q_nan()).total_cmp(&-f16::INFINITY));
    assert_eq!(Ordering::Less, (-q_nan()).total_cmp(&-f16::MAX));
    assert_eq!(Ordering::Less, (-q_nan()).total_cmp(&-2.5));
    assert_eq!(Ordering::Less, (-q_nan()).total_cmp(&-1.5));
    assert_eq!(Ordering::Less, (-q_nan()).total_cmp(&-1.0));
    assert_eq!(Ordering::Less, (-q_nan()).total_cmp(&-0.5));
    assert_eq!(Ordering::Less, (-q_nan()).total_cmp(&-f16::MIN_POSITIVE));
    // assert_eq!(Ordering::Less, (-q_nan()).total_cmp(&-max_subnorm()));
    // assert_eq!(Ordering::Less, (-q_nan()).total_cmp(&-min_subnorm()));
    assert_eq!(Ordering::Less, (-q_nan()).total_cmp(&-0.0));
    assert_eq!(Ordering::Less, (-q_nan()).total_cmp(&0.0));
    // assert_eq!(Ordering::Less, (-q_nan()).total_cmp(&min_subnorm()));
    // assert_eq!(Ordering::Less, (-q_nan()).total_cmp(&max_subnorm()));
    assert_eq!(Ordering::Less, (-q_nan()).total_cmp(&f16::MIN_POSITIVE));
    assert_eq!(Ordering::Less, (-q_nan()).total_cmp(&0.5));
    assert_eq!(Ordering::Less, (-q_nan()).total_cmp(&1.0));
    assert_eq!(Ordering::Less, (-q_nan()).total_cmp(&1.5));
    assert_eq!(Ordering::Less, (-q_nan()).total_cmp(&2.5));
    assert_eq!(Ordering::Less, (-q_nan()).total_cmp(&f16::MAX));
    assert_eq!(Ordering::Less, (-q_nan()).total_cmp(&f16::INFINITY));
    assert_eq!(Ordering::Less, (-q_nan()).total_cmp(&s_nan()));

    assert_eq!(Ordering::Less, (-s_nan()).total_cmp(&-f16::INFINITY));
    assert_eq!(Ordering::Less, (-s_nan()).total_cmp(&-f16::MAX));
    assert_eq!(Ordering::Less, (-s_nan()).total_cmp(&-2.5));
    assert_eq!(Ordering::Less, (-s_nan()).total_cmp(&-1.5));
    assert_eq!(Ordering::Less, (-s_nan()).total_cmp(&-1.0));
    assert_eq!(Ordering::Less, (-s_nan()).total_cmp(&-0.5));
    assert_eq!(Ordering::Less, (-s_nan()).total_cmp(&-f16::MIN_POSITIVE));
    // assert_eq!(Ordering::Less, (-s_nan()).total_cmp(&-max_subnorm()));
    // assert_eq!(Ordering::Less, (-s_nan()).total_cmp(&-min_subnorm()));
    assert_eq!(Ordering::Less, (-s_nan()).total_cmp(&-0.0));
    assert_eq!(Ordering::Less, (-s_nan()).total_cmp(&0.0));
    // assert_eq!(Ordering::Less, (-s_nan()).total_cmp(&min_subnorm()));
    // assert_eq!(Ordering::Less, (-s_nan()).total_cmp(&max_subnorm()));
    assert_eq!(Ordering::Less, (-s_nan()).total_cmp(&f16::MIN_POSITIVE));
    assert_eq!(Ordering::Less, (-s_nan()).total_cmp(&0.5));
    assert_eq!(Ordering::Less, (-s_nan()).total_cmp(&1.0));
    assert_eq!(Ordering::Less, (-s_nan()).total_cmp(&1.5));
    assert_eq!(Ordering::Less, (-s_nan()).total_cmp(&2.5));
    assert_eq!(Ordering::Less, (-s_nan()).total_cmp(&f16::MAX));
    assert_eq!(Ordering::Less, (-s_nan()).total_cmp(&f16::INFINITY));
    assert_eq!(Ordering::Less, (-s_nan()).total_cmp(&s_nan()));
}
