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

fn grow(v: &mut ~[int]) { *v += ~[1]; }

pub fn main() {
    let mut v: ~[int] = ~[];
    grow(&mut v);
    grow(&mut v);
    grow(&mut v);
    let len = vec::len::<int>(v);
    log(debug, len);
    fail_unless!((len == 3 as uint));
}
