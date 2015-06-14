// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Check that the various ways of getting to a reference to a vec (both sized
// and unsized) work properly.


const aa: [isize; 3] = [1, 2, 3];
const ab: &'static [isize; 3] = &aa;
const ac: &'static [isize] = ab;
const ad: &'static [isize] = &aa;
const ae: &'static [isize; 3] = &[1, 2, 3];
const af: &'static [isize] = &[1, 2, 3];

static ca: isize = aa[0];
static cb: isize = ab[1];
static cc: isize = ac[2];
static cd: isize = ad[0];
static ce: isize = ae[1];
static cf: isize = af[2];

fn main () {
    let b: &[isize] = &[1, 2, 3];
    assert_eq!(ac, b);
    assert_eq!(ad, b);
    assert_eq!(af, b);

    assert_eq!(ca, 1);
    assert_eq!(cb, 2);
    assert_eq!(cc, 3);
    assert_eq!(cd, 1);
    assert_eq!(ce, 2);
    assert_eq!(cf, 3);
}
