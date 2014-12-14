// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::os;
use std::uint;

// A simple implementation of parfib. One subtree is found in a new
// task and communicated over a oneshot pipe, the other is found
// locally. There is no sequential-mode threshold.

fn parfib(n: uint) -> uint {
    if(n == 0 || n == 1) {
        return 1;
    }

    let (tx, rx) = channel();
    spawn(move|| {
        tx.send(parfib(n-1));
    });
    let m2 = parfib(n-2);
    return (rx.recv() + m2);
}

fn main() {

    let args = os::args();
    let args = args.as_slice();
    let n = if args.len() == 2 {
        from_str::<uint>(args[1].as_slice()).unwrap()
    } else {
        10
    };

    parfib(n);

}
