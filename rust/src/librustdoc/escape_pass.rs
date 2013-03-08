// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Escapes text sequences

use pass::Pass;
use text_pass;

use core::str;

pub fn mk_pass() -> Pass {
    text_pass::mk_pass(~"escape", escape)
}

fn escape(s: &str) -> ~str {
    str::replace(s, ~"\\", ~"\\\\")
}

#[test]
fn should_escape_backslashes() {
    let s = ~"\\n";
    let r = escape(s);
    fail_unless!(r == ~"\\\\n");
}
