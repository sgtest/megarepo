// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use io;
use libc::{self, c_int, size_t, c_void};
use mem;
use sys::cvt;
use sys_common::AsInner;
use sync::atomic::{AtomicBool, Ordering};

pub struct FileDesc {
    fd: c_int,
}

impl FileDesc {
    pub fn new(fd: c_int) -> FileDesc {
        FileDesc { fd: fd }
    }

    pub fn raw(&self) -> c_int { self.fd }

    /// Extracts the actual filedescriptor without closing it.
    pub fn into_raw(self) -> c_int {
        let fd = self.fd;
        mem::forget(self);
        fd
    }

    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        let ret = try!(cvt(unsafe {
            libc::read(self.fd,
                       buf.as_mut_ptr() as *mut c_void,
                       buf.len() as size_t)
        }));
        Ok(ret as usize)
    }

    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        let ret = try!(cvt(unsafe {
            libc::write(self.fd,
                        buf.as_ptr() as *const c_void,
                        buf.len() as size_t)
        }));
        Ok(ret as usize)
    }

    #[cfg(not(any(target_env = "newlib", target_os = "solaris")))]
    pub fn set_cloexec(&self) {
        unsafe {
            let ret = libc::ioctl(self.fd, libc::FIOCLEX);
            debug_assert_eq!(ret, 0);
        }
    }
    #[cfg(any(target_env = "newlib", target_os = "solaris"))]
    pub fn set_cloexec(&self) {
        unsafe {
            let previous = libc::fcntl(self.fd, libc::F_GETFD);
            let ret = libc::fcntl(self.fd, libc::F_SETFD, previous | libc::FD_CLOEXEC);
            debug_assert_eq!(ret, 0);
        }
    }

    pub fn duplicate(&self) -> io::Result<FileDesc> {
        // We want to atomically duplicate this file descriptor and set the
        // CLOEXEC flag, and currently that's done via F_DUPFD_CLOEXEC. This
        // flag, however, isn't supported on older Linux kernels (earlier than
        // 2.6.24).
        //
        // To detect this and ensure that CLOEXEC is still set, we
        // follow a strategy similar to musl [1] where if passing
        // F_DUPFD_CLOEXEC causes `fcntl` to return EINVAL it means it's not
        // supported (the third parameter, 0, is always valid), so we stop
        // trying that. We also *still* call the `set_cloexec` method as
        // apparently some kernel at some point stopped setting CLOEXEC even
        // though it reported doing so on F_DUPFD_CLOEXEC.
        //
        // Also note that Android doesn't have F_DUPFD_CLOEXEC, but get it to
        // resolve so we at least compile this.
        //
        // [1]: http://comments.gmane.org/gmane.linux.lib.musl.general/2963
        #[cfg(target_os = "android")]
        use libc::F_DUPFD as F_DUPFD_CLOEXEC;
        #[cfg(not(target_os = "android"))]
        use libc::F_DUPFD_CLOEXEC;

        let make_filedesc = |fd| {
            let fd = FileDesc::new(fd);
            fd.set_cloexec();
            fd
        };
        static TRY_CLOEXEC: AtomicBool = AtomicBool::new(true);
        let fd = self.raw();
        if !cfg!(target_os = "android") && TRY_CLOEXEC.load(Ordering::Relaxed) {
            match cvt(unsafe { libc::fcntl(fd, F_DUPFD_CLOEXEC, 0) }) {
                Err(ref e) if e.raw_os_error() == Some(libc::EINVAL) => {
                    TRY_CLOEXEC.store(false, Ordering::Relaxed);
                }
                res => return res.map(make_filedesc),
            }
        }
        cvt(unsafe { libc::fcntl(fd, libc::F_DUPFD, 0) }).map(make_filedesc)
    }
}

impl AsInner<c_int> for FileDesc {
    fn as_inner(&self) -> &c_int { &self.fd }
}

impl Drop for FileDesc {
    fn drop(&mut self) {
        // Note that errors are ignored when closing a file descriptor. The
        // reason for this is that if an error occurs we don't actually know if
        // the file descriptor was closed or not, and if we retried (for
        // something like EINTR), we might close another valid file descriptor
        // (opened after we closed ours.
        let _ = unsafe { libc::close(self.fd) };
    }
}
