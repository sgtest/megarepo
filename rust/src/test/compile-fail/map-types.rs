// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

extern mod std;
use std::map;
use std::map::HashMap;
use std::map::Map;

// Test that trait types printed in error msgs include the type arguments.

fn main() {
    let x: Map<~str,~str> = map::HashMap::<~str,~str>() as Map::<~str,~str>;
    let y: Map<uint,~str> = x;
    //~^ ERROR mismatched types: expected `@std::map::Map<uint,~str>`
}
