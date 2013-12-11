// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// xfail-fast
// aux-build:crateresolve8-1.rs

#[pkgid="crateresolve8#0.1"];

extern mod crateresolve8(vers = "0.1", package_id="crateresolve8#0.1");
//extern mod crateresolve8(vers = "0.1");

pub fn main() {
    assert_eq!(crateresolve8::f(), 20);
}
