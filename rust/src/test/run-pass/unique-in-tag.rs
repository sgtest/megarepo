// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

extern crate debug;

fn test1() {
    enum bar { u(Box<int>), w(int), }

    let x = u(box 10);
    assert!(match x {
      u(a) => {
        println!("{:?}", a);
        *a
      }
      _ => { 66 }
    } == 10);
}

pub fn main() {
    test1();
}
