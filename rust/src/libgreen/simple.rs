// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! A small module implementing a simple "runtime" used for bootstrapping a rust
//! scheduler pool and then interacting with it.

use std::any::Any;
use std::mem;
use std::rt::Runtime;
use std::rt::local::Local;
use std::rt::mutex::NativeMutex;
use std::rt::task::{Task, BlockedTask, TaskOpts};

struct SimpleTask {
    lock: NativeMutex,
    awoken: bool,
}

impl Runtime for SimpleTask {
    // Implement the simple tasks of descheduling and rescheduling, but only in
    // a simple number of cases.
    fn deschedule(mut self: Box<SimpleTask>,
                  times: uint,
                  mut cur_task: Box<Task>,
                  f: |BlockedTask| -> Result<(), BlockedTask>) {
        assert!(times == 1);

        let me = &mut *self as *mut SimpleTask;
        let cur_dupe = &mut *cur_task as *mut Task;
        cur_task.put_runtime(self);
        let task = BlockedTask::block(cur_task);

        // See libnative/task.rs for what's going on here with the `awoken`
        // field and the while loop around wait()
        unsafe {
            let guard = (*me).lock.lock();
            (*me).awoken = false;
            match f(task) {
                Ok(()) => {
                    while !(*me).awoken {
                        guard.wait();
                    }
                }
                Err(task) => { mem::forget(task.wake()); }
            }
            drop(guard);
            cur_task = mem::transmute(cur_dupe);
        }
        Local::put(cur_task);
    }
    fn reawaken(mut self: Box<SimpleTask>, mut to_wake: Box<Task>) {
        let me = &mut *self as *mut SimpleTask;
        to_wake.put_runtime(self);
        unsafe {
            mem::forget(to_wake);
            let guard = (*me).lock.lock();
            (*me).awoken = true;
            guard.signal();
        }
    }

    // These functions are all unimplemented and panic as a result. This is on
    // purpose. A "simple task" is just that, a very simple task that can't
    // really do a whole lot. The only purpose of the task is to get us off our
    // feet and running.
    fn yield_now(self: Box<SimpleTask>, _cur_task: Box<Task>) { panic!() }
    fn maybe_yield(self: Box<SimpleTask>, _cur_task: Box<Task>) { panic!() }
    fn spawn_sibling(self: Box<SimpleTask>,
                     _cur_task: Box<Task>,
                     _opts: TaskOpts,
                     _f: proc():Send) {
        panic!()
    }

    fn stack_bounds(&self) -> (uint, uint) { panic!() }
    fn stack_guard(&self) -> Option<uint> { panic!() }

    fn can_block(&self) -> bool { true }
    fn wrap(self: Box<SimpleTask>) -> Box<Any+'static> { panic!() }
}

pub fn task() -> Box<Task> {
    let mut task = box Task::new();
    task.put_runtime(box SimpleTask {
        lock: unsafe {NativeMutex::new()},
        awoken: false,
    });
    return task;
}
