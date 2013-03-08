// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

pub fn main() {
    fail_unless!((~[0, 1]).to_str() == ~"[0, 1]");
    fail_unless!((&[1, 2]).to_str() == ~"[1, 2]");
    fail_unless!((@[2, 3]).to_str() == ~"[2, 3]");

    let foo = ~[3, 4];
    let bar = &[4, 5];
    let baz = @[5, 6];

    fail_unless!(foo.to_str() == ~"[3, 4]");
    fail_unless!(bar.to_str() == ~"[4, 5]");
    fail_unless!(baz.to_str() == ~"[5, 6]");

}
