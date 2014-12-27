// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! C definitions used by libnative that don't belong in liblibc

#![allow(dead_code)]
#![allow(non_camel_case_types)]

pub use self::select::fd_set;
pub use self::signal::{sigaction, siginfo, sigset_t};
pub use self::signal::{SA_ONSTACK, SA_RESTART, SA_RESETHAND, SA_NOCLDSTOP};
pub use self::signal::{SA_NODEFER, SA_NOCLDWAIT, SA_SIGINFO, SIGCHLD};

use libc;

#[cfg(any(target_os = "macos",
          target_os = "ios",
          target_os = "freebsd",
          target_os = "dragonfly"))]
pub const FIONBIO: libc::c_ulong = 0x8004667e;
#[cfg(any(all(target_os = "linux",
              any(target_arch = "x86",
                  target_arch = "x86_64",
                  target_arch = "arm")),
          target_os = "android"))]
pub const FIONBIO: libc::c_ulong = 0x5421;
#[cfg(all(target_os = "linux",
          any(target_arch = "mips", target_arch = "mipsel")))]
pub const FIONBIO: libc::c_ulong = 0x667e;

#[cfg(any(target_os = "macos",
          target_os = "ios",
          target_os = "freebsd",
          target_os = "dragonfly"))]
pub const FIOCLEX: libc::c_ulong = 0x20006601;
#[cfg(any(all(target_os = "linux",
              any(target_arch = "x86",
                  target_arch = "x86_64",
                  target_arch = "arm")),
          target_os = "android"))]
pub const FIOCLEX: libc::c_ulong = 0x5451;
#[cfg(all(target_os = "linux",
          any(target_arch = "mips", target_arch = "mipsel")))]
pub const FIOCLEX: libc::c_ulong = 0x6601;

#[cfg(any(target_os = "macos",
          target_os = "ios",
          target_os = "freebsd",
          target_os = "dragonfly"))]
pub const MSG_DONTWAIT: libc::c_int = 0x80;
#[cfg(any(target_os = "linux", target_os = "android"))]
pub const MSG_DONTWAIT: libc::c_int = 0x40;

pub const WNOHANG: libc::c_int = 1;

extern {
    pub fn gettimeofday(timeval: *mut libc::timeval,
                        tzp: *mut libc::c_void) -> libc::c_int;
    pub fn select(nfds: libc::c_int,
                  readfds: *mut fd_set,
                  writefds: *mut fd_set,
                  errorfds: *mut fd_set,
                  timeout: *mut libc::timeval) -> libc::c_int;
    pub fn getsockopt(sockfd: libc::c_int,
                      level: libc::c_int,
                      optname: libc::c_int,
                      optval: *mut libc::c_void,
                      optlen: *mut libc::socklen_t) -> libc::c_int;
    pub fn ioctl(fd: libc::c_int, req: libc::c_ulong, ...) -> libc::c_int;


    pub fn waitpid(pid: libc::pid_t, status: *mut libc::c_int,
                   options: libc::c_int) -> libc::pid_t;

    pub fn sigaction(signum: libc::c_int,
                     act: *const sigaction,
                     oldact: *mut sigaction) -> libc::c_int;

    pub fn sigaddset(set: *mut sigset_t, signum: libc::c_int) -> libc::c_int;
    pub fn sigdelset(set: *mut sigset_t, signum: libc::c_int) -> libc::c_int;
    pub fn sigemptyset(set: *mut sigset_t) -> libc::c_int;
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod select {
    pub const FD_SETSIZE: uint = 1024;

    #[repr(C)]
    pub struct fd_set {
        fds_bits: [i32, ..(FD_SETSIZE / 32)]
    }

    pub fn fd_set(set: &mut fd_set, fd: i32) {
        set.fds_bits[(fd / 32) as uint] |= 1 << ((fd % 32) as uint);
    }
}

#[cfg(any(target_os = "android",
          target_os = "freebsd",
          target_os = "dragonfly",
          target_os = "linux"))]
mod select {
    use uint;
    use libc;

    pub const FD_SETSIZE: uint = 1024;

    #[repr(C)]
    pub struct fd_set {
        // FIXME: shouldn't this be a c_ulong?
        fds_bits: [libc::uintptr_t, ..(FD_SETSIZE / uint::BITS)]
    }

    pub fn fd_set(set: &mut fd_set, fd: i32) {
        let fd = fd as uint;
        set.fds_bits[fd / uint::BITS] |= 1 << (fd % uint::BITS);
    }
}

#[cfg(any(all(target_os = "linux",
              any(target_arch = "x86",
                  target_arch = "x86_64",
                  target_arch = "arm")),
          target_os = "android"))]
mod signal {
    use libc;

    pub const SA_NOCLDSTOP: libc::c_ulong = 0x00000001;
    pub const SA_NOCLDWAIT: libc::c_ulong = 0x00000002;
    pub const SA_NODEFER: libc::c_ulong = 0x40000000;
    pub const SA_ONSTACK: libc::c_ulong = 0x08000000;
    pub const SA_RESETHAND: libc::c_ulong = 0x80000000;
    pub const SA_RESTART: libc::c_ulong = 0x10000000;
    pub const SA_SIGINFO: libc::c_ulong = 0x00000004;
    pub const SIGCHLD: libc::c_int = 17;

    // This definition is not as accurate as it could be, {pid, uid, status} is
    // actually a giant union. Currently we're only interested in these fields,
    // however.
    #[repr(C)]
    pub struct siginfo {
        si_signo: libc::c_int,
        si_errno: libc::c_int,
        si_code: libc::c_int,
        pub pid: libc::pid_t,
        pub uid: libc::uid_t,
        pub status: libc::c_int,
    }

    #[repr(C)]
    pub struct sigaction {
        pub sa_handler: extern fn(libc::c_int),
        pub sa_mask: sigset_t,
        pub sa_flags: libc::c_ulong,
        sa_restorer: *mut libc::c_void,
    }

    unsafe impl ::kinds::Send for sigaction { }
    unsafe impl ::kinds::Sync for sigaction { }

    #[repr(C)]
    #[cfg(target_word_size = "32")]
    pub struct sigset_t {
        __val: [libc::c_ulong, ..32],
    }

    #[repr(C)]
    #[cfg(target_word_size = "64")]
    pub struct sigset_t {
        __val: [libc::c_ulong, ..16],
    }
}

#[cfg(all(target_os = "linux",
          any(target_arch = "mips", target_arch = "mipsel")))]
mod signal {
    use libc;

    pub const SA_NOCLDSTOP: libc::c_ulong = 0x00000001;
    pub const SA_NOCLDWAIT: libc::c_ulong = 0x00010000;
    pub const SA_NODEFER: libc::c_ulong = 0x40000000;
    pub const SA_ONSTACK: libc::c_ulong = 0x08000000;
    pub const SA_RESETHAND: libc::c_ulong = 0x80000000;
    pub const SA_RESTART: libc::c_ulong = 0x10000000;
    pub const SA_SIGINFO: libc::c_ulong = 0x00000008;
    pub const SIGCHLD: libc::c_int = 18;

    // This definition is not as accurate as it could be, {pid, uid, status} is
    // actually a giant union. Currently we're only interested in these fields,
    // however.
    #[repr(C)]
    pub struct siginfo {
        si_signo: libc::c_int,
        si_code: libc::c_int,
        si_errno: libc::c_int,
        pub pid: libc::pid_t,
        pub uid: libc::uid_t,
        pub status: libc::c_int,
    }

    #[repr(C)]
    pub struct sigaction {
        pub sa_flags: libc::c_uint,
        pub sa_handler: extern fn(libc::c_int),
        pub sa_mask: sigset_t,
        sa_restorer: *mut libc::c_void,
        sa_resv: [libc::c_int, ..1],
    }

    impl ::kinds::Send for sigaction { }
    impl ::kinds::Sync for sigaction { }

    #[repr(C)]
    pub struct sigset_t {
        __val: [libc::c_ulong, ..32],
    }
}

#[cfg(any(target_os = "macos",
          target_os = "ios",
          target_os = "freebsd",
          target_os = "dragonfly"))]
mod signal {
    use libc;

    pub const SA_ONSTACK: libc::c_int = 0x0001;
    pub const SA_RESTART: libc::c_int = 0x0002;
    pub const SA_RESETHAND: libc::c_int = 0x0004;
    pub const SA_NOCLDSTOP: libc::c_int = 0x0008;
    pub const SA_NODEFER: libc::c_int = 0x0010;
    pub const SA_NOCLDWAIT: libc::c_int = 0x0020;
    pub const SA_SIGINFO: libc::c_int = 0x0040;
    pub const SIGCHLD: libc::c_int = 20;

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    pub type sigset_t = u32;
    #[cfg(any(target_os = "freebsd", target_os = "dragonfly"))]
    #[repr(C)]
    pub struct sigset_t {
        bits: [u32, ..4],
    }

    // This structure has more fields, but we're not all that interested in
    // them.
    #[repr(C)]
    pub struct siginfo {
        pub si_signo: libc::c_int,
        pub si_errno: libc::c_int,
        pub si_code: libc::c_int,
        pub pid: libc::pid_t,
        pub uid: libc::uid_t,
        pub status: libc::c_int,
    }

    #[repr(C)]
    pub struct sigaction {
        pub sa_handler: extern fn(libc::c_int),
        pub sa_flags: libc::c_int,
        pub sa_mask: sigset_t,
    }
}
