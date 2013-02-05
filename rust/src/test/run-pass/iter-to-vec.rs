// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

pub fn main() {
    assert [1u, 3u].to_vec() == ~[1u, 3u];
    let e: ~[uint] = ~[];
    assert e.to_vec() == ~[];
    assert None::<uint>.to_vec() == ~[];
    assert Some(1u).to_vec() == ~[1u];
    assert Some(2u).to_vec() == ~[2u];
}
