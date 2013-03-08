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
// aux-build:cci_class_2.rs
extern mod cci_class_2;
use cci_class_2::kitties::*;

pub fn main() {
  let nyan : cat = cat(52u, 99);
  let kitty = cat(1000u, 2);
  fail_unless!((nyan.how_hungry == 99));
  fail_unless!((kitty.how_hungry == 2));
  nyan.speak();
}
