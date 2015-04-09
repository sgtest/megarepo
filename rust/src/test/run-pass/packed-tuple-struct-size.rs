// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.



use std::mem;

#[repr(packed)]
struct S4(u8,[u8;  3]);

#[repr(packed)]
struct S5(u8, u32);

#[repr(packed)]
struct S13(i64, f32, u8);

enum Foo {
    Bar = 1,
    Baz = 2
}

#[repr(packed)]
struct S3_Foo(u8, u16, Foo);

#[repr(packed)]
struct S7_Option(f32, u8, u16, Option<Box<f64>>);

pub fn main() {
    assert_eq!(mem::size_of::<S4>(), 4);

    assert_eq!(mem::size_of::<S5>(), 5);

    assert_eq!(mem::size_of::<S13>(), 13);

    assert_eq!(mem::size_of::<S3_Foo>(),
               3 + mem::size_of::<Foo>());

    assert_eq!(mem::size_of::<S7_Option>(),
              7 + mem::size_of::<Option<Box<f64>>>());
}
