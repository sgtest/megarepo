// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//buggy.rs
extern mod std;
use std::map::HashMap;
use std::map;

fn main() {
    let buggy_map :HashMap<uint, &uint> =
      HashMap::<uint, &uint>();
    buggy_map.insert(42, &*~1); //~ ERROR illegal borrow

    // but it is ok if we use a temporary
    let tmp = ~2;
    buggy_map.insert(43, &*tmp);
}
