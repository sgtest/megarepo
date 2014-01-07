// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! The native runtime crate
//!
//! This crate contains an implementation of 1:1 scheduling for a "native"
//! runtime. In addition, all I/O provided by this crate is the thread blocking
//! version of I/O.

#[crate_id = "native#0.9"];
#[license = "MIT/ASL2"];
#[crate_type = "rlib"];
#[crate_type = "dylib"];

// NB this crate explicitly does *not* allow glob imports, please seriously
//    consider whether they're needed before adding that feature here (the
//    answer is that you don't need them)

use std::os;
use std::rt;

mod bookeeping;
pub mod io;
pub mod task;

// XXX: this should not exist here
#[cfg(stage0, nativestart)]
#[lang = "start"]
pub fn lang_start(main: *u8, argc: int, argv: **u8) -> int {
    use std::cast;
    use std::task;

    do start(argc, argv) {
        // Instead of invoking main directly on this thread, invoke it on
        // another spawned thread that we are guaranteed to know the size of the
        // stack of. Currently, we do not have a method of figuring out the size
        // of the main thread's stack, so for stack overflow detection to work
        // we must spawn the task in a subtask which we know the stack size of.
        let main: extern "Rust" fn() = unsafe { cast::transmute(main) };
        let mut task = task::task();
        task.name("<main>");
        match do task.try { main() } {
            Ok(()) => { os::set_exit_status(0); }
            Err(..) => { os::set_exit_status(rt::DEFAULT_ERROR_CODE); }
        }
    }
}

/// Executes the given procedure after initializing the runtime with the given
/// argc/argv.
///
/// This procedure is guaranteed to run on the thread calling this function, but
/// the stack bounds for this rust task will *not* be set. Care must be taken
/// for this function to not overflow its stack.
///
/// This function will only return once *all* native threads in the system have
/// exited.
pub fn start(argc: int, argv: **u8, main: proc()) -> int {
    rt::init(argc, argv);
    let mut exit_code = None;
    let mut main = Some(main);
    task::new().run(|| {
        exit_code = Some(run(main.take_unwrap()));
    });
    unsafe { rt::cleanup(); }
    // If the exit code wasn't set, then the task block must have failed.
    return exit_code.unwrap_or(rt::DEFAULT_ERROR_CODE);
}

/// Executes a procedure on the current thread in a Rust task context.
///
/// This function has all of the same details as `start` except for a different
/// number of arguments.
pub fn run(main: proc()) -> int {
    main();
    bookeeping::wait_for_other_tasks();
    os::get_exit_status()
}
