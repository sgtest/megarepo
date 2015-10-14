// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Can't use empty braced struct as enum pattern

#![feature(braced_empty_structs)]

struct Empty1 {}

enum E {
    Empty2 {}
}

fn main() {
    let e1 = Empty1 {};
    let e2 = E::Empty2 {};

    // Rejected by parser as yet
    // match e1 {
    //     Empty1() => () // ERROR unresolved enum variant, struct or const `Empty1`
    // }
    match e1 {
        Empty1(..) => () //~ ERROR unresolved enum variant, struct or const `Empty1`
    }
    // Issue #28692
    // match e2 {
    //     E::Empty2() => () // ERROR unresolved enum variant, struct or const `Empty2`
    // }
    // match e2 {
    //     E::Empty2(..) => () // ERROR unresolved enum variant, struct or const `Empty2`
    // }
}
