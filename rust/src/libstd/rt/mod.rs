// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Runtime services
//!
//! The `rt` module provides a narrow set of runtime services,
//! including the global heap (exported in `heap`) and unwinding and
//! backtrace support. The APIs in this module are highly unstable,
//! and should be considered as private implementation details for the
//! time being.

#![unstable(feature = "rt",
            reason = "this public module should not exist and is highly likely \
                      to disappear")]
#![allow(missing_docs)]

use prelude::v1::*;
use sys;
use thread;

// Reexport some of our utilities which are expected by other crates.
pub use self::util::min_stack;
pub use self::unwind::{begin_unwind, begin_unwind_fmt};

// Reexport some functionality from liballoc.
pub use alloc::heap;

// Simple backtrace functionality (to print on panic)
pub mod backtrace;

// Internals
#[macro_use]
mod macros;

// These should be refactored/moved/made private over time
pub mod util;
pub mod unwind;
pub mod args;

mod at_exit_imp;
mod libunwind;

mod dwarf;

/// The default error code of the rust runtime if the main thread panics instead
/// of exiting cleanly.
pub const DEFAULT_ERROR_CODE: isize = 101;

#[cfg(not(test))]
#[lang = "start"]
fn lang_start(main: *const u8, argc: isize, argv: *const *const u8) -> isize {
    use prelude::v1::*;

    use mem;
    use rt;
    use sys_common::thread_info::{self, NewThread};
    use thread::Thread;

    let failed = unsafe {
        let main_guard = sys::thread::guard::init();
        sys::stack_overflow::init();

        // Next, set up the current Thread with the guard information we just
        // created. Note that this isn't necessary in general for new threads,
        // but we just do this to name the main thread and to give it correct
        // info about the stack bounds.
        let thread: Thread = NewThread::new(Some("<main>".to_string()));
        thread_info::set(main_guard, thread);

        // By default, some platforms will send a *signal* when a EPIPE error
        // would otherwise be delivered. This runtime doesn't install a SIGPIPE
        // handler, causing it to kill the program, which isn't exactly what we
        // want!
        //
        // Hence, we set SIGPIPE to ignore when the program starts up in order
        // to prevent this problem.
        #[cfg(windows)] fn ignore_sigpipe() {}
        #[cfg(unix)] fn ignore_sigpipe() {
            use libc;
            use libc::funcs::posix01::signal::signal;
            unsafe {
                assert!(signal(libc::SIGPIPE, libc::SIG_IGN) != !0);
            }
        }
        ignore_sigpipe();

        // Store our args if necessary in a squirreled away location
        args::init(argc, argv);

        // And finally, let's run some code!
        let res = thread::catch_panic(mem::transmute::<_, fn()>(main));
        cleanup();
        res.is_err()
    };

    // If the exit code wasn't set, then the try block must have panicked.
    if failed {
        rt::DEFAULT_ERROR_CODE
    } else {
        0
    }
}

/// Enqueues a procedure to run when the main thread exits.
///
/// Currently these closures are only run once the main *Rust* thread exits.
/// Once the `at_exit` handlers begin running, more may be enqueued, but not
/// infinitely so. Eventually a handler registration will be forced to fail.
///
/// Returns `Ok` if the handler was successfully registered, meaning that the
/// closure will be run once the main thread exits. Returns `Err` to indicate
/// that the closure could not be registered, meaning that it is not scheduled
/// to be rune.
pub fn at_exit<F: FnOnce() + Send + 'static>(f: F) -> Result<(), ()> {
    if at_exit_imp::push(Box::new(f)) {Ok(())} else {Err(())}
}

/// One-time runtime cleanup.
///
/// This function is unsafe because it performs no checks to ensure that the
/// runtime has completely ceased running. It is the responsibility of the
/// caller to ensure that the runtime is entirely shut down and nothing will be
/// poking around at the internal components.
///
/// Invoking cleanup while portions of the runtime are still in use may cause
/// undefined behavior.
pub unsafe fn cleanup() {
    args::cleanup();
    sys::stack_overflow::cleanup();
    at_exit_imp::cleanup();
}
