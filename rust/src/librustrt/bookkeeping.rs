// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Task bookkeeping
//!
//! This module keeps track of the number of running tasks so that entry points
//! with libnative know when it's possible to exit the program (once all tasks
//! have exited).
//!
//! The green counterpart for this is bookkeeping on sched pools, and it's up to
//! each respective runtime to make sure that they call increment() and
//! decrement() manually.

use core::atomic;
use core::ops::Drop;

use mutex::{StaticNativeMutex, NATIVE_MUTEX_INIT};

static TASK_COUNT: atomic::AtomicUint = atomic::INIT_ATOMIC_UINT;
static TASK_LOCK: StaticNativeMutex = NATIVE_MUTEX_INIT;

#[allow(missing_copy_implementations)]
pub struct Token { _private: () }

impl Drop for Token {
    fn drop(&mut self) { decrement() }
}

/// Increment the number of live tasks, returning a token which will decrement
/// the count when dropped.
pub fn increment() -> Token {
    let _ = TASK_COUNT.fetch_add(1, atomic::SeqCst);
    Token { _private: () }
}

pub fn decrement() {
    unsafe {
        if TASK_COUNT.fetch_sub(1, atomic::SeqCst) == 1 {
            let guard = TASK_LOCK.lock();
            guard.signal();
        }
    }
}

/// Waits for all other native tasks in the system to exit. This is only used by
/// the entry points of native programs
pub fn wait_for_other_tasks() {
    unsafe {
        let guard = TASK_LOCK.lock();
        while TASK_COUNT.load(atomic::SeqCst) > 0 {
            guard.wait();
        }
    }
}
