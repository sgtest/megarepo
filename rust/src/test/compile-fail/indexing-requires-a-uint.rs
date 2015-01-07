// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Make sure that indexing an array is only valid with a `uint`, not any other
// integral type.

fn main() {
    fn bar<T>(_: T) {}
    [0][0u8]; //~ ERROR: the trait `core::ops::Index<u8>` is not implemented
    //~^ ERROR: the trait `core::ops::Index<u8>` is not implemented

    [0][0]; // should infer to be a uint

    let i = 0;      // i is an IntVar
    [0][i];         // i should be locked to uint
    bar::<int>(i);  // i should not be re-coerced back to an int
    //~^ ERROR: mismatched types
}

