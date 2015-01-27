// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::old_io::println;
use std::sync::mpsc::channel;
use std::thread::Thread;

pub fn main() {
    let (tx, rx) = channel();

    tx.send("hello, world").unwrap();

    Thread::scoped(move|| {
        println(rx.recv().unwrap());
    }).join().ok().unwrap();
}
