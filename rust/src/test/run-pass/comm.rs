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

use std::task;

pub fn main() {
    let (tx, rx) = channel();
    let _t = task::spawn(proc() { child(&tx) });
    let y = rx.recv();
    println!("received");
    println!("{:?}", y);
    assert_eq!(y, 10);
}

fn child(c: &Sender<int>) {
    println!("sending");
    c.send(10);
    println!("value sent");
}
