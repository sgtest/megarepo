// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

struct pair<A,B> {
    a: A, b: B
}

fn f<A:Copy + &static>(a: A, b: u16) -> @fn() -> (A, u16) {
    let result: @fn() -> (A, u16) = || (a, b);
    result
}

pub fn main() {
    let (a, b) = f(22_u64, 44u16)();
    debug!("a=%? b=%?", a, b);
    fail_unless!(a == 22u64);
    fail_unless!(b == 44u16);
}
