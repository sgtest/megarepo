// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Exercise the unused_mut attribute in some positive and negative cases

#[allow(dead_assignment)];
#[allow(unused_variable)];
#[deny(unused_mut)];

fn main() {
    // negative cases
    let mut a = 3; //~ ERROR: variable does not need to be mutable
    let mut a = 2; //~ ERROR: variable does not need to be mutable
    let mut b = 3; //~ ERROR: variable does not need to be mutable
    let mut a = ~[3]; //~ ERROR: variable does not need to be mutable

    // positive cases
    let mut a = 2;
    a = 3;
    let mut a = ~[];
    a.push(3);
    let mut a = ~[];
    do callback {
        a.push(3);
    }
}

fn callback(f: &fn()) {}

// make sure the lint attribute can be turned off
#[allow(unused_mut)]
fn foo(mut a: int) {
    let mut a = 3;
    let mut b = ~[2];
}
