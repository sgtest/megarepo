// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// ignore-test
// This checks that preemption works.

// note: halfway done porting to modern rust
use std::comm;

fn starve_main(alive: Receiver<isize>) {
    println!("signalling main");
    alive.recv();
    println!("starving main");
    let mut i: isize = 0;
    loop { i += 1; }
}

pub fn main() {
    let (port, chan) = stream();

    println!("main started");
    spawn(move|| {
        starve_main(port);
    });
    let mut i: isize = 0;
    println!("main waiting for alive signal");
    chan.send(i);
    println!("main got alive signal");
    while i < 50 { println!("main iterated"); i += 1; }
    println!("main completed");
}
