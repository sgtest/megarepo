// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Implementation of the helper thread for the timer module
//!
//! This module contains the management necessary for the timer worker thread.
//! This thread is responsible for performing the send()s on channels for timers
//! that are using channels instead of a blocking call.
//!
//! The timer thread is lazily initialized, and it's shut down via the
//! `shutdown` function provided. It must be maintained as an invariant that
//! `shutdown` is only called when the entire program is finished. No new timers
//! can be created in the future and there must be no active timers at that
//! time.

use mem;
use rt::bookkeeping;
use rt::mutex::StaticNativeMutex;
use rt;
use cell::UnsafeCell;
use sys::helper_signal;
use prelude::*;

use task;

/// A structure for management of a helper thread.
///
/// This is generally a static structure which tracks the lifetime of a helper
/// thread.
///
/// The fields of this helper are all public, but they should not be used, this
/// is for static initialization.
pub struct Helper<M> {
    /// Internal lock which protects the remaining fields
    pub lock: StaticNativeMutex,

    // You'll notice that the remaining fields are UnsafeCell<T>, and this is
    // because all helper thread operations are done through &self, but we need
    // these to be mutable (once `lock` is held).

    /// Lazily allocated channel to send messages to the helper thread.
    pub chan: UnsafeCell<*mut Sender<M>>,

    /// OS handle used to wake up a blocked helper thread
    pub signal: UnsafeCell<uint>,

    /// Flag if this helper thread has booted and been initialized yet.
    pub initialized: UnsafeCell<bool>,
}

impl<M: Send> Helper<M> {
    /// Lazily boots a helper thread, becoming a no-op if the helper has already
    /// been spawned.
    ///
    /// This function will check to see if the thread has been initialized, and
    /// if it has it returns quickly. If initialization has not happened yet,
    /// the closure `f` will be run (inside of the initialization lock) and
    /// passed to the helper thread in a separate task.
    ///
    /// This function is safe to be called many times.
    pub fn boot<T: Send>(&'static self,
                         f: || -> T,
                         helper: fn(helper_signal::signal, Receiver<M>, T)) {
        unsafe {
            let _guard = self.lock.lock();
            if !*self.initialized.get() {
                let (tx, rx) = channel();
                *self.chan.get() = mem::transmute(box tx);
                let (receive, send) = helper_signal::new();
                *self.signal.get() = send as uint;

                let t = f();
                task::spawn(proc() {
                    bookkeeping::decrement();
                    helper(receive, rx, t);
                    self.lock.lock().signal()
                });

                rt::at_exit(proc() { self.shutdown() });
                *self.initialized.get() = true;
            }
        }
    }

    /// Sends a message to a spawned worker thread.
    ///
    /// This is only valid if the worker thread has previously booted
    pub fn send(&'static self, msg: M) {
        unsafe {
            let _guard = self.lock.lock();

            // Must send and *then* signal to ensure that the child receives the
            // message. Otherwise it could wake up and go to sleep before we
            // send the message.
            assert!(!self.chan.get().is_null());
            (**self.chan.get()).send(msg);
            helper_signal::signal(*self.signal.get() as helper_signal::signal);
        }
    }

    fn shutdown(&'static self) {
        unsafe {
            // Shut down, but make sure this is done inside our lock to ensure
            // that we'll always receive the exit signal when the thread
            // returns.
            let guard = self.lock.lock();

            // Close the channel by destroying it
            let chan: Box<Sender<M>> = mem::transmute(*self.chan.get());
            *self.chan.get() = 0 as *mut Sender<M>;
            drop(chan);
            helper_signal::signal(*self.signal.get() as helper_signal::signal);

            // Wait for the child to exit
            guard.wait();
            drop(guard);

            // Clean up after ourselves
            self.lock.destroy();
            helper_signal::close(*self.signal.get() as helper_signal::signal);
            *self.signal.get() = 0;
        }
    }
}
