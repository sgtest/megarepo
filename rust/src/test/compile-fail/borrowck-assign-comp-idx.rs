// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

struct Point {
    x: int,
    y: int,
}

fn a() {
    let mut p = vec!(1);

    // Create an immutable pointer into p's contents:
    let q: &int = p.get(0);

    *p.get_mut(0) = 5; //~ ERROR cannot borrow

    println!("{}", *q);
}

fn borrow(_x: &[int], _f: ||) {}

fn b() {
    // here we alias the mutable vector into an imm slice and try to
    // modify the original:

    let mut p = vec!(1);

    borrow(
        p.as_slice(),
        || *p.get_mut(0) = 5); //~ ERROR cannot borrow `p` as mutable
}

fn c() {
    // Legal because the scope of the borrow does not include the
    // modification:
    let mut p = vec!(1);
    borrow(p.as_slice(), ||{});
    *p.get_mut(0) = 5;
}

fn main() {
}
