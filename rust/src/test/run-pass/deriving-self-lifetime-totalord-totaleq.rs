// xfail-test FIXME #6257

// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::cmp::{Less,Equal,Greater};

#[deriving(TotalEq,TotalOrd)]
struct A<'self> {
    x: &'self int
}

fn main() {
    let a = A { x: &1 }, b = A { x: &2 };

    assert!(a.equals(&a));
    assert!(b.equals(&b));


    assert_eq!(a.cmp(&a), Equal);
    assert_eq!(b.cmp(&b), Equal);

    assert_eq!(a.cmp(&b), Less);
    assert_eq!(b.cmp(&a), Greater);
}
