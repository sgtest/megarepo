// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// ignore-fast

extern crate extra;

use std::task;

fn start(c: &Chan<Chan<~str>>) {
    let (p, ch) = Chan::new();
    c.send(ch);

    let mut a;
    let mut b;
    a = p.recv();
    assert!(a == ~"A");
    error!("{:?}", a);
    b = p.recv();
    assert!(b == ~"B");
    error!("{:?}", b);
}

pub fn main() {
    let (p, ch) = Chan::new();
    let _child = task::spawn(proc() { start(&ch) });

    let mut c = p.recv();
    c.send(~"A");
    c.send(~"B");
    task::deschedule();
}
