// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Implementation of running at_exit routines
//!
//! Documentation can be found on the `rt::at_exit` function.

use core::prelude::*;

use alloc::boxed::Box;
use collections::MutableSeq;
use collections::vec::Vec;
use core::atomic;
use core::mem;

use exclusive::Exclusive;

type Queue = Exclusive<Vec<proc():Send>>;

static mut QUEUE: atomic::AtomicUint = atomic::INIT_ATOMIC_UINT;
static mut RUNNING: atomic::AtomicBool = atomic::INIT_ATOMIC_BOOL;

pub fn init() {
    let state: Box<Queue> = box Exclusive::new(Vec::new());
    unsafe {
        rtassert!(!RUNNING.load(atomic::SeqCst));
        assert!(QUEUE.swap(mem::transmute(state), atomic::SeqCst) == 0);
    }
}

pub fn push(f: proc():Send) {
    unsafe {
        // Note that the check against 0 for the queue pointer is not atomic at
        // all with respect to `run`, meaning that this could theoretically be a
        // use-after-free. There's not much we can do to protect against that,
        // however. Let's just assume a well-behaved runtime and go from there!
        rtassert!(!RUNNING.load(atomic::SeqCst));
        let queue = QUEUE.load(atomic::SeqCst);
        rtassert!(queue != 0);
        (*(queue as *const Queue)).lock().push(f);
    }
}

pub fn run() {
    let cur = unsafe {
        rtassert!(!RUNNING.load(atomic::SeqCst));
        let queue = QUEUE.swap(0, atomic::SeqCst);
        rtassert!(queue != 0);

        let queue: Box<Queue> = mem::transmute(queue);
        let v = mem::replace(&mut *queue.lock(), Vec::new());
        v
    };

    for to_run in cur.move_iter() {
        to_run();
    }
}
