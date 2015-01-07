// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! HTML Escaping
//!
//! This module contains one unit-struct which can be used to HTML-escape a
//! string of text (for use in a format string).

use std::fmt;

/// Wrapper struct which will emit the HTML-escaped version of the contained
/// string when passed to a format string.
pub struct Escape<'a>(pub &'a str);

//NOTE(stage0): remove impl after snapshot
#[cfg(stage0)]
impl<'a> fmt::Show for Escape<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::String::fmt(self, f)
    }
}

impl<'a> fmt::String for Escape<'a> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        // Because the internet is always right, turns out there's not that many
        // characters to escape: http://stackoverflow.com/questions/7381974
        let Escape(s) = *self;
        let pile_o_bits = s.as_slice();
        let mut last = 0;
        for (i, ch) in s.bytes().enumerate() {
            match ch as char {
                '<' | '>' | '&' | '\'' | '"' => {
                    try!(fmt.write_str(pile_o_bits.slice(last, i)));
                    let s = match ch as char {
                        '>' => "&gt;",
                        '<' => "&lt;",
                        '&' => "&amp;",
                        '\'' => "&#39;",
                        '"' => "&quot;",
                        _ => unreachable!()
                    };
                    try!(fmt.write_str(s));
                    last = i + 1;
                }
                _ => {}
            }
        }

        if last < s.len() {
            try!(fmt.write_str(pile_o_bits.slice_from(last)));
        }
        Ok(())
    }
}
