// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Native threads
//!
//! ## The threading model
//!
//! An executing Rust program consists of a collection of native OS threads,
//! each with their own stack and local state.
//!
//! Communication between threads can be done through
//! [channels](../../std/sync/mpsc/index.html), Rust's message-passing
//! types, along with [other forms of thread
//! synchronization](../../std/sync/index.html) and shared-memory data
//! structures. In particular, types that are guaranteed to be
//! threadsafe are easily shared between threads using the
//! atomically-reference-counted container,
//! [`Arc`](../../std/sync/struct.Arc.html).
//!
//! Fatal logic errors in Rust cause *thread panic*, during which
//! a thread will unwind the stack, running destructors and freeing
//! owned resources. Thread panic is unrecoverable from within
//! the panicking thread (i.e. there is no 'try/catch' in Rust), but
//! the panic may optionally be detected from a different thread. If
//! the main thread panics, the application will exit with a non-zero
//! exit code.
//!
//! When the main thread of a Rust program terminates, the entire program shuts
//! down, even if other threads are still running. However, this module provides
//! convenient facilities for automatically waiting for the termination of a
//! child thread (i.e., join).
//!
//! ## The `Thread` type
//!
//! Threads are represented via the `Thread` type, which you can
//! get in one of two ways:
//!
//! * By spawning a new thread, e.g. using the `thread::spawn` function.
//! * By requesting the current thread, using the `thread::current` function.
//!
//! Threads can be named, and provide some built-in support for low-level
//! synchronization (described below).
//!
//! The `thread::current()` function is available even for threads not spawned
//! by the APIs of this module.
//!
//! ## Spawning a thread
//!
//! A new thread can be spawned using the `thread::spawn` function:
//!
//! ```rust
//! use std::thread;
//!
//! thread::spawn(move || {
//!     // some work here
//! });
//! ```
//!
//! In this example, the spawned thread is "detached" from the current
//! thread. This means that it can outlive its parent (the thread that spawned
//! it), unless this parent is the main thread.
//!
//! The parent thread can also wait on the completion of the child
//! thread; a call to `spawn` produces a `JoinHandle`, which provides
//! a `join` method for waiting:
//!
//! ```rust
//! use std::thread;
//!
//! let child = thread::spawn(move || {
//!     // some work here
//! });
//! // some work here
//! let res = child.join();
//! ```
//!
//! The `join` method returns a `Result` containing `Ok` of the final
//! value produced by the child thread, or `Err` of the value given to
//! a call to `panic!` if the child panicked.
//!
//! ## Scoped threads
//!
//! The `spawn` method does not allow the child and parent threads to
//! share any stack data, since that is not safe in general. However,
//! `scoped` makes it possible to share the parent's stack by forcing
//! a join before any relevant stack frames are popped:
//!
//! ```rust
//! # #![feature(scoped)]
//! use std::thread;
//!
//! let guard = thread::scoped(move || {
//!     // some work here
//! });
//!
//! // do some other work in the meantime
//! let output = guard.join();
//! ```
//!
//! The `scoped` function doesn't return a `Thread` directly; instead,
//! it returns a *join guard*. The join guard is an RAII-style guard
//! that will automatically join the child thread (block until it
//! terminates) when it is dropped. You can join the child thread in
//! advance by calling the `join` method on the guard, which will also
//! return the result produced by the thread.  A handle to the thread
//! itself is available via the `thread` method of the join guard.
//!
//! ## Configuring threads
//!
//! A new thread can be configured before it is spawned via the `Builder` type,
//! which currently allows you to set the name, stack size, and writers for
//! `println!` and `panic!` for the child thread:
//!
//! ```rust
//! # #![allow(unused_must_use)]
//! use std::thread;
//!
//! thread::Builder::new().name("child1".to_string()).spawn(move || {
//!     println!("Hello, world!");
//! });
//! ```
//!
//! ## Blocking support: park and unpark
//!
//! Every thread is equipped with some basic low-level blocking support, via the
//! `park` and `unpark` functions.
//!
//! Conceptually, each `Thread` handle has an associated token, which is
//! initially not present:
//!
//! * The `thread::park()` function blocks the current thread unless or until
//!   the token is available for its thread handle, at which point it atomically
//!   consumes the token. It may also return *spuriously*, without consuming the
//!   token. `thread::park_timeout()` does the same, but allows specifying a
//!   maximum time to block the thread for.
//!
//! * The `unpark()` method on a `Thread` atomically makes the token available
//!   if it wasn't already.
//!
//! In other words, each `Thread` acts a bit like a semaphore with initial count
//! 0, except that the semaphore is *saturating* (the count cannot go above 1),
//! and can return spuriously.
//!
//! The API is typically used by acquiring a handle to the current thread,
//! placing that handle in a shared data structure so that other threads can
//! find it, and then `park`ing. When some desired condition is met, another
//! thread calls `unpark` on the handle.
//!
//! The motivation for this design is twofold:
//!
//! * It avoids the need to allocate mutexes and condvars when building new
//!   synchronization primitives; the threads already provide basic blocking/signaling.
//!
//! * It can be implemented very efficiently on many platforms.
//!
//! ## Thread-local storage
//!
//! This module also provides an implementation of thread local storage for Rust
//! programs. Thread local storage is a method of storing data into a global
//! variable which each thread in the program will have its own copy of.
//! Threads do not share this data, so accesses do not need to be synchronized.
//!
//! At a high level, this module provides two variants of storage:
//!
//! * Owned thread-local storage. This is a type of thread local key which
//!   owns the value that it contains, and will destroy the value when the
//!   thread exits. This variant is created with the `thread_local!` macro and
//!   can contain any value which is `'static` (no borrowed pointers).
//!
//! * Scoped thread-local storage. This type of key is used to store a reference
//!   to a value into local storage temporarily for the scope of a function
//!   call. There are no restrictions on what types of values can be placed
//!   into this key.
//!
//! Both forms of thread local storage provide an accessor function, `with`,
//! which will yield a shared reference to the value to the specified
//! closure. Thread-local keys only allow shared access to values as there is no
//! way to guarantee uniqueness if a mutable borrow was allowed. Most values
//! will want to make use of some form of **interior mutability** through the
//! `Cell` or `RefCell` types.

#![stable(feature = "rust1", since = "1.0.0")]

use prelude::v1::*;

use any::Any;
use cell::UnsafeCell;
use fmt;
use io;
use marker::PhantomData;
use rt::{self, unwind};
use sync::{Mutex, Condvar, Arc};
use sys::thread as imp;
use sys_common::{stack, thread_info};
use thunk::Thunk;
use time::Duration;

////////////////////////////////////////////////////////////////////////////////
// Thread-local storage
////////////////////////////////////////////////////////////////////////////////

#[macro_use] mod local;
#[macro_use] mod scoped_tls;

#[stable(feature = "rust1", since = "1.0.0")]
pub use self::local::{LocalKey, LocalKeyState};

#[unstable(feature = "scoped_tls",
            reason = "scoped TLS has yet to have wide enough use to fully \
                      consider stabilizing its interface")]
pub use self::scoped_tls::ScopedKey;

#[doc(hidden)] pub use self::local::__impl as __local;
#[doc(hidden)] pub use self::scoped_tls::__impl as __scoped;

////////////////////////////////////////////////////////////////////////////////
// Builder
////////////////////////////////////////////////////////////////////////////////

/// Thread configuration. Provides detailed control over the properties
/// and behavior of new threads.
#[stable(feature = "rust1", since = "1.0.0")]
pub struct Builder {
    // A name for the thread-to-be, for identification in panic messages
    name: Option<String>,
    // The size of the stack for the spawned thread
    stack_size: Option<usize>,
}

impl Builder {
    /// Generates the base configuration for spawning a thread, from which
    /// configuration methods can be chained.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn new() -> Builder {
        Builder {
            name: None,
            stack_size: None,
        }
    }

    /// Names the thread-to-be. Currently the name is used for identification
    /// only in panic messages.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn name(mut self, name: String) -> Builder {
        self.name = Some(name);
        self
    }

    /// Sets the size of the stack for the new thread.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn stack_size(mut self, size: usize) -> Builder {
        self.stack_size = Some(size);
        self
    }

    /// Spawns a new thread, and returns a join handle for it.
    ///
    /// The child thread may outlive the parent (unless the parent thread
    /// is the main thread; the whole process is terminated when the main
    /// thread finishes.) The join handle can be used to block on
    /// termination of the child thread, including recovering its panics.
    ///
    /// # Errors
    ///
    /// Unlike the `spawn` free function, this method yields an
    /// `io::Result` to capture any failure to create the thread at
    /// the OS level.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn spawn<F, T>(self, f: F) -> io::Result<JoinHandle<T>> where
        F: FnOnce() -> T, F: Send + 'static, T: Send + 'static
    {
        self.spawn_inner(Box::new(f)).map(|i| JoinHandle(i))
    }

    /// Spawns a new child thread that must be joined within a given
    /// scope, and returns a `JoinGuard`.
    ///
    /// The join guard can be used to explicitly join the child thread (via
    /// `join`), returning `Result<T>`, or it will implicitly join the child
    /// upon being dropped. Because the child thread may refer to data on the
    /// current thread's stack (hence the "scoped" name), it cannot be detached;
    /// it *must* be joined before the relevant stack frame is popped. See the
    /// module documentation for additional details.
    ///
    /// # Errors
    ///
    /// Unlike the `scoped` free function, this method yields an
    /// `io::Result` to capture any failure to create the thread at
    /// the OS level.
    #[unstable(feature = "scoped",
               reason = "memory unsafe if destructor is avoided, see #24292")]
    pub fn scoped<'a, T, F>(self, f: F) -> io::Result<JoinGuard<'a, T>> where
        T: Send + 'a, F: FnOnce() -> T, F: Send + 'a
    {
        self.spawn_inner(Box::new(f)).map(|inner| {
            JoinGuard { inner: inner, _marker: PhantomData }
        })
    }

    fn spawn_inner<T: Send>(self, f: Thunk<(), T>) -> io::Result<JoinInner<T>> {
        let Builder { name, stack_size } = self;

        let stack_size = stack_size.unwrap_or(rt::min_stack());

        let my_thread = Thread::new(name);
        let their_thread = my_thread.clone();

        let my_packet = Packet(Arc::new(UnsafeCell::new(None)));
        let their_packet = Packet(my_packet.0.clone());

        // Spawning a new OS thread guarantees that __morestack will never get
        // triggered, but we must manually set up the actual stack bounds once
        // this function starts executing. This raises the lower limit by a bit
        // because by the time that this function is executing we've already
        // consumed at least a little bit of stack (we don't know the exact byte
        // address at which our stack started).
        let main = move || {
            let something_around_the_top_of_the_stack = 1;
            let addr = &something_around_the_top_of_the_stack as *const i32;
            let my_stack_top = addr as usize;
            let my_stack_bottom = my_stack_top - stack_size + 1024;
            unsafe {
                if let Some(name) = their_thread.name() {
                    imp::set_name(name);
                }
                stack::record_os_managed_stack_bounds(my_stack_bottom,
                                                      my_stack_top);
                thread_info::set(imp::guard::current(), their_thread);
            }

            let mut output: Option<T> = None;
            let try_result = {
                let ptr = &mut output;

                // There are two primary reasons that general try/catch is
                // unsafe. The first is that we do not support nested
                // try/catch. The fact that this is happening in a newly-spawned
                // thread suffices. The second is that unwinding while unwinding
                // is not defined.  We take care of that by having an
                // 'unwinding' flag in the thread itself. For these reasons,
                // this unsafety should be ok.
                unsafe {
                    unwind::try(move || {
                        let f: Thunk<(), T> = f;
                        let v: T = f();
                        *ptr = Some(v)
                    })
                }
            };
            unsafe {
                *their_packet.0.get() = Some(match (output, try_result) {
                    (Some(data), Ok(_)) => Ok(data),
                    (None, Err(cause)) => Err(cause),
                    _ => unreachable!()
                });
            }
        };

        Ok(JoinInner {
            native: try!(unsafe { imp::create(stack_size, Box::new(main)) }),
            thread: my_thread,
            packet: my_packet,
            joined: false,
        })
    }
}

////////////////////////////////////////////////////////////////////////////////
// Free functions
////////////////////////////////////////////////////////////////////////////////

/// Spawns a new thread, returning a `JoinHandle` for it.
///
/// The join handle will implicitly *detach* the child thread upon being
/// dropped. In this case, the child thread may outlive the parent (unless
/// the parent thread is the main thread; the whole process is terminated when
/// the main thread finishes.) Additionally, the join handle provides a `join`
/// method that can be used to join the child thread. If the child thread
/// panics, `join` will return an `Err` containing the argument given to
/// `panic`.
///
/// # Panics
///
/// Panics if the OS fails to create a thread; use `Builder::spawn`
/// to recover from such errors.
#[stable(feature = "rust1", since = "1.0.0")]
pub fn spawn<F, T>(f: F) -> JoinHandle<T> where
    F: FnOnce() -> T, F: Send + 'static, T: Send + 'static
{
    Builder::new().spawn(f).unwrap()
}

/// Spawns a new *scoped* thread, returning a `JoinGuard` for it.
///
/// The join guard can be used to explicitly join the child thread (via
/// `join`), returning `Result<T>`, or it will implicitly join the child
/// upon being dropped. Because the child thread may refer to data on the
/// current thread's stack (hence the "scoped" name), it cannot be detached;
/// it *must* be joined before the relevant stack frame is popped. See the
/// module documentation for additional details.
///
/// # Panics
///
/// Panics if the OS fails to create a thread; use `Builder::scoped`
/// to recover from such errors.
#[unstable(feature = "scoped",
           reason = "memory unsafe if destructor is avoided, see #24292")]
pub fn scoped<'a, T, F>(f: F) -> JoinGuard<'a, T> where
    T: Send + 'a, F: FnOnce() -> T, F: Send + 'a
{
    Builder::new().scoped(f).unwrap()
}

/// Gets a handle to the thread that invokes it.
#[stable(feature = "rust1", since = "1.0.0")]
pub fn current() -> Thread {
    thread_info::current_thread()
}

/// Cooperatively gives up a timeslice to the OS scheduler.
#[stable(feature = "rust1", since = "1.0.0")]
pub fn yield_now() {
    unsafe { imp::yield_now() }
}

/// Determines whether the current thread is unwinding because of panic.
#[inline]
#[stable(feature = "rust1", since = "1.0.0")]
pub fn panicking() -> bool {
    unwind::panicking()
}

/// Invokes a closure, capturing the cause of panic if one occurs.
///
/// This function will return `Ok(())` if the closure does not panic, and will
/// return `Err(cause)` if the closure panics. The `cause` returned is the
/// object with which panic was originally invoked.
///
/// It is currently undefined behavior to unwind from Rust code into foreign
/// code, so this function is particularly useful when Rust is called from
/// another language (normally C). This can run arbitrary Rust code, capturing a
/// panic and allowing a graceful handling of the error.
///
/// It is **not** recommended to use this function for a general try/catch
/// mechanism. The `Result` type is more appropriate to use for functions that
/// can fail on a regular basis.
///
/// The closure provided is required to adhere to the `'static` bound to ensure
/// that it cannot reference data in the parent stack frame, mitigating problems
/// with exception safety. Furthermore, a `Send` bound is also required,
/// providing the same safety guarantees as `thread::spawn` (ensuring the
/// closure is properly isolated from the parent).
///
/// # Examples
///
/// ```
/// # #![feature(catch_panic)]
/// use std::thread;
///
/// let result = thread::catch_panic(|| {
///     println!("hello!");
/// });
/// assert!(result.is_ok());
///
/// let result = thread::catch_panic(|| {
///     panic!("oh no!");
/// });
/// assert!(result.is_err());
/// ```
#[unstable(feature = "catch_panic", reason = "recent API addition")]
pub fn catch_panic<F, R>(f: F) -> Result<R>
    where F: FnOnce() -> R + Send + 'static
{
    let mut result = None;
    unsafe {
        let result = &mut result;
        try!(::rt::unwind::try(move || *result = Some(f())))
    }
    Ok(result.unwrap())
}

/// Puts the current thread to sleep for the specified amount of time.
///
/// The thread may sleep longer than the duration specified due to scheduling
/// specifics or platform-dependent functionality. Note that on unix platforms
/// this function will not return early due to a signal being received or a
/// spurious wakeup.
#[stable(feature = "rust1", since = "1.0.0")]
pub fn sleep_ms(ms: u32) {
    imp::sleep(Duration::milliseconds(ms as i64))
}

/// Deprecated: use `sleep_ms` instead.
#[unstable(feature = "thread_sleep",
           reason = "recently added, needs an RFC, and `Duration` itself is \
                     unstable")]
#[deprecated(since = "1.0.0", reason = "use sleep_ms instead")]
pub fn sleep(dur: Duration) {
    imp::sleep(dur)
}

/// Blocks unless or until the current thread's token is made available (may wake spuriously).
///
/// See the module doc for more detail.
//
// The implementation currently uses the trivial strategy of a Mutex+Condvar
// with wakeup flag, which does not actually allow spurious wakeups. In the
// future, this will be implemented in a more efficient way, perhaps along the lines of
//   http://cr.openjdk.java.net/~stefank/6989984.1/raw_files/new/src/os/linux/vm/os_linux.cpp
// or futuxes, and in either case may allow spurious wakeups.
#[stable(feature = "rust1", since = "1.0.0")]
pub fn park() {
    let thread = current();
    let mut guard = thread.inner.lock.lock().unwrap();
    while !*guard {
        guard = thread.inner.cvar.wait(guard).unwrap();
    }
    *guard = false;
}

/// Blocks unless or until the current thread's token is made available or
/// the specified duration has been reached (may wake spuriously).
///
/// The semantics of this function are equivalent to `park()` except that the
/// thread will be blocked for roughly no longer than *duration*. This method
/// should not be used for precise timing due to anomalies such as
/// preemption or platform differences that may not cause the maximum
/// amount of time waited to be precisely *duration* long.
///
/// See the module doc for more detail.
#[stable(feature = "rust1", since = "1.0.0")]
pub fn park_timeout_ms(ms: u32) {
    let thread = current();
    let mut guard = thread.inner.lock.lock().unwrap();
    if !*guard {
        let (g, _) = thread.inner.cvar.wait_timeout_ms(guard, ms).unwrap();
        guard = g;
    }
    *guard = false;
}

/// Deprecated: use `park_timeout_ms`
#[unstable(feature = "std_misc", reason = "recently introduced, depends on Duration")]
#[deprecated(since = "1.0.0", reason = "use park_timeout_ms instead")]
pub fn park_timeout(duration: Duration) {
    park_timeout_ms(duration.num_milliseconds() as u32)
}

////////////////////////////////////////////////////////////////////////////////
// Thread
////////////////////////////////////////////////////////////////////////////////

/// The internal representation of a `Thread` handle
struct Inner {
    name: Option<String>,
    lock: Mutex<bool>,          // true when there is a buffered unpark
    cvar: Condvar,
}

unsafe impl Sync for Inner {}

#[derive(Clone)]
#[stable(feature = "rust1", since = "1.0.0")]
/// A handle to a thread.
pub struct Thread {
    inner: Arc<Inner>,
}

impl Thread {
    // Used only internally to construct a thread object without spawning
    fn new(name: Option<String>) -> Thread {
        Thread {
            inner: Arc::new(Inner {
                name: name,
                lock: Mutex::new(false),
                cvar: Condvar::new(),
            })
        }
    }

    /// Atomically makes the handle's token available if it is not already.
    ///
    /// See the module doc for more detail.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn unpark(&self) {
        let mut guard = self.inner.lock.lock().unwrap();
        if !*guard {
            *guard = true;
            self.inner.cvar.notify_one();
        }
    }

    /// Gets the thread's name.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn name(&self) -> Option<&str> {
        self.inner.name.as_ref().map(|s| &**s)
    }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl fmt::Debug for Thread {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&self.name(), f)
    }
}

// a hack to get around privacy restrictions
impl thread_info::NewThread for Thread {
    fn new(name: Option<String>) -> Thread { Thread::new(name) }
}

////////////////////////////////////////////////////////////////////////////////
// JoinHandle and JoinGuard
////////////////////////////////////////////////////////////////////////////////

/// Indicates the manner in which a thread exited.
///
/// A thread that completes without panicking is considered to exit successfully.
#[stable(feature = "rust1", since = "1.0.0")]
pub type Result<T> = ::result::Result<T, Box<Any + Send + 'static>>;

struct Packet<T>(Arc<UnsafeCell<Option<Result<T>>>>);

unsafe impl<T:Send> Send for Packet<T> {}
unsafe impl<T> Sync for Packet<T> {}

/// Inner representation for JoinHandle and JoinGuard
struct JoinInner<T> {
    native: imp::rust_thread,
    thread: Thread,
    packet: Packet<T>,
    joined: bool,
}

impl<T> JoinInner<T> {
    fn join(&mut self) -> Result<T> {
        assert!(!self.joined);
        unsafe { imp::join(self.native) };
        self.joined = true;
        unsafe {
            (*self.packet.0.get()).take().unwrap()
        }
    }
}

/// An owned permission to join on a thread (block on its termination).
///
/// Unlike a `JoinGuard`, a `JoinHandle` *detaches* the child thread
/// when it is dropped, rather than automatically joining on drop.
///
/// Due to platform restrictions, it is not possible to `Clone` this
/// handle: the ability to join a child thread is a uniquely-owned
/// permission.
#[stable(feature = "rust1", since = "1.0.0")]
pub struct JoinHandle<T>(JoinInner<T>);

impl<T> JoinHandle<T> {
    /// Extracts a handle to the underlying thread
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn thread(&self) -> &Thread {
        &self.0.thread
    }

    /// Waits for the associated thread to finish.
    ///
    /// If the child thread panics, `Err` is returned with the parameter given
    /// to `panic`.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn join(mut self) -> Result<T> {
        self.0.join()
    }
}

#[stable(feature = "rust1", since = "1.0.0")]
#[unsafe_destructor]
impl<T> Drop for JoinHandle<T> {
    fn drop(&mut self) {
        if !self.0.joined {
            unsafe { imp::detach(self.0.native) }
        }
    }
}

/// An RAII-style guard that will block until thread termination when dropped.
///
/// The type `T` is the return type for the thread's main function.
///
/// Joining on drop is necessary to ensure memory safety when stack
/// data is shared between a parent and child thread.
///
/// Due to platform restrictions, it is not possible to `Clone` this
/// handle: the ability to join a child thread is a uniquely-owned
/// permission.
#[must_use = "thread will be immediately joined if `JoinGuard` is not used"]
#[unstable(feature = "scoped",
           reason = "memory unsafe if destructor is avoided, see #24292")]
pub struct JoinGuard<'a, T: Send + 'a> {
    inner: JoinInner<T>,
    _marker: PhantomData<&'a T>,
}

#[stable(feature = "rust1", since = "1.0.0")]
unsafe impl<'a, T: Send + 'a> Sync for JoinGuard<'a, T> {}

impl<'a, T: Send + 'a> JoinGuard<'a, T> {
    /// Extracts a handle to the thread this guard will join on.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn thread(&self) -> &Thread {
        &self.inner.thread
    }

    /// Waits for the associated thread to finish, returning the result of the
    /// thread's calculation.
    ///
    /// # Panics
    ///
    /// Panics on the child thread are propagated by panicking the parent.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn join(mut self) -> T {
        match self.inner.join() {
            Ok(res) => res,
            Err(_) => panic!("child thread {:?} panicked", self.thread()),
        }
    }
}

#[unsafe_destructor]
#[unstable(feature = "scoped",
           reason = "memory unsafe if destructor is avoided, see #24292")]
impl<'a, T: Send + 'a> Drop for JoinGuard<'a, T> {
    fn drop(&mut self) {
        if !self.inner.joined {
            if self.inner.join().is_err() {
                panic!("child thread {:?} panicked", self.thread());
            }
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// Tests
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod test {
    use prelude::v1::*;

    use any::Any;
    use sync::mpsc::{channel, Sender};
    use result;
    use super::{Builder};
    use thread;
    use thunk::Thunk;
    use time::Duration;
    use u32;

    // !!! These tests are dangerous. If something is buggy, they will hang, !!!
    // !!! instead of exiting cleanly. This might wedge the buildbots.       !!!

    #[test]
    fn test_unnamed_thread() {
        thread::spawn(move|| {
            assert!(thread::current().name().is_none());
        }).join().ok().unwrap();
    }

    #[test]
    fn test_named_thread() {
        Builder::new().name("ada lovelace".to_string()).scoped(move|| {
            assert!(thread::current().name().unwrap() == "ada lovelace".to_string());
        }).unwrap().join();
    }

    #[test]
    fn test_run_basic() {
        let (tx, rx) = channel();
        thread::spawn(move|| {
            tx.send(()).unwrap();
        });
        rx.recv().unwrap();
    }

    #[test]
    fn test_join_success() {
        assert!(thread::scoped(move|| -> String {
            "Success!".to_string()
        }).join() == "Success!");
    }

    #[test]
    fn test_join_panic() {
        match thread::spawn(move|| {
            panic!()
        }).join() {
            result::Result::Err(_) => (),
            result::Result::Ok(()) => panic!()
        }
    }

    #[test]
    fn test_scoped_success() {
        let res = thread::scoped(move|| -> String {
            "Success!".to_string()
        }).join();
        assert!(res == "Success!");
    }

    #[test]
    #[should_panic]
    fn test_scoped_panic() {
        thread::scoped(|| panic!()).join();
    }

    #[test]
    #[should_panic]
    fn test_scoped_implicit_panic() {
        let _ = thread::scoped(|| panic!());
    }

    #[test]
    fn test_spawn_sched() {
        use clone::Clone;

        let (tx, rx) = channel();

        fn f(i: i32, tx: Sender<()>) {
            let tx = tx.clone();
            thread::spawn(move|| {
                if i == 0 {
                    tx.send(()).unwrap();
                } else {
                    f(i - 1, tx);
                }
            });

        }
        f(10, tx);
        rx.recv().unwrap();
    }

    #[test]
    fn test_spawn_sched_childs_on_default_sched() {
        let (tx, rx) = channel();

        thread::spawn(move|| {
            thread::spawn(move|| {
                tx.send(()).unwrap();
            });
        });

        rx.recv().unwrap();
    }

    fn avoid_copying_the_body<F>(spawnfn: F) where F: FnOnce(Thunk<'static>) {
        let (tx, rx) = channel();

        let x: Box<_> = box 1;
        let x_in_parent = (&*x) as *const i32 as usize;

        spawnfn(Box::new(move|| {
            let x_in_child = (&*x) as *const i32 as usize;
            tx.send(x_in_child).unwrap();
        }));

        let x_in_child = rx.recv().unwrap();
        assert_eq!(x_in_parent, x_in_child);
    }

    #[test]
    fn test_avoid_copying_the_body_spawn() {
        avoid_copying_the_body(|v| {
            thread::spawn(move || v());
        });
    }

    #[test]
    fn test_avoid_copying_the_body_thread_spawn() {
        avoid_copying_the_body(|f| {
            thread::spawn(move|| {
                f();
            });
        })
    }

    #[test]
    fn test_avoid_copying_the_body_join() {
        avoid_copying_the_body(|f| {
            let _ = thread::spawn(move|| {
                f()
            }).join();
        })
    }

    #[test]
    fn test_child_doesnt_ref_parent() {
        // If the child refcounts the parent task, this will stack overflow when
        // climbing the task tree to dereference each ancestor. (See #1789)
        // (well, it would if the constant were 8000+ - I lowered it to be more
        // valgrind-friendly. try this at home, instead..!)
        const GENERATIONS: u32 = 16;
        fn child_no(x: u32) -> Thunk<'static> {
            return Box::new(move|| {
                if x < GENERATIONS {
                    thread::spawn(move|| child_no(x+1)());
                }
            });
        }
        thread::spawn(|| child_no(0)());
    }

    #[test]
    fn test_simple_newsched_spawn() {
        thread::spawn(move || {});
    }

    #[test]
    fn test_try_panic_message_static_str() {
        match thread::spawn(move|| {
            panic!("static string");
        }).join() {
            Err(e) => {
                type T = &'static str;
                assert!(e.is::<T>());
                assert_eq!(*e.downcast::<T>().unwrap(), "static string");
            }
            Ok(()) => panic!()
        }
    }

    #[test]
    fn test_try_panic_message_owned_str() {
        match thread::spawn(move|| {
            panic!("owned string".to_string());
        }).join() {
            Err(e) => {
                type T = String;
                assert!(e.is::<T>());
                assert_eq!(*e.downcast::<T>().unwrap(), "owned string".to_string());
            }
            Ok(()) => panic!()
        }
    }

    #[test]
    fn test_try_panic_message_any() {
        match thread::spawn(move|| {
            panic!(box 413u16 as Box<Any + Send>);
        }).join() {
            Err(e) => {
                type T = Box<Any + Send>;
                assert!(e.is::<T>());
                let any = e.downcast::<T>().unwrap();
                assert!(any.is::<u16>());
                assert_eq!(*any.downcast::<u16>().unwrap(), 413);
            }
            Ok(()) => panic!()
        }
    }

    #[test]
    fn test_try_panic_message_unit_struct() {
        struct Juju;

        match thread::spawn(move|| {
            panic!(Juju)
        }).join() {
            Err(ref e) if e.is::<Juju>() => {}
            Err(_) | Ok(()) => panic!()
        }
    }

    #[test]
    fn test_park_timeout_unpark_before() {
        for _ in 0..10 {
            thread::current().unpark();
            thread::park_timeout_ms(u32::MAX);
        }
    }

    #[test]
    fn test_park_timeout_unpark_not_called() {
        for _ in 0..10 {
            thread::park_timeout_ms(10);
        }
    }

    #[test]
    fn test_park_timeout_unpark_called_other_thread() {
        for _ in 0..10 {
            let th = thread::current();

            let _guard = thread::spawn(move || {
                super::sleep_ms(50);
                th.unpark();
            });

            thread::park_timeout_ms(u32::MAX);
        }
    }

    #[test]
    fn sleep_ms_smoke() {
        thread::sleep_ms(2);
    }

    // NOTE: the corresponding test for stderr is in run-pass/task-stderr, due
    // to the test harness apparently interfering with stderr configuration.
}
