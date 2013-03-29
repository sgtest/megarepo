// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.



// -*- rust -*-
extern mod std;

fn test1() {
    let mut s: ~str = ~"hello";
    s += ~"world";
    debug!(s.clone());
    assert!((s[9] == 'd' as u8));
}

fn test2() {
    // This tests for issue #163

    let ff: ~str = ~"abc";
    let a: ~str = ff + ~"ABC" + ff;
    let b: ~str = ~"ABC" + ff + ~"ABC";
    debug!(a.clone());
    debug!(b.clone());
    assert!((a == ~"abcABCabc"));
    assert!((b == ~"ABCabcABC"));
}

pub fn main() { test1(); test2(); }
