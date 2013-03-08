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
// aux-build:cci_class_cast.rs
extern mod cci_class_cast;
use core::to_str::ToStr;
use cci_class_cast::kitty::*;

fn print_out(thing: @ToStr, expected: ~str) {
  let actual = thing.to_str();
  debug!("%s", actual);
  fail_unless!((actual == expected));
}

pub fn main() {
  let nyan : @ToStr = @cat(0u, 2, ~"nyan") as @ToStr;
  print_out(nyan, ~"nyan");
}

