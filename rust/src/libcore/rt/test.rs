// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use uint;
use option::*;
use cell::Cell;
use result::{Result, Ok, Err};
use super::io::net::ip::{IpAddr, Ipv4};
use rt::task::Task;
use rt::thread::Thread;
use rt::local::Local;

/// Creates a new scheduler in a new thread and runs a task in it,
/// then waits for the scheduler to exit. Failure of the task
/// will abort the process.
pub fn run_in_newsched_task(f: ~fn()) {
    use super::sched::*;
    use unstable::run_in_bare_thread;
    use rt::uv::uvio::UvEventLoop;

    let f = Cell(f);

    do run_in_bare_thread {
        let mut sched = ~UvEventLoop::new_scheduler();
        let task = ~Coroutine::with_task(&mut sched.stack_pool,
                                         ~Task::without_unwinding(),
                                         f.take());
        sched.enqueue_task(task);
        sched.run();
    }
}

/// Test tasks will abort on failure instead of unwinding
pub fn spawntask(f: ~fn()) {
    use super::sched::*;

    let mut sched = Local::take::<Scheduler>();
    let task = ~Coroutine::with_task(&mut sched.stack_pool,
                                     ~Task::without_unwinding(),
                                     f);
    do sched.switch_running_tasks_and_then(task) |task| {
        let task = Cell(task);
        let sched = Local::take::<Scheduler>();
        sched.schedule_new_task(task.take());
    }
}

/// Create a new task and run it right now. Aborts on failure
pub fn spawntask_immediately(f: ~fn()) {
    use super::sched::*;

    let mut sched = Local::take::<Scheduler>();
    let task = ~Coroutine::with_task(&mut sched.stack_pool,
                                     ~Task::without_unwinding(),
                                     f);
    do sched.switch_running_tasks_and_then(task) |task| {
        let task = Cell(task);
        do Local::borrow::<Scheduler> |sched| {
            sched.enqueue_task(task.take());
        }
    }
}

/// Create a new task and run it right now. Aborts on failure
pub fn spawntask_later(f: ~fn()) {
    use super::sched::*;

    let mut sched = Local::take::<Scheduler>();
    let task = ~Coroutine::with_task(&mut sched.stack_pool,
                                     ~Task::without_unwinding(),
                                     f);

    sched.enqueue_task(task);
    Local::put(sched);
}

/// Spawn a task and either run it immediately or run it later
pub fn spawntask_random(f: ~fn()) {
    use super::sched::*;
    use rand::{Rand, rng};

    let mut rng = rng();
    let run_now: bool = Rand::rand(&mut rng);

    let mut sched = Local::take::<Scheduler>();
    let task = ~Coroutine::with_task(&mut sched.stack_pool,
                                     ~Task::without_unwinding(),
                                     f);

    if run_now {
        do sched.switch_running_tasks_and_then(task) |task| {
            let task = Cell(task);
            do Local::borrow::<Scheduler> |sched| {
                sched.enqueue_task(task.take());
            }
        }
    } else {
        sched.enqueue_task(task);
        Local::put(sched);
    }
}


/// Spawn a task and wait for it to finish, returning whether it completed successfully or failed
pub fn spawntask_try(f: ~fn()) -> Result<(), ()> {
    use cell::Cell;
    use super::sched::*;
    use task;
    use unstable::finally::Finally;

    // Our status variables will be filled in from the scheduler context
    let mut failed = false;
    let failed_ptr: *mut bool = &mut failed;

    // Switch to the scheduler
    let f = Cell(Cell(f));
    let sched = Local::take::<Scheduler>();
    do sched.deschedule_running_task_and_then() |old_task| {
        let old_task = Cell(old_task);
        let f = f.take();
        let mut sched = Local::take::<Scheduler>();
        let new_task = ~do Coroutine::new(&mut sched.stack_pool) {
            do (|| {
                (f.take())()
            }).finally {
                // Check for failure then resume the parent task
                unsafe { *failed_ptr = task::failing(); }
                let sched = Local::take::<Scheduler>();
                do sched.switch_running_tasks_and_then(old_task.take()) |new_task| {
                    let new_task = Cell(new_task);
                    do Local::borrow::<Scheduler> |sched| {
                        sched.enqueue_task(new_task.take());
                    }
                }
            }
        };

        sched.resume_task_immediately(new_task);
    }

    if !failed { Ok(()) } else { Err(()) }
}

// Spawn a new task in a new scheduler and return a thread handle.
pub fn spawntask_thread(f: ~fn()) -> Thread {
    use rt::sched::*;
    use rt::uv::uvio::UvEventLoop;

    let f = Cell(f);
    let thread = do Thread::start {
        let mut sched = ~UvEventLoop::new_scheduler();
        let task = ~Coroutine::with_task(&mut sched.stack_pool,
                                         ~Task::without_unwinding(),
                                         f.take());
        sched.enqueue_task(task);
        sched.run();
    };
    return thread;
}

/// Get a port number, starting at 9600, for use in tests
pub fn next_test_port() -> u16 {
    unsafe {
        return rust_dbg_next_port() as u16;
    }
    extern {
        fn rust_dbg_next_port() -> ::libc::uintptr_t;
    }
}

/// Get a unique localhost:port pair starting at 9600
pub fn next_test_ip4() -> IpAddr {
    Ipv4(127, 0, 0, 1, next_test_port())
}

/// Get a constant that represents the number of times to repeat stress tests. Default 1.
pub fn stress_factor() -> uint {
    use os::getenv;

    match getenv("RUST_RT_STRESS") {
        Some(val) => uint::from_str(val).get(),
        None => 1
    }
}

