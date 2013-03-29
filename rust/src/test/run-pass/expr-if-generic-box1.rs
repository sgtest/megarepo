// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.




// -*- rust -*-
type compare<T> = @fn(@T, @T) -> bool;

fn test_generic<T>(expected: @T, not_expected: @T, eq: compare<T>) {
    let actual: @T = if true { expected } else { not_expected };
    assert!((eq(expected, actual)));
}

fn test_box() {
    fn compare_box(b1: @bool, b2: @bool) -> bool { return *b1 == *b2; }
    test_generic::<bool>(@true, @false, compare_box);
}

pub fn main() { test_box(); }
