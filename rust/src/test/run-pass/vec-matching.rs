// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


#![feature(advanced_slice_patterns)]
#![feature(slice_patterns)]
#![feature(rustc_attrs)]

#[rustc_mir]
fn a() {
    let x = [1];
    match x {
        [a] => {
            assert_eq!(a, 1);
        }
    }
}

#[rustc_mir]
fn b() {
    let x = [1, 2, 3];
    match x {
        [a, b, c..] => {
            assert_eq!(a, 1);
            assert_eq!(b, 2);
            let expected: &[_] = &[3];
            assert_eq!(c, expected);
        }
    }
    match x {
        [a.., b, c] => {
            let expected: &[_] = &[1];
            assert_eq!(a, expected);
            assert_eq!(b, 2);
            assert_eq!(c, 3);
        }
    }
    match x {
        [a, b.., c] => {
            assert_eq!(a, 1);
            let expected: &[_] = &[2];
            assert_eq!(b, expected);
            assert_eq!(c, 3);
        }
    }
    match x {
        [a, b, c] => {
            assert_eq!(a, 1);
            assert_eq!(b, 2);
            assert_eq!(c, 3);
        }
    }
}


#[rustc_mir]
fn b_slice() {
    let x : &[_] = &[1, 2, 3];
    match x {
        &[a, b, ref c..] => {
            assert_eq!(a, 1);
            assert_eq!(b, 2);
            let expected: &[_] = &[3];
            assert_eq!(c, expected);
        }
        _ => unreachable!()
    }
    match x {
        &[ref a.., b, c] => {
            let expected: &[_] = &[1];
            assert_eq!(a, expected);
            assert_eq!(b, 2);
            assert_eq!(c, 3);
        }
        _ => unreachable!()
    }
    match x {
        &[a, ref b.., c] => {
            assert_eq!(a, 1);
            let expected: &[_] = &[2];
            assert_eq!(b, expected);
            assert_eq!(c, 3);
        }
        _ => unreachable!()
    }
    match x {
        &[a, b, c] => {
            assert_eq!(a, 1);
            assert_eq!(b, 2);
            assert_eq!(c, 3);
        }
        _ => unreachable!()
    }
}

#[rustc_mir]
fn c() {
    let x = [1];
    match x {
        [2, ..] => panic!(),
        [..] => ()
    }
}

#[rustc_mir]
fn d() {
    let x = [1, 2, 3];
    let branch = match x {
        [1, 1, ..] => 0,
        [1, 2, 3, ..] => 1,
        [1, 2, ..] => 2,
        _ => 3
    };
    assert_eq!(branch, 1);
}

#[rustc_mir]
fn e() {
    let x: &[isize] = &[1, 2, 3];
    let a = match *x {
        [1, 2] => 0,
        [..] => 1,
    };

    assert_eq!(a, 1);

    let b = match *x {
        [2, ..] => 0,
        [1, 2, ..] => 1,
        [_] => 2,
        [..] => 3
    };

    assert_eq!(b, 1);


    let c = match *x {
        [_, _, _, _, ..] => 0,
        [1, 2, ..] => 1,
        [_] => 2,
        [..] => 3
    };

    assert_eq!(c, 1);
}

pub fn main() {
    a();
    b();
    b_slice();
    c();
    d();
    e();
}
