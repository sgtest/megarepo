// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Test that a custom deref with a fat pointer return type does not ICE

pub struct Arr {
    ptr: Box<[uint]>
}

impl Deref<[uint]> for Arr {
    fn deref(&self) -> &[uint] {
        panic!();
    }
}

impl DerefMut<[uint]> for Arr {
    fn deref_mut(&mut self) -> &mut [uint] {
        &mut *self.ptr
    }
}

pub fn foo(arr: &mut Arr) {
    let x: &mut [uint] = &mut **arr;
    assert!(x[0] == 1);
    assert!(x[1] == 2);
    assert!(x[2] == 3);
}

fn main() {
    let mut a = Arr { ptr: box [1, 2, 3] };
    foo(&mut a);
}
