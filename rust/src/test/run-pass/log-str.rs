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
    let act = sys::log_str(&~[1, 2, 3]);
    fail_unless!(~"~[1, 2, 3]" == act);

    let act = fmt!("%?/%6?", ~[1, 2, 3], ~"hi");
    fail_unless!(act == ~"~[1, 2, 3]/ ~\"hi\"");
}
