// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// xfail-pretty
#[path = "mod-merge-hack-template.rs"]
#[merge = "mod-merge-hack-inst.rs"]
mod myint32;

pub fn main() {
    fail_unless!(myint32::bits == 32);
    fail_unless!(myint32::min(10, 20) == 10);
}
