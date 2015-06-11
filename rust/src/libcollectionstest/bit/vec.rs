// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::collections::BitVec;
use std::u32;

#[test]
fn test_to_str() {
    let zerolen = BitVec::new();
    assert_eq!(format!("{:?}", zerolen), "");

    let eightbits = BitVec::from_elem(8, false);
    assert_eq!(format!("{:?}", eightbits), "00000000")
}

#[test]
fn test_0_elements() {
    let act = BitVec::new();
    let exp = Vec::new();
    assert!(act.eq_vec(&exp));
    assert!(act.none() && act.all());
}

#[test]
fn test_1_element() {
    let mut act = BitVec::from_elem(1, false);
    assert!(act.eq_vec(&[false]));
    assert!(act.none() && !act.all());
    act = BitVec::from_elem(1, true);
    assert!(act.eq_vec(&[true]));
    assert!(!act.none() && act.all());
}

#[test]
fn test_2_elements() {
    let mut b = BitVec::from_elem(2, false);
    b.set(0, true);
    b.set(1, false);
    assert_eq!(format!("{:?}", b), "10");
    assert!(!b.none() && !b.all());
}

#[test]
fn test_10_elements() {
    let mut act;
    // all 0

    act = BitVec::from_elem(10, false);
    assert!((act.eq_vec(
                &[false, false, false, false, false, false, false, false, false, false])));
    assert!(act.none() && !act.all());
    // all 1

    act = BitVec::from_elem(10, true);
    assert!((act.eq_vec(&[true, true, true, true, true, true, true, true, true, true])));
    assert!(!act.none() && act.all());
    // mixed

    act = BitVec::from_elem(10, false);
    act.set(0, true);
    act.set(1, true);
    act.set(2, true);
    act.set(3, true);
    act.set(4, true);
    assert!((act.eq_vec(&[true, true, true, true, true, false, false, false, false, false])));
    assert!(!act.none() && !act.all());
    // mixed

    act = BitVec::from_elem(10, false);
    act.set(5, true);
    act.set(6, true);
    act.set(7, true);
    act.set(8, true);
    act.set(9, true);
    assert!((act.eq_vec(&[false, false, false, false, false, true, true, true, true, true])));
    assert!(!act.none() && !act.all());
    // mixed

    act = BitVec::from_elem(10, false);
    act.set(0, true);
    act.set(3, true);
    act.set(6, true);
    act.set(9, true);
    assert!((act.eq_vec(&[true, false, false, true, false, false, true, false, false, true])));
    assert!(!act.none() && !act.all());
}

#[test]
fn test_31_elements() {
    let mut act;
    // all 0

    act = BitVec::from_elem(31, false);
    assert!(act.eq_vec(
            &[false, false, false, false, false, false, false, false, false, false, false,
              false, false, false, false, false, false, false, false, false, false, false,
              false, false, false, false, false, false, false, false, false]));
    assert!(act.none() && !act.all());
    // all 1

    act = BitVec::from_elem(31, true);
    assert!(act.eq_vec(
            &[true, true, true, true, true, true, true, true, true, true, true, true, true,
              true, true, true, true, true, true, true, true, true, true, true, true, true,
              true, true, true, true, true]));
    assert!(!act.none() && act.all());
    // mixed

    act = BitVec::from_elem(31, false);
    act.set(0, true);
    act.set(1, true);
    act.set(2, true);
    act.set(3, true);
    act.set(4, true);
    act.set(5, true);
    act.set(6, true);
    act.set(7, true);
    assert!(act.eq_vec(
            &[true, true, true, true, true, true, true, true, false, false, false, false, false,
              false, false, false, false, false, false, false, false, false, false, false,
              false, false, false, false, false, false, false]));
    assert!(!act.none() && !act.all());
    // mixed

    act = BitVec::from_elem(31, false);
    act.set(16, true);
    act.set(17, true);
    act.set(18, true);
    act.set(19, true);
    act.set(20, true);
    act.set(21, true);
    act.set(22, true);
    act.set(23, true);
    assert!(act.eq_vec(
            &[false, false, false, false, false, false, false, false, false, false, false,
              false, false, false, false, false, true, true, true, true, true, true, true, true,
              false, false, false, false, false, false, false]));
    assert!(!act.none() && !act.all());
    // mixed

    act = BitVec::from_elem(31, false);
    act.set(24, true);
    act.set(25, true);
    act.set(26, true);
    act.set(27, true);
    act.set(28, true);
    act.set(29, true);
    act.set(30, true);
    assert!(act.eq_vec(
            &[false, false, false, false, false, false, false, false, false, false, false,
              false, false, false, false, false, false, false, false, false, false, false,
              false, false, true, true, true, true, true, true, true]));
    assert!(!act.none() && !act.all());
    // mixed

    act = BitVec::from_elem(31, false);
    act.set(3, true);
    act.set(17, true);
    act.set(30, true);
    assert!(act.eq_vec(
            &[false, false, false, true, false, false, false, false, false, false, false, false,
              false, false, false, false, false, true, false, false, false, false, false, false,
              false, false, false, false, false, false, true]));
    assert!(!act.none() && !act.all());
}

#[test]
fn test_32_elements() {
    let mut act;
    // all 0

    act = BitVec::from_elem(32, false);
    assert!(act.eq_vec(
            &[false, false, false, false, false, false, false, false, false, false, false,
              false, false, false, false, false, false, false, false, false, false, false,
              false, false, false, false, false, false, false, false, false, false]));
    assert!(act.none() && !act.all());
    // all 1

    act = BitVec::from_elem(32, true);
    assert!(act.eq_vec(
            &[true, true, true, true, true, true, true, true, true, true, true, true, true,
              true, true, true, true, true, true, true, true, true, true, true, true, true,
              true, true, true, true, true, true]));
    assert!(!act.none() && act.all());
    // mixed

    act = BitVec::from_elem(32, false);
    act.set(0, true);
    act.set(1, true);
    act.set(2, true);
    act.set(3, true);
    act.set(4, true);
    act.set(5, true);
    act.set(6, true);
    act.set(7, true);
    assert!(act.eq_vec(
            &[true, true, true, true, true, true, true, true, false, false, false, false, false,
              false, false, false, false, false, false, false, false, false, false, false,
              false, false, false, false, false, false, false, false]));
    assert!(!act.none() && !act.all());
    // mixed

    act = BitVec::from_elem(32, false);
    act.set(16, true);
    act.set(17, true);
    act.set(18, true);
    act.set(19, true);
    act.set(20, true);
    act.set(21, true);
    act.set(22, true);
    act.set(23, true);
    assert!(act.eq_vec(
            &[false, false, false, false, false, false, false, false, false, false, false,
              false, false, false, false, false, true, true, true, true, true, true, true, true,
              false, false, false, false, false, false, false, false]));
    assert!(!act.none() && !act.all());
    // mixed

    act = BitVec::from_elem(32, false);
    act.set(24, true);
    act.set(25, true);
    act.set(26, true);
    act.set(27, true);
    act.set(28, true);
    act.set(29, true);
    act.set(30, true);
    act.set(31, true);
    assert!(act.eq_vec(
            &[false, false, false, false, false, false, false, false, false, false, false,
              false, false, false, false, false, false, false, false, false, false, false,
              false, false, true, true, true, true, true, true, true, true]));
    assert!(!act.none() && !act.all());
    // mixed

    act = BitVec::from_elem(32, false);
    act.set(3, true);
    act.set(17, true);
    act.set(30, true);
    act.set(31, true);
    assert!(act.eq_vec(
            &[false, false, false, true, false, false, false, false, false, false, false, false,
              false, false, false, false, false, true, false, false, false, false, false, false,
              false, false, false, false, false, false, true, true]));
    assert!(!act.none() && !act.all());
}

#[test]
fn test_33_elements() {
    let mut act;
    // all 0

    act = BitVec::from_elem(33, false);
    assert!(act.eq_vec(
            &[false, false, false, false, false, false, false, false, false, false, false,
              false, false, false, false, false, false, false, false, false, false, false,
              false, false, false, false, false, false, false, false, false, false, false]));
    assert!(act.none() && !act.all());
    // all 1

    act = BitVec::from_elem(33, true);
    assert!(act.eq_vec(
            &[true, true, true, true, true, true, true, true, true, true, true, true, true,
              true, true, true, true, true, true, true, true, true, true, true, true, true,
              true, true, true, true, true, true, true]));
    assert!(!act.none() && act.all());
    // mixed

    act = BitVec::from_elem(33, false);
    act.set(0, true);
    act.set(1, true);
    act.set(2, true);
    act.set(3, true);
    act.set(4, true);
    act.set(5, true);
    act.set(6, true);
    act.set(7, true);
    assert!(act.eq_vec(
            &[true, true, true, true, true, true, true, true, false, false, false, false, false,
              false, false, false, false, false, false, false, false, false, false, false,
              false, false, false, false, false, false, false, false, false]));
    assert!(!act.none() && !act.all());
    // mixed

    act = BitVec::from_elem(33, false);
    act.set(16, true);
    act.set(17, true);
    act.set(18, true);
    act.set(19, true);
    act.set(20, true);
    act.set(21, true);
    act.set(22, true);
    act.set(23, true);
    assert!(act.eq_vec(
            &[false, false, false, false, false, false, false, false, false, false, false,
              false, false, false, false, false, true, true, true, true, true, true, true, true,
              false, false, false, false, false, false, false, false, false]));
    assert!(!act.none() && !act.all());
    // mixed

    act = BitVec::from_elem(33, false);
    act.set(24, true);
    act.set(25, true);
    act.set(26, true);
    act.set(27, true);
    act.set(28, true);
    act.set(29, true);
    act.set(30, true);
    act.set(31, true);
    assert!(act.eq_vec(
            &[false, false, false, false, false, false, false, false, false, false, false,
              false, false, false, false, false, false, false, false, false, false, false,
              false, false, true, true, true, true, true, true, true, true, false]));
    assert!(!act.none() && !act.all());
    // mixed

    act = BitVec::from_elem(33, false);
    act.set(3, true);
    act.set(17, true);
    act.set(30, true);
    act.set(31, true);
    act.set(32, true);
    assert!(act.eq_vec(
            &[false, false, false, true, false, false, false, false, false, false, false, false,
              false, false, false, false, false, true, false, false, false, false, false, false,
              false, false, false, false, false, false, true, true, true]));
    assert!(!act.none() && !act.all());
}

#[test]
fn test_equal_differing_sizes() {
    let v0 = BitVec::from_elem(10, false);
    let v1 = BitVec::from_elem(11, false);
    assert!(v0 != v1);
}

#[test]
fn test_equal_greatly_differing_sizes() {
    let v0 = BitVec::from_elem(10, false);
    let v1 = BitVec::from_elem(110, false);
    assert!(v0 != v1);
}

#[test]
fn test_equal_sneaky_small() {
    let mut a = BitVec::from_elem(1, false);
    a.set(0, true);

    let mut b = BitVec::from_elem(1, true);
    b.set(0, true);

    assert_eq!(a, b);
}

#[test]
fn test_equal_sneaky_big() {
    let mut a = BitVec::from_elem(100, false);
    for i in 0..100 {
        a.set(i, true);
    }

    let mut b = BitVec::from_elem(100, true);
    for i in 0..100 {
        b.set(i, true);
    }

    assert_eq!(a, b);
}

#[test]
fn test_from_bytes() {
    let bit_vec = BitVec::from_bytes(&[0b10110110, 0b00000000, 0b11111111]);
    let str = concat!("10110110", "00000000", "11111111");
    assert_eq!(format!("{:?}", bit_vec), str);
}

#[test]
fn test_to_bytes() {
    let mut bv = BitVec::from_elem(3, true);
    bv.set(1, false);
    assert_eq!(bv.to_bytes(), [0b10100000]);

    let mut bv = BitVec::from_elem(9, false);
    bv.set(2, true);
    bv.set(8, true);
    assert_eq!(bv.to_bytes(), [0b00100000, 0b10000000]);
}

#[test]
fn test_from_bools() {
    let bools = vec![true, false, true, true];
    let bit_vec: BitVec = bools.iter().map(|n| *n).collect();
    assert_eq!(format!("{:?}", bit_vec), "1011");
}

#[test]
fn test_to_bools() {
    let bools = vec![false, false, true, false, false, true, true, false];
    assert_eq!(BitVec::from_bytes(&[0b00100110]).iter().collect::<Vec<bool>>(), bools);
}

#[test]
fn test_bit_vec_iterator() {
    let bools = vec![true, false, true, true];
    let bit_vec: BitVec = bools.iter().map(|n| *n).collect();

    assert_eq!(bit_vec.iter().collect::<Vec<bool>>(), bools);

    let long: Vec<_> = (0..10000).map(|i| i % 2 == 0).collect();
    let bit_vec: BitVec = long.iter().map(|n| *n).collect();
    assert_eq!(bit_vec.iter().collect::<Vec<bool>>(), long)
}

#[test]
fn test_small_difference() {
    let mut b1 = BitVec::from_elem(3, false);
    let mut b2 = BitVec::from_elem(3, false);
    b1.set(0, true);
    b1.set(1, true);
    b2.set(1, true);
    b2.set(2, true);
    assert!(b1.difference(&b2));
    assert!(b1[0]);
    assert!(!b1[1]);
    assert!(!b1[2]);
}

#[test]
fn test_big_difference() {
    let mut b1 = BitVec::from_elem(100, false);
    let mut b2 = BitVec::from_elem(100, false);
    b1.set(0, true);
    b1.set(40, true);
    b2.set(40, true);
    b2.set(80, true);
    assert!(b1.difference(&b2));
    assert!(b1[0]);
    assert!(!b1[40]);
    assert!(!b1[80]);
}

#[test]
fn test_small_clear() {
    let mut b = BitVec::from_elem(14, true);
    assert!(!b.none() && b.all());
    b.clear();
    assert!(b.none() && !b.all());
}

#[test]
fn test_big_clear() {
    let mut b = BitVec::from_elem(140, true);
    assert!(!b.none() && b.all());
    b.clear();
    assert!(b.none() && !b.all());
}

#[test]
fn test_bit_vec_lt() {
    let mut a = BitVec::from_elem(5, false);
    let mut b = BitVec::from_elem(5, false);

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
    let mut a = BitVec::from_elem(5, false);
    let mut b = BitVec::from_elem(5, false);

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
fn test_small_bit_vec_tests() {
    let v = BitVec::from_bytes(&[0]);
    assert!(!v.all());
    assert!(!v.any());
    assert!(v.none());

    let v = BitVec::from_bytes(&[0b00010100]);
    assert!(!v.all());
    assert!(v.any());
    assert!(!v.none());

    let v = BitVec::from_bytes(&[0xFF]);
    assert!(v.all());
    assert!(v.any());
    assert!(!v.none());
}

#[test]
fn test_big_bit_vec_tests() {
    let v = BitVec::from_bytes(&[ // 88 bits
        0, 0, 0, 0,
        0, 0, 0, 0,
        0, 0, 0]);
    assert!(!v.all());
    assert!(!v.any());
    assert!(v.none());

    let v = BitVec::from_bytes(&[ // 88 bits
        0, 0, 0b00010100, 0,
        0, 0, 0, 0b00110100,
        0, 0, 0]);
    assert!(!v.all());
    assert!(v.any());
    assert!(!v.none());

    let v = BitVec::from_bytes(&[ // 88 bits
        0xFF, 0xFF, 0xFF, 0xFF,
        0xFF, 0xFF, 0xFF, 0xFF,
        0xFF, 0xFF, 0xFF]);
    assert!(v.all());
    assert!(v.any());
    assert!(!v.none());
}

#[test]
fn test_bit_vec_push_pop() {
    let mut s = BitVec::from_elem(5 * u32::BITS - 2, false);
    assert_eq!(s.len(), 5 * u32::BITS - 2);
    assert_eq!(s[5 * u32::BITS - 3], false);
    s.push(true);
    s.push(true);
    assert_eq!(s[5 * u32::BITS - 2], true);
    assert_eq!(s[5 * u32::BITS - 1], true);
    // Here the internal vector will need to be extended
    s.push(false);
    assert_eq!(s[5 * u32::BITS], false);
    s.push(false);
    assert_eq!(s[5 * u32::BITS + 1], false);
    assert_eq!(s.len(), 5 * u32::BITS + 2);
    // Pop it all off
    assert_eq!(s.pop(), Some(false));
    assert_eq!(s.pop(), Some(false));
    assert_eq!(s.pop(), Some(true));
    assert_eq!(s.pop(), Some(true));
    assert_eq!(s.len(), 5 * u32::BITS - 2);
}

#[test]
fn test_bit_vec_truncate() {
    let mut s = BitVec::from_elem(5 * u32::BITS, true);

    assert_eq!(s, BitVec::from_elem(5 * u32::BITS, true));
    assert_eq!(s.len(), 5 * u32::BITS);
    s.truncate(4 * u32::BITS);
    assert_eq!(s, BitVec::from_elem(4 * u32::BITS, true));
    assert_eq!(s.len(), 4 * u32::BITS);
    // Truncating to a size > s.len() should be a noop
    s.truncate(5 * u32::BITS);
    assert_eq!(s, BitVec::from_elem(4 * u32::BITS, true));
    assert_eq!(s.len(), 4 * u32::BITS);
    s.truncate(3 * u32::BITS - 10);
    assert_eq!(s, BitVec::from_elem(3 * u32::BITS - 10, true));
    assert_eq!(s.len(), 3 * u32::BITS - 10);
    s.truncate(0);
    assert_eq!(s, BitVec::from_elem(0, true));
    assert_eq!(s.len(), 0);
}

#[test]
fn test_bit_vec_reserve() {
    let mut s = BitVec::from_elem(5 * u32::BITS, true);
    // Check capacity
    assert!(s.capacity() >= 5 * u32::BITS);
    s.reserve(2 * u32::BITS);
    assert!(s.capacity() >= 7 * u32::BITS);
    s.reserve(7 * u32::BITS);
    assert!(s.capacity() >= 12 * u32::BITS);
    s.reserve_exact(7 * u32::BITS);
    assert!(s.capacity() >= 12 * u32::BITS);
    s.reserve(7 * u32::BITS + 1);
    assert!(s.capacity() >= 12 * u32::BITS + 1);
    // Check that length hasn't changed
    assert_eq!(s.len(), 5 * u32::BITS);
    s.push(true);
    s.push(false);
    s.push(true);
    assert_eq!(s[5 * u32::BITS - 1], true);
    assert_eq!(s[5 * u32::BITS - 0], true);
    assert_eq!(s[5 * u32::BITS + 1], false);
    assert_eq!(s[5 * u32::BITS + 2], true);
}

#[test]
fn test_bit_vec_grow() {
    let mut bit_vec = BitVec::from_bytes(&[0b10110110, 0b00000000, 0b10101010]);
    bit_vec.grow(32, true);
    assert_eq!(bit_vec, BitVec::from_bytes(&[0b10110110, 0b00000000, 0b10101010,
                                 0xFF, 0xFF, 0xFF, 0xFF]));
    bit_vec.grow(64, false);
    assert_eq!(bit_vec, BitVec::from_bytes(&[0b10110110, 0b00000000, 0b10101010,
                                 0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0]));
    bit_vec.grow(16, true);
    assert_eq!(bit_vec, BitVec::from_bytes(&[0b10110110, 0b00000000, 0b10101010,
                                 0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0, 0xFF, 0xFF]));
}

#[test]
fn test_bit_vec_extend() {
    let mut bit_vec = BitVec::from_bytes(&[0b10110110, 0b00000000, 0b11111111]);
    let ext = BitVec::from_bytes(&[0b01001001, 0b10010010, 0b10111101]);
    bit_vec.extend(&ext);
    assert_eq!(bit_vec, BitVec::from_bytes(&[0b10110110, 0b00000000, 0b11111111,
                                 0b01001001, 0b10010010, 0b10111101]));
}

#[test]
fn test_bit_vecextend_ref() {
    let mut bv = BitVec::from_bytes(&[0b10100011]);
    bv.extend(&[true, false, true]);

    assert_eq!(bv.len(), 11);
    assert!(bv.eq_vec(&[true, false, true, false, false, false, true, true,
                        true, false, true]));

    let bw = BitVec::from_bytes(&[0b00010001]);
    bv.extend(&bw);

    assert_eq!(bv.len(), 19);
    assert!(bv.eq_vec(&[true, false, true, false, false, false, true, true,
                        true, false, true, false, false, false, true, false,
                        false, false, true]));
}

#[test]
fn test_bit_vec_append() {
    // Append to BitVec that holds a multiple of u32::BITS bits
    let mut a = BitVec::from_bytes(&[0b10100000, 0b00010010, 0b10010010, 0b00110011]);
    let mut b = BitVec::new();
    b.push(false);
    b.push(true);
    b.push(true);

    a.append(&mut b);

    assert_eq!(a.len(), 35);
    assert_eq!(b.len(), 0);
    assert!(b.capacity() >= 3);

    assert!(a.eq_vec(&[true, false, true, false, false, false, false, false,
                       false, false, false, true, false, false, true, false,
                       true, false, false, true, false, false, true, false,
                       false, false, true, true, false, false, true, true,
                       false, true, true]));

    // Append to arbitrary BitVec
    let mut a = BitVec::new();
    a.push(true);
    a.push(false);

    let mut b = BitVec::from_bytes(&[0b10100000, 0b00010010, 0b10010010, 0b00110011, 0b10010101]);

    a.append(&mut b);

    assert_eq!(a.len(), 42);
    assert_eq!(b.len(), 0);
    assert!(b.capacity() >= 40);

    assert!(a.eq_vec(&[true, false, true, false, true, false, false, false,
                       false, false, false, false, false, true, false, false,
                       true, false, true, false, false, true, false, false,
                       true, false, false, false, true, true, false, false,
                       true, true, true, false, false, true, false, true,
                       false, true]));

    // Append to empty BitVec
    let mut a = BitVec::new();
    let mut b = BitVec::from_bytes(&[0b10100000, 0b00010010, 0b10010010, 0b00110011, 0b10010101]);

    a.append(&mut b);

    assert_eq!(a.len(), 40);
    assert_eq!(b.len(), 0);
    assert!(b.capacity() >= 40);

    assert!(a.eq_vec(&[true, false, true, false, false, false, false, false,
                       false, false, false, true, false, false, true, false,
                       true, false, false, true, false, false, true, false,
                       false, false, true, true, false, false, true, true,
                       true, false, false, true, false, true, false, true]));

    // Append empty BitVec
    let mut a = BitVec::from_bytes(&[0b10100000, 0b00010010, 0b10010010, 0b00110011, 0b10010101]);
    let mut b = BitVec::new();

    a.append(&mut b);

    assert_eq!(a.len(), 40);
    assert_eq!(b.len(), 0);

    assert!(a.eq_vec(&[true, false, true, false, false, false, false, false,
                       false, false, false, true, false, false, true, false,
                       true, false, false, true, false, false, true, false,
                       false, false, true, true, false, false, true, true,
                       true, false, false, true, false, true, false, true]));
}

#[test]
fn test_bit_vec_split_off() {
    // Split at 0
    let mut a = BitVec::new();
    a.push(true);
    a.push(false);
    a.push(false);
    a.push(true);

    let b = a.split_off(0);

    assert_eq!(a.len(), 0);
    assert_eq!(b.len(), 4);

    assert!(b.eq_vec(&[true, false, false, true]));

    // Split at last bit
    a.truncate(0);
    a.push(true);
    a.push(false);
    a.push(false);
    a.push(true);

    let b = a.split_off(4);

    assert_eq!(a.len(), 4);
    assert_eq!(b.len(), 0);

    assert!(a.eq_vec(&[true, false, false, true]));

    // Split at block boundary
    let mut a = BitVec::from_bytes(&[0b10100000, 0b00010010, 0b10010010, 0b00110011, 0b11110011]);

    let b = a.split_off(32);

    assert_eq!(a.len(), 32);
    assert_eq!(b.len(), 8);

    assert!(a.eq_vec(&[true, false, true, false, false, false, false, false,
                       false, false, false, true, false, false, true, false,
                       true, false, false, true, false, false, true, false,
                       false, false, true, true, false, false, true, true]));
    assert!(b.eq_vec(&[true, true, true, true, false, false, true, true]));

    // Don't split at block boundary
    let mut a = BitVec::from_bytes(&[0b10100000, 0b00010010, 0b10010010, 0b00110011,
                                     0b01101011, 0b10101101]);

    let b = a.split_off(13);

    assert_eq!(a.len(), 13);
    assert_eq!(b.len(), 35);

    assert!(a.eq_vec(&[true, false, true, false, false, false, false, false,
                       false, false, false, true, false]));
    assert!(b.eq_vec(&[false, true, false, true, false, false, true, false,
                       false, true, false, false, false, true, true, false,
                       false, true, true, false, true, true, false, true,
                       false, true, true,  true, false, true, false, true,
                       true, false, true]));
}

mod bench {
    use std::collections::BitVec;
    use std::u32;
    use std::__rand::{Rng, thread_rng, ThreadRng};

    use test::{Bencher, black_box};

    const BENCH_BITS : usize = 1 << 14;

    fn rng() -> ThreadRng {
        thread_rng()
    }

    #[bench]
    fn bench_usize_small(b: &mut Bencher) {
        let mut r = rng();
        let mut bit_vec = 0 as usize;
        b.iter(|| {
            for _ in 0..100 {
                bit_vec |= 1 << ((r.next_u32() as usize) % u32::BITS);
            }
            black_box(&bit_vec);
        });
    }

    #[bench]
    fn bench_bit_set_big_fixed(b: &mut Bencher) {
        let mut r = rng();
        let mut bit_vec = BitVec::from_elem(BENCH_BITS, false);
        b.iter(|| {
            for _ in 0..100 {
                bit_vec.set((r.next_u32() as usize) % BENCH_BITS, true);
            }
            black_box(&bit_vec);
        });
    }

    #[bench]
    fn bench_bit_set_big_variable(b: &mut Bencher) {
        let mut r = rng();
        let mut bit_vec = BitVec::from_elem(BENCH_BITS, false);
        b.iter(|| {
            for _ in 0..100 {
                bit_vec.set((r.next_u32() as usize) % BENCH_BITS, r.gen());
            }
            black_box(&bit_vec);
        });
    }

    #[bench]
    fn bench_bit_set_small(b: &mut Bencher) {
        let mut r = rng();
        let mut bit_vec = BitVec::from_elem(u32::BITS, false);
        b.iter(|| {
            for _ in 0..100 {
                bit_vec.set((r.next_u32() as usize) % u32::BITS, true);
            }
            black_box(&bit_vec);
        });
    }

    #[bench]
    fn bench_bit_vec_big_union(b: &mut Bencher) {
        let mut b1 = BitVec::from_elem(BENCH_BITS, false);
        let b2 = BitVec::from_elem(BENCH_BITS, false);
        b.iter(|| {
            b1.union(&b2)
        })
    }

    #[bench]
    fn bench_bit_vec_small_iter(b: &mut Bencher) {
        let bit_vec = BitVec::from_elem(u32::BITS, false);
        b.iter(|| {
            let mut sum = 0;
            for _ in 0..10 {
                for pres in &bit_vec {
                    sum += pres as usize;
                }
            }
            sum
        })
    }

    #[bench]
    fn bench_bit_vec_big_iter(b: &mut Bencher) {
        let bit_vec = BitVec::from_elem(BENCH_BITS, false);
        b.iter(|| {
            let mut sum = 0;
            for pres in &bit_vec {
                sum += pres as usize;
            }
            sum
        })
    }
}
