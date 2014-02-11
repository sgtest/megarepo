// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// ignore-test linked failure

// A port of task-killjoin to use a class with a dtor to manage
// the join.

use std::cell::Cell;
use std::comm::*;
use std::ptr;
use std::task;

struct notify {
    ch: Chan<bool>,
    v: @Cell<bool>,
}

#[unsafe_destructor]
impl Drop for notify {
    fn drop(&mut self) {
        unsafe {
            error!("notify: task=%? v=%x unwinding=%b b=%b",
                   0,
                   ptr::to_unsafe_ptr(&(*(self.v))) as uint,
                   task::failing(),
                   *(self.v));
            let b = *(self.v);
            self.ch.send(b);
        }
    }
}

fn notify(ch: Chan<bool>, v: @Cell<bool>) -> notify {
    notify {
        ch: ch,
        v: v
    }
}

fn joinable(f: proc()) -> Port<bool> {
    fn wrapper(c: Chan<bool>, f: ||) {
        let b = @Cell::new(false);
        error!("wrapper: task=%? allocated v=%x",
               0,
               ptr::to_unsafe_ptr(&b) as uint);
        let _r = notify(c, b);
        f();
        *b = true;
    }
    let (p, c) = stream();
    task::spawn_unlinked(proc() {
        let ccc = c;
        wrapper(ccc, f)
    });
    p
}

fn join(port: Port<bool>) -> bool {
    port.recv()
}

fn supervised() {
    // Deschedule to make sure the supervisor joins before we
    // fail. This is currently not needed because the supervisor
    // runs first, but I can imagine that changing.
    error!("supervised task=%?", 0);
    task::deschedule();
    fail!();
}

fn supervisor() {
    error!("supervisor task=%?", 0);
    let t = joinable(supervised);
    join(t);
}

pub fn main() {
    join(joinable(supervisor));
}
