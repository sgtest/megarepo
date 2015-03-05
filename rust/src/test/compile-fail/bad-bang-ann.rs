// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Tests that a function with a ! annotation always actually fails

fn bad_bang(i: usize) -> ! { //~ ERROR computation may converge in a function marked as diverging
    if i < 0 { } else { panic!(); }
}

fn main() { bad_bang(5); }
