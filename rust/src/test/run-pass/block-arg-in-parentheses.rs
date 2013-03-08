// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

fn w_semi(v: ~[int]) -> int {
    // the semicolon causes compiler not to
    // complain about the ignored return value:
    do vec::foldl(0, v) |x,y| { x+*y };
    -10
}

fn w_paren1(v: ~[int]) -> int {
    (do vec::foldl(0, v) |x,y| { x+*y }) - 10
}

fn w_paren2(v: ~[int]) -> int {
    (do vec::foldl(0, v) |x,y| { x+*y} - 10)
}

fn w_ret(v: ~[int]) -> int {
    return do vec::foldl(0, v) |x,y| { x+*y } - 10;
}

pub fn main() {
    fail_unless!(w_semi(~[0, 1, 2, 3]) == -10);
    fail_unless!(w_paren1(~[0, 1, 2, 3]) == -4);
    fail_unless!(w_paren2(~[0, 1, 2, 3]) == -4);
    fail_unless!(w_ret(~[0, 1, 2, 3]) == -4);
}

