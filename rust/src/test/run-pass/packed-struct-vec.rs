// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
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
#[derive(Copy, Clone, PartialEq, Debug)]
struct Foo {
    bar: u8,
    baz: u64
}

pub fn main() {
    let foos = [Foo { bar: 1, baz: 2 }; 10];

    assert_eq!(mem::size_of::<[Foo; 10]>(), 90);

    for i in 0..10 {
        assert_eq!(foos[i], Foo { bar: 1, baz: 2});
    }

    for &foo in &foos {
        assert_eq!(foo, Foo { bar: 1, baz: 2 });
    }
}
