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

use std::task;

pub fn main() { println!("===== WITHOUT THREADS ====="); test00(); }

fn test00_start(ch: &Sender<int>, message: int, count: int) {
    println!("Starting test00_start");
    let mut i: int = 0;
    while i < count {
        println!("Sending Message");
        ch.send(message + 0);
        i = i + 1;
    }
    println!("Ending test00_start");
}

fn test00() {
    let number_of_tasks: int = 16;
    let number_of_messages: int = 4;

    println!("Creating tasks");

    let (tx, rx) = channel();

    let mut i: int = 0;

    // Create and spawn tasks...
    let mut results = Vec::new();
    while i < number_of_tasks {
        let tx = tx.clone();
        let mut builder = task::task();
        results.push(builder.future_result());
        builder.spawn({
            let i = i;
            proc() {
                test00_start(&tx, i, number_of_messages)
            }
        });
        i = i + 1;
    }

    // Read from spawned tasks...
    let mut sum = 0;
    for _r in results.iter() {
        i = 0;
        while i < number_of_messages {
            let value = rx.recv();
            sum += value;
            i = i + 1;
        }
    }

    // Join spawned tasks...
    for r in results.iter() { r.recv(); }

    println!("Completed: Final number is: ");
    println!("{:?}", sum);
    // assert (sum == (((number_of_tasks * (number_of_tasks - 1)) / 2) *
    //       number_of_messages));
    assert_eq!(sum, 480);
}
