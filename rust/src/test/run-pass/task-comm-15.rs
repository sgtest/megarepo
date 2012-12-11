// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// xfail-fast
// xfail-win32
#[legacy_modes];

extern mod std;

fn start(c: pipes::Chan<int>, i0: int) {
    let mut i = i0;
    while i > 0 {
        c.send(0);
        i = i - 1;
    }
}

fn main() {
    // Spawn a task that sends us back messages. The parent task
    // is likely to terminate before the child completes, so from
    // the child's point of view the receiver may die. We should
    // drop messages on the floor in this case, and not crash!
    let (ch, p) = pipes::stream();
    task::spawn(|move ch| start(ch, 10));
    p.recv();
}
