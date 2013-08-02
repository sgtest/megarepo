// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*! The Rust Runtime, including the task scheduler and I/O

The `rt` module provides the private runtime infrastructure necessary
to support core language features like the exchange and local heap,
the garbage collector, logging, local data and unwinding. It also
implements the default task scheduler and task model. Initialization
routines are provided for setting up runtime resources in common
configurations, including that used by `rustc` when generating
executables.

It is intended that the features provided by `rt` can be factored in a
way such that the core library can be built with different 'profiles'
for different use cases, e.g. excluding the task scheduler. A number
of runtime features though are critical to the functioning of the
language and an implementation must be provided regardless of the
execution environment.

Of foremost importance is the global exchange heap, in the module
`global_heap`. Very little practical Rust code can be written without
access to the global heap. Unlike most of `rt` the global heap is
truly a global resource and generally operates independently of the
rest of the runtime.

All other runtime features are task-local, including the local heap,
the garbage collector, local storage, logging and the stack unwinder.

The relationship between `rt` and the rest of the core library is
not entirely clear yet and some modules will be moving into or
out of `rt` as development proceeds.

Several modules in `core` are clients of `rt`:

* `core::task` - The user-facing interface to the Rust task model.
* `core::task::local_data` - The interface to local data.
* `core::gc` - The garbage collector.
* `core::unstable::lang` - Miscellaneous lang items, some of which rely on `core::rt`.
* `core::condition` - Uses local data.
* `core::cleanup` - Local heap destruction.
* `core::io` - In the future `core::io` will use an `rt` implementation.
* `core::logging`
* `core::pipes`
* `core::comm`
* `core::stackwalk`

*/

#[doc(hidden)];
#[deny(unused_imports)];
#[deny(unused_mut)];
#[deny(unused_variable)];
#[deny(unused_unsafe)];

use cell::Cell;
use clone::Clone;
use container::Container;
use iter::Times;
use iterator::{Iterator, IteratorUtil};
use option::{Some, None};
use ptr::RawPtr;
use rt::local::Local;
use rt::sched::{Scheduler, Shutdown};
use rt::sleeper_list::SleeperList;
use rt::task::{Task, SchedTask, GreenTask, Sched};
use rt::thread::Thread;
use rt::work_queue::WorkQueue;
use rt::uv::uvio::UvEventLoop;
use unstable::atomics::{AtomicInt, SeqCst};
use unstable::sync::UnsafeAtomicRcBox;
use vec::{OwnedVector, MutableVector};

/// The global (exchange) heap.
pub mod global_heap;

/// Implementations of language-critical runtime features like @.
pub mod task;

/// Facilities related to task failure, killing, and death.
mod kill;

/// The coroutine task scheduler, built on the `io` event loop.
mod sched;

/// Synchronous I/O.
pub mod io;

/// The EventLoop and internal synchronous I/O interface.
mod rtio;

/// libuv and default rtio implementation.
pub mod uv;

/// The Local trait for types that are accessible via thread-local
/// or task-local storage.
pub mod local;

/// A parallel work-stealing deque.
mod work_queue;

/// A parallel queue.
mod message_queue;

/// A parallel data structure for tracking sleeping schedulers.
mod sleeper_list;

/// Stack segments and caching.
mod stack;

/// CPU context swapping.
mod context;

/// Bindings to system threading libraries.
mod thread;

/// The runtime configuration, read from environment variables
pub mod env;

/// The local, managed heap
pub mod local_heap;

/// The Logger trait and implementations
pub mod logging;

/// Tools for testing the runtime
pub mod test;

/// Reference counting
pub mod rc;

/// A simple single-threaded channel type for passing buffered data between
/// scheduler and task context
pub mod tube;

/// Simple reimplementation of core::comm
pub mod comm;

/// Routines for select()ing on pipes.
pub mod select;

// FIXME #5248 shouldn't be pub
/// The runtime needs to be able to put a pointer into thread-local storage.
pub mod local_ptr;

// FIXME #5248: The import in `sched` doesn't resolve unless this is pub!
/// Bindings to pthread/windows thread-local storage.
pub mod thread_local_storage;

pub mod metrics;

// FIXME #5248 shouldn't be pub
/// Just stuff
pub mod util;

// Global command line argument storage
pub mod args;

// Support for dynamic borrowck
pub mod borrowck;

/// Set up a default runtime configuration, given compiler-supplied arguments.
///
/// This is invoked by the `start` _language item_ (unstable::lang) to
/// run a Rust executable.
///
/// # Arguments
///
/// * `argc` & `argv` - The argument vector. On Unix this information is used
///   by os::args.
/// * `crate_map` - Runtime information about the executing crate, mostly for logging
///
/// # Return value
///
/// The return value is used as the process return code. 0 on success, 101 on error.
pub fn start(argc: int, argv: **u8, crate_map: *u8, main: ~fn()) -> int {

    init(argc, argv, crate_map);
    let exit_code = run(main);
    cleanup();

    return exit_code;
}

/// Like `start` but creates an additional scheduler on the current thread,
/// which in most cases will be the 'main' thread, and pins the main task to it.
///
/// This is appropriate for running code that must execute on the main thread,
/// such as the platform event loop and GUI.
pub fn start_on_main_thread(argc: int, argv: **u8, crate_map: *u8, main: ~fn()) -> int {
    init(argc, argv, crate_map);
    let exit_code = run_on_main_thread(main);
    cleanup();

    return exit_code;
}

/// One-time runtime initialization.
///
/// Initializes global state, including frobbing
/// the crate's logging flags, registering GC
/// metadata, and storing the process arguments.
pub fn init(argc: int, argv: **u8, crate_map: *u8) {
    // XXX: Derefing these pointers is not safe.
    // Need to propagate the unsafety to `start`.
    unsafe {
        args::init(argc, argv);
        logging::init(crate_map);
        rust_update_gc_metadata(crate_map);
    }

    extern {
        fn rust_update_gc_metadata(crate_map: *u8);
    }
}

/// One-time runtime cleanup.
pub fn cleanup() {
    args::cleanup();
}

/// Execute the main function in a scheduler.
///
/// Configures the runtime according to the environment, by default
/// using a task scheduler with the same number of threads as cores.
/// Returns a process exit code.
pub fn run(main: ~fn()) -> int {
    run_(main, false)
}

pub fn run_on_main_thread(main: ~fn()) -> int {
    run_(main, true)
}

fn run_(main: ~fn(), use_main_sched: bool) -> int {
    static DEFAULT_ERROR_CODE: int = 101;

    let nscheds = util::default_sched_threads();

    let main = Cell::new(main);

    // The shared list of sleeping schedulers. Schedulers wake each other
    // occassionally to do new work.
    let sleepers = SleeperList::new();
    // The shared work queue. Temporary until work stealing is implemented.
    let work_queue = WorkQueue::new();

    // The schedulers.
    let mut scheds = ~[];
    // Handles to the schedulers. When the main task ends these will be
    // sent the Shutdown message to terminate the schedulers.
    let mut handles = ~[];

    do nscheds.times {
        rtdebug!("inserting a regular scheduler");

        // Every scheduler is driven by an I/O event loop.
        let loop_ = ~UvEventLoop::new();
        let mut sched = ~Scheduler::new(loop_, work_queue.clone(), sleepers.clone());
        let handle = sched.make_handle();

        scheds.push(sched);
        handles.push(handle);
    }

    // If we need a main-thread task then create a main thread scheduler
    // that will reject any task that isn't pinned to it
    let main_sched = if use_main_sched {

        // Create a friend handle.
        let mut friend_sched = scheds.pop();
        let friend_handle = friend_sched.make_handle();
        scheds.push(friend_sched);

        let main_loop = ~UvEventLoop::new();
        let mut main_sched = ~Scheduler::new_special(main_loop,
                                                     work_queue.clone(),
                                                     sleepers.clone(),
                                                     false,
                                                     Some(friend_handle));
        let main_handle = main_sched.make_handle();
        handles.push(main_handle);
        Some(main_sched)
    } else {
        None
    };

    // Create a shared cell for transmitting the process exit
    // code from the main task to this function.
    let exit_code = UnsafeAtomicRcBox::new(AtomicInt::new(0));
    let exit_code_clone = exit_code.clone();

    // When the main task exits, after all the tasks in the main
    // task tree, shut down the schedulers and set the exit code.
    let handles = Cell::new(handles);
    let on_exit: ~fn(bool) = |exit_success| {

        let mut handles = handles.take();
        foreach handle in handles.mut_iter() {
            handle.send(Shutdown);
        }

        unsafe {
            let exit_code = if exit_success {
                use rt::util;

                // If we're exiting successfully, then return the global
                // exit status, which can be set programmatically.
                util::get_exit_status()
            } else {
                DEFAULT_ERROR_CODE
            };
            (*exit_code_clone.get()).store(exit_code, SeqCst);
        }
    };

    let mut threads = ~[];

    let on_exit = Cell::new(on_exit);

    if !use_main_sched {

        // In the case where we do not use a main_thread scheduler we
        // run the main task in one of our threads.

        let mut main_task = ~Task::new_root(&mut scheds[0].stack_pool,
                                            main.take());
        main_task.death.on_exit = Some(on_exit.take());
        let main_task_cell = Cell::new(main_task);

        let sched = scheds.pop();
        let sched_cell = Cell::new(sched);
        let thread = do Thread::start {
            let sched = sched_cell.take();
            sched.bootstrap(main_task_cell.take());
        };
        threads.push(thread);
    }

    // Run each remaining scheduler in a thread.
    while !scheds.is_empty() {
        rtdebug!("creating regular schedulers");
        let sched = scheds.pop();
        let sched_cell = Cell::new(sched);
        let thread = do Thread::start {
            let mut sched = sched_cell.take();
            let bootstrap_task = ~do Task::new_root(&mut sched.stack_pool) || {
                rtdebug!("boostraping a non-primary scheduler");
            };
            sched.bootstrap(bootstrap_task);
        };
        threads.push(thread);
    }

    // If we do have a main thread scheduler, run it now.

    if use_main_sched {

        rtdebug!("about to create the main scheduler task");

        let mut main_sched = main_sched.get();

        let home = Sched(main_sched.make_handle());
        let mut main_task = ~Task::new_root_homed(&mut main_sched.stack_pool,
                                                  home, main.take());
        main_task.death.on_exit = Some(on_exit.take());
        rtdebug!("boostrapping main_task");

        main_sched.bootstrap(main_task);
    }

    rtdebug!("waiting for threads");

    // Wait for schedulers
    foreach thread in threads.consume_iter() {
        thread.join();
    }

    // Return the exit code
    unsafe {
        (*exit_code.get()).load(SeqCst)
    }
}

/// Possible contexts in which Rust code may be executing.
/// Different runtime services are available depending on context.
/// Mostly used for determining if we're using the new scheduler
/// or the old scheduler.
#[deriving(Eq)]
pub enum RuntimeContext {
    // Only the exchange heap is available
    GlobalContext,
    // The scheduler may be accessed
    SchedulerContext,
    // Full task services, e.g. local heap, unwinding
    TaskContext,
    // Running in an old-style task
    OldTaskContext
}

/// Determine the current RuntimeContext
pub fn context() -> RuntimeContext {

    use task::rt::rust_task;

    if unsafe { rust_try_get_task().is_not_null() } {
        return OldTaskContext;
    } else if Local::exists::<Task>() {
        // In this case we know it is a new runtime context, but we
        // need to check which one. Going to try borrowing task to
        // check. Task should always be in TLS, so hopefully this
        // doesn't conflict with other ops that borrow.
        return do Local::borrow::<Task,RuntimeContext> |task| {
            match task.task_type {
                SchedTask => SchedulerContext,
                GreenTask(_) => TaskContext
            }
        };
    } else {
        return GlobalContext;
    }

    extern {
        #[rust_stack]
        pub fn rust_try_get_task() -> *rust_task;
    }
}

#[test]
fn test_context() {
    use unstable::run_in_bare_thread;

    assert_eq!(context(), OldTaskContext);
    do run_in_bare_thread {
        assert_eq!(context(), GlobalContext);
    }
}
