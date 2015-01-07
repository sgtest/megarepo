// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(slicing_syntax)]

fn assert_repr_eq<T: std::fmt::Show>(obj : T, expected : String) {
    assert_eq!(expected, format!("{:?}", obj));
}

pub fn main() {
    let abc = [1i, 2, 3];
    let tf = [true, false];
    let x  = [(), ()];
    let slice = &x[0..1];

    assert_repr_eq(&abc[], "[1i, 2i, 3i]".to_string());
    assert_repr_eq(&tf[], "[true, false]".to_string());
    assert_repr_eq(&x[], "[(), ()]".to_string());
    assert_repr_eq(slice, "[()]".to_string());
    assert_repr_eq(&x[], "[(), ()]".to_string());
}
