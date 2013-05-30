// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::sys;

#[packed]
struct S4(u8,[u8, .. 3]);

#[packed]
struct S5(u8, u32);

#[packed]
struct S13_str(i64, f32, u8, ~str);

enum Foo {
    Bar = 1,
    Baz = 2
}

#[packed]
struct S3_Foo(u8, u16, Foo);

#[packed]
struct S7_Option(f32, u8, u16, Option<@mut f64>);

fn main() {
    assert_eq!(sys::size_of::<S4>(), 4);

    assert_eq!(sys::size_of::<S5>(), 5);

    assert_eq!(sys::size_of::<S13_str>(),
               13 + sys::size_of::<~str>());

    assert_eq!(sys::size_of::<S3_Foo>(),
               3 + sys::size_of::<Foo>());

    assert_eq!(sys::size_of::<S7_Option>(),
              7 + sys::size_of::<Option<@mut f64>>());
}
