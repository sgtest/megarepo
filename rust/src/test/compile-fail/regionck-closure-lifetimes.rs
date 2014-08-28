// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

fn env<'a>(blk: |p: ||: 'a|) {
    // Test that the closure here cannot be assigned
    // the lifetime `'a`, which outlives the current
    // block.

    let mut state = 0i;
    let statep = &mut state;
    blk(|| *statep = 1i); //~ ERROR captured variable `statep` does not outlive
}

fn no_env_no_for<'a>(blk: |p: |||: 'a) {
    // Test that a closure with no free variables CAN
    // outlive the block in which it is created.

    blk(|| ())
}

fn repeating_loop() {
    // Test that the closure cannot be created within `loop` loop and
    // called without, even though the state that it closes over is
    // external to the loop.

    let closure;
    let state = 0i;

    loop {
        closure = || state; //~ ERROR cannot infer
        break;
    }

    closure();
}

fn repeating_while() {
    // Test that the closure cannot be created within `while` loop and
    // called without, even though the state that it closes over is
    // external to the loop.

    let closure;
    let state = 0i;

    while true {
        closure = || state; //~ ERROR cannot infer
        break;
    }

    closure();
}

fn main() {}
