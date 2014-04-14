// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


#![allow(dead_assignment)]

use std::task;

pub fn main() { test00(); }

fn test00_start(c: &Sender<int>, start: int,
                number_of_messages: int) {
    let mut i: int = 0;
    while i < number_of_messages { c.send(start + i); i += 1; }
}

fn test00() {
    let mut r: int = 0;
    let mut sum: int = 0;
    let (tx, rx) = channel();
    let number_of_messages: int = 10;

    let tx2 = tx.clone();
    task::spawn(proc() {
        test00_start(&tx2, number_of_messages * 0, number_of_messages);
    });
    let tx2 = tx.clone();
    task::spawn(proc() {
        test00_start(&tx2, number_of_messages * 1, number_of_messages);
    });
    let tx2 = tx.clone();
    task::spawn(proc() {
        test00_start(&tx2, number_of_messages * 2, number_of_messages);
    });
    let tx2 = tx.clone();
    task::spawn(proc() {
        test00_start(&tx2, number_of_messages * 3, number_of_messages);
    });

    let mut i: int = 0;
    while i < number_of_messages {
        r = rx.recv();
        sum += r;
        r = rx.recv();
        sum += r;
        r = rx.recv();
        sum += r;
        r = rx.recv();
        sum += r;
        i += 1;
    }

    assert_eq!(sum, number_of_messages * 4 * (number_of_messages * 4 - 1) / 2);
}
