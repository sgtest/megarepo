// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// run-pass
#![allow(unused_mut)]
// The logic for parsing Kleene operators in macros has a special case to disambiguate `?`.
// Specifically, `$(pat)?` is the ZeroOrOne operator whereas `$(pat)?+` or `$(pat)?*` are the
// ZeroOrMore and OneOrMore operators using `?` as a separator. These tests are intended to
// exercise that logic in the macro parser.
//
// Moreover, we also throw in some tests for using a separator with `?`, which is meaningless but
// included for consistency with `+` and `*`.
//
// This test focuses on non-error cases and making sure the correct number of repetitions happen.

// edition:2018

#![feature(macro_at_most_once_rep)]

macro_rules! foo {
    ($($a:ident)? ; $num:expr) => { {
        let mut x = 0;

        $(
            x += $a;
         )?

        assert_eq!(x, $num);
    } }
}

pub fn main() {
    let a = 1;

    // accept 0 or 1 repetitions
    foo!( ; 0);
    foo!(a ; 1);
}
