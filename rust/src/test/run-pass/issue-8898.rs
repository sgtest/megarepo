// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[feature(managed_boxes)];

fn assert_repr_eq<T>(obj : T, expected : ~str) {
    assert_eq!(expected, format!("{:?}", obj));
}

pub fn main() {
    let abc = [1, 2, 3];
    let tf = [true, false];
    let x  = [(), ()];
    let slice = x.slice(0,1);
    let z = @x;

    assert_repr_eq(abc, ~"[1, 2, 3]");
    assert_repr_eq(tf, ~"[true, false]");
    assert_repr_eq(x, ~"[(), ()]");
    assert_repr_eq(slice, ~"&[()]");
    assert_repr_eq(&x, ~"&[(), ()]");
    assert_repr_eq(z, ~"@[(), ()]");
}
