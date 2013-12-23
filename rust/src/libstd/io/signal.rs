// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!

Signal handling

This modules provides bindings to receive signals safely, built on top of the
local I/O factory. There are a number of defined signals which can be caught,
but not all signals will work across all platforms (windows doesn't have
definitions for a number of signals.

*/

use clone::Clone;
use comm::{Port, SharedChan};
use container::{Map, MutableMap};
use hashmap;
use io::io_error;
use result::{Err, Ok};
use rt::rtio::{IoFactory, LocalIo, RtioSignal};

#[repr(int)]
#[deriving(Eq, IterBytes)]
pub enum Signum {
    /// Equivalent to SIGBREAK, delivered when the user presses Ctrl-Break.
    Break = 21i,
    /// Equivalent to SIGHUP, delivered when the user closes the terminal
    /// window. On delivery of HangUp, the program is given approximately
    /// 10 seconds to perform any cleanup. After that, Windows will
    /// unconditionally terminate it.
    HangUp = 1i,
    /// Equivalent to SIGINT, delivered when the user presses Ctrl-c.
    Interrupt = 2i,
    /// Equivalent to SIGQUIT, delivered when the user presses Ctrl-\.
    Quit = 3i,
    /// Equivalent to SIGTSTP, delivered when the user presses Ctrl-z.
    StopTemporarily = 20i,
    /// Equivalent to SIGUSR1.
    User1 = 10i,
    /// Equivalent to SIGUSR2.
    User2 = 12i,
    /// Equivalent to SIGWINCH, delivered when the console has been resized.
    /// WindowSizeChange may not be delivered in a timely manner; size change
    /// will only be detected when the cursor is being moved.
    WindowSizeChange = 28i,
}

/// Listener provides a port to listen for registered signals.
///
/// Listener automatically unregisters its handles once it is out of scope.
/// However, clients can still unregister signums manually.
///
/// # Example
///
/// ```rust,ignore
/// use std::io::signal::{Listener, Interrupt};
///
/// let mut listener = Listener::new();
/// listener.register(Interrupt);
///
/// do spawn {
///     loop {
///         match listener.port.recv() {
///             Interrupt => println("Got Interrupt'ed"),
///             _ => (),
///         }
///     }
/// }
///
/// ```
pub struct Listener {
    /// A map from signums to handles to keep the handles in memory
    priv handles: hashmap::HashMap<Signum, ~RtioSignal>,
    /// chan is where all the handles send signums, which are received by
    /// the clients from port.
    priv chan: SharedChan<Signum>,

    /// Clients of Listener can `recv()` from this port. This is exposed to
    /// allow selection over this port as well as manipulation of the port
    /// directly.
    port: Port<Signum>,
}

impl Listener {
    /// Creates a new listener for signals. Once created, signals are bound via
    /// the `register` method (otherwise nothing will ever be received)
    pub fn new() -> Listener {
        let (port, chan) = SharedChan::new();
        Listener {
            chan: chan,
            port: port,
            handles: hashmap::HashMap::new(),
        }
    }

    /// Listen for a signal, returning true when successfully registered for
    /// signum. Signals can be received using `recv()`.
    ///
    /// Once a signal is registered, this listener will continue to receive
    /// notifications of signals until it is unregistered. This occurs
    /// regardless of the number of other listeners registered in other tasks
    /// (or on this task).
    ///
    /// Signals are still received if there is no task actively waiting for
    /// a signal, and a later call to `recv` will return the signal that was
    /// received while no task was waiting on it.
    ///
    /// # Failure
    ///
    /// If this function fails to register a signal handler, then an error will
    /// be raised on the `io_error` condition and the function will return
    /// false.
    pub fn register(&mut self, signum: Signum) -> bool {
        if self.handles.contains_key(&signum) {
            return true; // self is already listening to signum, so succeed
        }
        let mut io = LocalIo::borrow();
        match io.get().signal(signum, self.chan.clone()) {
            Ok(w) => {
                self.handles.insert(signum, w);
                true
            },
            Err(ioerr) => {
                io_error::cond.raise(ioerr);
                false
            }
        }
    }

    /// Unregisters a signal. If this listener currently had a handler
    /// registered for the signal, then it will stop receiving any more
    /// notification about the signal. If the signal has already been received,
    /// it may still be returned by `recv`.
    pub fn unregister(&mut self, signum: Signum) {
        self.handles.pop(&signum);
    }
}

#[cfg(test)]
mod test {
    use libc;
    use io::timer;
    use super::{Listener, Interrupt};

    // kill is only available on Unixes
    #[cfg(unix)]
    fn sigint() {
        unsafe {
            libc::funcs::posix88::signal::kill(libc::getpid(), libc::SIGINT);
        }
    }

    #[test] #[cfg(unix, not(target_os="android"))] // FIXME(#10378)
    fn test_io_signal_smoketest() {
        let mut signal = Listener::new();
        signal.register(Interrupt);
        sigint();
        timer::sleep(10);
        match signal.port.recv() {
            Interrupt => (),
            s => fail!("Expected Interrupt, got {:?}", s),
        }
    }

    #[test] #[cfg(unix, not(target_os="android"))] // FIXME(#10378)
    fn test_io_signal_two_signal_one_signum() {
        let mut s1 = Listener::new();
        let mut s2 = Listener::new();
        s1.register(Interrupt);
        s2.register(Interrupt);
        sigint();
        timer::sleep(10);
        match s1.port.recv() {
            Interrupt => (),
            s => fail!("Expected Interrupt, got {:?}", s),
        }
        match s2.port.recv() {
            Interrupt => (),
            s => fail!("Expected Interrupt, got {:?}", s),
        }
    }

    #[test] #[cfg(unix, not(target_os="android"))] // FIXME(#10378)
    fn test_io_signal_unregister() {
        let mut s1 = Listener::new();
        let mut s2 = Listener::new();
        s1.register(Interrupt);
        s2.register(Interrupt);
        s2.unregister(Interrupt);
        sigint();
        timer::sleep(10);
        assert!(s2.port.try_recv().is_none());
    }

    #[cfg(windows)]
    #[test]
    fn test_io_signal_invalid_signum() {
        use io;
        use super::User1;
        let mut s = Listener::new();
        let mut called = false;
        io::io_error::cond.trap(|_| {
            called = true;
        }).inside(|| {
            if s.register(User1) {
                fail!("Unexpected successful registry of signum {:?}", User1);
            }
        });
        assert!(called);
    }
}
