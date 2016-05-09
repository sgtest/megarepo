// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Unix-specific extensions to primitives in the `std::process` module.

#![stable(feature = "rust1", since = "1.0.0")]

use prelude::v1::*;

use io;
use os::unix::io::{FromRawFd, RawFd, AsRawFd, IntoRawFd};
use process;
use sys;
use sys_common::{AsInnerMut, AsInner, FromInner, IntoInner};

/// Unix-specific extensions to the `std::process::Command` builder
#[stable(feature = "rust1", since = "1.0.0")]
pub trait CommandExt {
    /// Sets the child process's user id. This translates to a
    /// `setuid` call in the child process. Failure in the `setuid`
    /// call will cause the spawn to fail.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn uid(&mut self, id: u32) -> &mut process::Command;

    /// Similar to `uid`, but sets the group id of the child process. This has
    /// the same semantics as the `uid` field.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn gid(&mut self, id: u32) -> &mut process::Command;

    /// Create a new session (cf. `setsid(2)`) for the child process. This means
    /// that the child is the leader of a new process group. The parent process
    /// remains the child reaper of the new process.
    ///
    /// This is not enough to create a daemon process. The *init* process should
    /// be the child reaper of a daemon. This can be achieved if the parent
    /// process exit. Moreover, a daemon should not have a controlling terminal.
    /// To achieve this, a session leader (the child) must spawn another process
    /// (the daemon) in the same session.
    #[unstable(feature = "process_session_leader", reason = "recently added",
               issue = "27811")]
    #[rustc_deprecated(reason = "use `before_exec` instead",
                       since = "1.9.0")]
    fn session_leader(&mut self, on: bool) -> &mut process::Command;

    /// Schedules a closure to be run just before the `exec` function is
    /// invoked.
    ///
    /// The closure is allowed to return an I/O error whose OS error code will
    /// be communicated back to the parent and returned as an error from when
    /// the spawn was requested.
    ///
    /// Multiple closures can be registered and they will be called in order of
    /// their registration. If a closure returns `Err` then no further closures
    /// will be called and the spawn operation will immediately return with a
    /// failure.
    ///
    /// # Notes
    ///
    /// This closure will be run in the context of the child process after a
    /// `fork`. This primarily means that any modificatons made to memory on
    /// behalf of this closure will **not** be visible to the parent process.
    /// This is often a very constrained environment where normal operations
    /// like `malloc` or acquiring a mutex are not guaranteed to work (due to
    /// other threads perhaps still running when the `fork` was run).
    ///
    /// When this closure is run, aspects such as the stdio file descriptors and
    /// working directory have successfully been changed, so output to these
    /// locations may not appear where intended.
    #[unstable(feature = "process_exec", issue = "31398")]
    fn before_exec<F>(&mut self, f: F) -> &mut process::Command
        where F: FnMut() -> io::Result<()> + Send + Sync + 'static;

    /// Performs all the required setup by this `Command`, followed by calling
    /// the `execvp` syscall.
    ///
    /// On success this function will not return, and otherwise it will return
    /// an error indicating why the exec (or another part of the setup of the
    /// `Command`) failed.
    ///
    /// This function, unlike `spawn`, will **not** `fork` the process to create
    /// a new child. Like spawn, however, the default behavior for the stdio
    /// descriptors will be to inherited from the current process.
    ///
    /// # Notes
    ///
    /// The process may be in a "broken state" if this function returns in
    /// error. For example the working directory, environment variables, signal
    /// handling settings, various user/group information, or aspects of stdio
    /// file descriptors may have changed. If a "transactional spawn" is
    /// required to gracefully handle errors it is recommended to use the
    /// cross-platform `spawn` instead.
    #[stable(feature = "process_exec2", since = "1.9.0")]
    fn exec(&mut self) -> io::Error;
}

#[stable(feature = "rust1", since = "1.0.0")]
impl CommandExt for process::Command {
    fn uid(&mut self, id: u32) -> &mut process::Command {
        self.as_inner_mut().uid(id);
        self
    }

    fn gid(&mut self, id: u32) -> &mut process::Command {
        self.as_inner_mut().gid(id);
        self
    }

    fn session_leader(&mut self, on: bool) -> &mut process::Command {
        self.as_inner_mut().session_leader(on);
        self
    }

    fn before_exec<F>(&mut self, f: F) -> &mut process::Command
        where F: FnMut() -> io::Result<()> + Send + Sync + 'static
    {
        self.as_inner_mut().before_exec(Box::new(f));
        self
    }

    fn exec(&mut self) -> io::Error {
        self.as_inner_mut().exec(sys::process::Stdio::Inherit)
    }
}

/// Unix-specific extensions to `std::process::ExitStatus`
#[stable(feature = "rust1", since = "1.0.0")]
pub trait ExitStatusExt {
    /// Creates a new `ExitStatus` from the raw underlying `i32` return value of
    /// a process.
    #[unstable(feature = "exit_status_from", issue = "32713")]
    fn from_raw(raw: i32) -> Self;

    /// If the process was terminated by a signal, returns that signal.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn signal(&self) -> Option<i32>;
}

#[stable(feature = "rust1", since = "1.0.0")]
impl ExitStatusExt for process::ExitStatus {
    fn from_raw(raw: i32) -> Self {
        process::ExitStatus::from_inner(From::from(raw))
    }

    fn signal(&self) -> Option<i32> {
        self.as_inner().signal()
    }
}

#[stable(feature = "process_extensions", since = "1.2.0")]
impl FromRawFd for process::Stdio {
    unsafe fn from_raw_fd(fd: RawFd) -> process::Stdio {
        let fd = sys::fd::FileDesc::new(fd);
        let io = sys::process::Stdio::Fd(fd);
        process::Stdio::from_inner(io)
    }
}

#[stable(feature = "process_extensions", since = "1.2.0")]
impl AsRawFd for process::ChildStdin {
    fn as_raw_fd(&self) -> RawFd {
        self.as_inner().fd().raw()
    }
}

#[stable(feature = "process_extensions", since = "1.2.0")]
impl AsRawFd for process::ChildStdout {
    fn as_raw_fd(&self) -> RawFd {
        self.as_inner().fd().raw()
    }
}

#[stable(feature = "process_extensions", since = "1.2.0")]
impl AsRawFd for process::ChildStderr {
    fn as_raw_fd(&self) -> RawFd {
        self.as_inner().fd().raw()
    }
}

#[stable(feature = "process_extensions", since = "1.2.0")]
impl IntoRawFd for process::ChildStdin {
    fn into_raw_fd(self) -> RawFd {
        self.into_inner().into_fd().into_raw()
    }
}

#[stable(feature = "process_extensions", since = "1.2.0")]
impl IntoRawFd for process::ChildStdout {
    fn into_raw_fd(self) -> RawFd {
        self.into_inner().into_fd().into_raw()
    }
}

#[stable(feature = "process_extensions", since = "1.2.0")]
impl IntoRawFd for process::ChildStderr {
    fn into_raw_fd(self) -> RawFd {
        self.into_inner().into_fd().into_raw()
    }
}
