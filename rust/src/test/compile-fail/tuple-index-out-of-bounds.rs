// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(tuple_indexing)]

struct Point(int, int);

fn main() {
    let origin = Point(0, 0);
    origin.0;
    origin.1;
    origin.2;
    //~^ ERROR attempted out-of-bounds tuple index `2` on type `Point`
    let tuple = (0i, 0i);
    tuple.0;
    tuple.1;
    tuple.2;
    //~^ ERROR attempted out-of-bounds tuple index `2` on type `(int,int)`
}
