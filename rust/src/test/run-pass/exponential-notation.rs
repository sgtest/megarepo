// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::num::strconv::ExponentFormat::{ExpBin, ExpDec};
use std::num::strconv::SignificantDigits::DigMax;
use std::num::strconv::SignFormat::{SignAll, SignNeg};
use std::num::strconv::float_to_str_common as to_string;

macro_rules! t {
    ($a:expr, $b:expr) => { { let (r, _) = $a; assert_eq!(r, $b.to_string()); } }
}

pub fn main() {
    // Basic usage
    t!(to_string(1.2345678e-5f64, 10u, true, SignNeg, DigMax(6), ExpDec, false),
             "1.234568e-5");

    // Hexadecimal output
    t!(to_string(7.281738281250e+01f64, 16u, true, SignAll, DigMax(6), ExpBin, false),
              "+1.2345p+6");
    t!(to_string(-1.777768135071e-02f64, 16u, true, SignAll, DigMax(6), ExpBin, false),
             "-1.2345p-6");

    // Some denormals
    t!(to_string(4.9406564584124654e-324f64, 10u, true, SignNeg, DigMax(6), ExpBin, false),
             "1p-1074");
    t!(to_string(2.2250738585072009e-308f64, 10u, true, SignNeg, DigMax(6), ExpBin, false),
             "1p-1022");
}
