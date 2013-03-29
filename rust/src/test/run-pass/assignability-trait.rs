// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Tests that type assignability is used to search for instances when
// making method calls, but only if there aren't any matches without
// it.

trait iterable<A> {
    fn iterate(&self, blk: &fn(x: &A) -> bool);
}

impl<'self,A> iterable<A> for &'self [A] {
    fn iterate(&self, f: &fn(x: &A) -> bool) {
        for vec::each(*self) |e| {
            if !f(e) { break; }
        }
    }
}

impl<A> iterable<A> for ~[A] {
    fn iterate(&self, f: &fn(x: &A) -> bool) {
        for vec::each(*self) |e| {
            if !f(e) { break; }
        }
    }
}

fn length<A, T: iterable<A>>(x: T) -> uint {
    let mut len = 0;
    for x.iterate() |_y| { len += 1 }
    return len;
}

pub fn main() {
    let x = ~[0,1,2,3];
    // Call a method
    for x.iterate() |y| { assert!(x[*y] == *y); }
    // Call a parameterized function
    assert!(length(x.clone()) == vec::len(x));
    // Call a parameterized function, with type arguments that require
    // a borrow
    assert!(length::<int, &[int]>(x) == vec::len(x));

    // Now try it with a type that *needs* to be borrowed
    let z = [0,1,2,3];
    // Call a method
    for z.iterate() |y| { assert!(z[*y] == *y); }
    // Call a parameterized function
    assert!(length::<int, &[int]>(z) == vec::len(z));
}
