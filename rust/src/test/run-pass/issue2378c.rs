// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// xfail-test -- #2378 unfixed 
// aux-build:issue2378a.rs
// aux-build:issue2378b.rs

use issue2378a;
use issue2378b;

use issue2378a::{just, methods};
use issue2378b::{methods};

pub fn main() {
    let x = {a: just(3), b: just(5)};
    assert!(x[0u] == (3, 5));
}
