// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


type compare<'a, T> = |T, T|: 'a -> bool;

fn test_generic<T:Clone>(expected: T, eq: compare<T>) {
    let actual: T = { expected.clone() };
    assert!((eq(expected, actual)));
}

fn test_vec() {
    fn compare_vec(v1: Box<int>, v2: Box<int>) -> bool { return v1 == v2; }
    test_generic::<Box<int>>(box 1, compare_vec);
}

pub fn main() { test_vec(); }
