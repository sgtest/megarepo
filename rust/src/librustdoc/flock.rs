// Copyright 2014-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Simple file-locking apis for each OS.
//!
//! This is not meant to be in the standard library, it does nothing with
//! green/native threading. This is just a bare-bones enough solution for
//! librustdoc, it is not production quality at all.

#![allow(non_camel_case_types)]

pub use self::imp::Lock;

#[cfg(unix)]
mod imp {
    use std::ffi::{AsOsStr, CString};
    use std::os::unix::prelude::*;
    use std::path::Path;
    use libc;
    use std::os as stdos;

    #[cfg(target_os = "linux")]
    mod os {
        use libc;

        pub struct flock {
            pub l_type: libc::c_short,
            pub l_whence: libc::c_short,
            pub l_start: libc::off_t,
            pub l_len: libc::off_t,
            pub l_pid: libc::pid_t,

            // not actually here, but brings in line with freebsd
            pub l_sysid: libc::c_int,
        }

        pub const F_WRLCK: libc::c_short = 1;
        pub const F_UNLCK: libc::c_short = 2;
        pub const F_SETLK: libc::c_int = 6;
        pub const F_SETLKW: libc::c_int = 7;
    }

    #[cfg(target_os = "freebsd")]
    mod os {
        use libc;

        pub struct flock {
            pub l_start: libc::off_t,
            pub l_len: libc::off_t,
            pub l_pid: libc::pid_t,
            pub l_type: libc::c_short,
            pub l_whence: libc::c_short,
            pub l_sysid: libc::c_int,
        }

        pub const F_UNLCK: libc::c_short = 2;
        pub const F_WRLCK: libc::c_short = 3;
        pub const F_SETLK: libc::c_int = 12;
        pub const F_SETLKW: libc::c_int = 13;
    }

    #[cfg(any(target_os = "dragonfly",
              target_os = "bitrig",
              target_os = "openbsd"))]
    mod os {
        use libc;

        pub struct flock {
            pub l_start: libc::off_t,
            pub l_len: libc::off_t,
            pub l_pid: libc::pid_t,
            pub l_type: libc::c_short,
            pub l_whence: libc::c_short,

            // not actually here, but brings in line with freebsd
            pub l_sysid: libc::c_int,
        }

        pub const F_UNLCK: libc::c_short = 2;
        pub const F_WRLCK: libc::c_short = 3;
        pub const F_SETLK: libc::c_int = 8;
        pub const F_SETLKW: libc::c_int = 9;
    }

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    mod os {
        use libc;

        pub struct flock {
            pub l_start: libc::off_t,
            pub l_len: libc::off_t,
            pub l_pid: libc::pid_t,
            pub l_type: libc::c_short,
            pub l_whence: libc::c_short,

            // not actually here, but brings in line with freebsd
            pub l_sysid: libc::c_int,
        }

        pub const F_UNLCK: libc::c_short = 2;
        pub const F_WRLCK: libc::c_short = 3;
        pub const F_SETLK: libc::c_int = 8;
        pub const F_SETLKW: libc::c_int = 9;
    }

    pub struct Lock {
        fd: libc::c_int,
    }

    impl Lock {
        pub fn new(p: &Path) -> Lock {
            let buf = CString::new(p.as_os_str().as_bytes()).unwrap();
            let fd = unsafe {
                libc::open(buf.as_ptr(), libc::O_RDWR | libc::O_CREAT,
                           libc::S_IRWXU)
            };
            assert!(fd > 0, "failed to open lockfile: [{}] {}",
                    stdos::errno(), stdos::last_os_error());
            let flock = os::flock {
                l_start: 0,
                l_len: 0,
                l_pid: 0,
                l_whence: libc::SEEK_SET as libc::c_short,
                l_type: os::F_WRLCK,
                l_sysid: 0,
            };
            let ret = unsafe {
                libc::fcntl(fd, os::F_SETLKW, &flock)
            };
            if ret == -1 {
                let errno = stdos::errno();
                unsafe { libc::close(fd); }
                panic!("could not lock `{}`: [{}] {}", p.display(),
                       errno, stdos::error_string(errno))
            }
            Lock { fd: fd }
        }
    }

    impl Drop for Lock {
        fn drop(&mut self) {
            let flock = os::flock {
                l_start: 0,
                l_len: 0,
                l_pid: 0,
                l_whence: libc::SEEK_SET as libc::c_short,
                l_type: os::F_UNLCK,
                l_sysid: 0,
            };
            unsafe {
                libc::fcntl(self.fd, os::F_SETLK, &flock);
                libc::close(self.fd);
            }
        }
    }
}

#[cfg(windows)]
mod imp {
    use libc;
    use std::ffi::AsOsStr;
    use std::mem;
    use std::os::windows::prelude::*;
    use std::os;
    use std::path::Path;
    use std::ptr;

    const LOCKFILE_EXCLUSIVE_LOCK: libc::DWORD = 0x00000002;

    #[allow(non_snake_case)]
    extern "system" {
        fn LockFileEx(hFile: libc::HANDLE,
                      dwFlags: libc::DWORD,
                      dwReserved: libc::DWORD,
                      nNumberOfBytesToLockLow: libc::DWORD,
                      nNumberOfBytesToLockHigh: libc::DWORD,
                      lpOverlapped: libc::LPOVERLAPPED) -> libc::BOOL;
        fn UnlockFileEx(hFile: libc::HANDLE,
                        dwReserved: libc::DWORD,
                        nNumberOfBytesToLockLow: libc::DWORD,
                        nNumberOfBytesToLockHigh: libc::DWORD,
                        lpOverlapped: libc::LPOVERLAPPED) -> libc::BOOL;
    }

    pub struct Lock {
        handle: libc::HANDLE,
    }

    impl Lock {
        pub fn new(p: &Path) -> Lock {
            let mut p_16: Vec<_> = p.as_os_str().encode_wide().collect();
            p_16.push(0);
            let handle = unsafe {
                libc::CreateFileW(p_16.as_ptr(),
                                  libc::FILE_GENERIC_READ |
                                    libc::FILE_GENERIC_WRITE,
                                  libc::FILE_SHARE_READ |
                                    libc::FILE_SHARE_DELETE |
                                    libc::FILE_SHARE_WRITE,
                                  ptr::null_mut(),
                                  libc::CREATE_ALWAYS,
                                  libc::FILE_ATTRIBUTE_NORMAL,
                                  ptr::null_mut())
            };
            if handle == libc::INVALID_HANDLE_VALUE {
                panic!("create file error: [{}] {}",
                       os::errno(), os::last_os_error());
            }
            let mut overlapped: libc::OVERLAPPED = unsafe { mem::zeroed() };
            let ret = unsafe {
                LockFileEx(handle, LOCKFILE_EXCLUSIVE_LOCK, 0, 100, 0,
                           &mut overlapped)
            };
            if ret == 0 {
                let errno = os::errno();
                unsafe { libc::CloseHandle(handle); }
                panic!("could not lock `{}`: [{}] {}", p.display(),
                       errno, os::error_string(errno));
            }
            Lock { handle: handle }
        }
    }

    impl Drop for Lock {
        fn drop(&mut self) {
            let mut overlapped: libc::OVERLAPPED = unsafe { mem::zeroed() };
            unsafe {
                UnlockFileEx(self.handle, 0, 100, 0, &mut overlapped);
                libc::CloseHandle(self.handle);
            }
        }
    }
}
