// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use libc::{c_int, c_char, c_void, ssize_t};
use libc;
use std::c_str::CString;
use std::c_str;
use std::mem;
use std::os;
use std::rt::rtio::{IoResult, IoError};
use std::rt::rtio;
use std::rt::task::BlockedTask;

use homing::{HomingIO, HomeHandle};
use super::{Loop, UvError, uv_error_to_io_error, wait_until_woken_after, wakeup};
use uvio::UvIoFactory;
use uvll;

pub struct FsRequest {
    req: *mut uvll::uv_fs_t,
    fired: bool,
}

pub struct FileWatcher {
    loop_: Loop,
    fd: c_int,
    close: rtio::CloseBehavior,
    home: HomeHandle,
}

impl FsRequest {
    pub fn open(io: &mut UvIoFactory, path: &CString, flags: int, mode: int)
        -> Result<FileWatcher, UvError>
    {
        execute(|req, cb| unsafe {
            uvll::uv_fs_open(io.uv_loop(),
                             req, path.with_ref(|p| p), flags as c_int,
                             mode as c_int, cb)
        }).map(|req|
            FileWatcher::new(io, req.get_result() as c_int,
                             rtio::CloseSynchronously)
        )
    }

    pub fn unlink(loop_: &Loop, path: &CString) -> Result<(), UvError> {
        execute_nop(|req, cb| unsafe {
            uvll::uv_fs_unlink(loop_.handle, req, path.with_ref(|p| p),
                               cb)
        })
    }

    pub fn lstat(loop_: &Loop, path: &CString)
        -> Result<rtio::FileStat, UvError>
    {
        execute(|req, cb| unsafe {
            uvll::uv_fs_lstat(loop_.handle, req, path.with_ref(|p| p),
                              cb)
        }).map(|req| req.mkstat())
    }

    pub fn stat(loop_: &Loop, path: &CString) -> Result<rtio::FileStat, UvError> {
        execute(|req, cb| unsafe {
            uvll::uv_fs_stat(loop_.handle, req, path.with_ref(|p| p),
                             cb)
        }).map(|req| req.mkstat())
    }

    pub fn fstat(loop_: &Loop, fd: c_int) -> Result<rtio::FileStat, UvError> {
        execute(|req, cb| unsafe {
            uvll::uv_fs_fstat(loop_.handle, req, fd, cb)
        }).map(|req| req.mkstat())
    }

    pub fn write(loop_: &Loop, fd: c_int, buf: &[u8], offset: i64)
        -> Result<(), UvError>
    {
        // In libuv, uv_fs_write is basically just shelling out to a write()
        // syscall at some point, with very little fluff around it. This means
        // that write() could actually be a short write, so we need to be sure
        // to call it continuously if we get a short write back. This method is
        // expected to write the full data if it returns success.
        let mut written = 0;
        while written < buf.len() {
            let offset = if offset == -1 {
                offset
            } else {
                offset + written as i64
            };
            let uvbuf = uvll::uv_buf_t {
                base: buf.slice_from(written as uint).as_ptr() as *mut _,
                len: (buf.len() - written) as uvll::uv_buf_len_t,
            };
            match execute(|req, cb| unsafe {
                uvll::uv_fs_write(loop_.handle, req, fd, &uvbuf, 1, offset, cb)
            }).map(|req| req.get_result()) {
                Err(e) => return Err(e),
                Ok(n) => { written += n as uint; }
            }
        }
        Ok(())
    }

    pub fn read(loop_: &Loop, fd: c_int, buf: &mut [u8], offset: i64)
        -> Result<int, UvError>
    {
        execute(|req, cb| unsafe {
            let mut uvbuf = uvll::uv_buf_t {
                base: buf.as_mut_ptr(),
                len: buf.len() as uvll::uv_buf_len_t,
            };
            uvll::uv_fs_read(loop_.handle, req, fd, &mut uvbuf, 1, offset, cb)
        }).map(|req| {
            req.get_result() as int
        })
    }

    pub fn mkdir(loop_: &Loop, path: &CString, mode: c_int)
        -> Result<(), UvError>
    {
        execute_nop(|req, cb| unsafe {
            uvll::uv_fs_mkdir(loop_.handle, req, path.with_ref(|p| p),
                              mode, cb)
        })
    }

    pub fn rmdir(loop_: &Loop, path: &CString) -> Result<(), UvError> {
        execute_nop(|req, cb| unsafe {
            uvll::uv_fs_rmdir(loop_.handle, req, path.with_ref(|p| p),
                              cb)
        })
    }

    pub fn rename(loop_: &Loop, path: &CString, to: &CString)
        -> Result<(), UvError>
    {
        execute_nop(|req, cb| unsafe {
            uvll::uv_fs_rename(loop_.handle,
                               req,
                               path.with_ref(|p| p),
                               to.with_ref(|p| p),
                               cb)
        })
    }

    pub fn chmod(loop_: &Loop, path: &CString, mode: c_int)
        -> Result<(), UvError>
    {
        execute_nop(|req, cb| unsafe {
            uvll::uv_fs_chmod(loop_.handle, req, path.with_ref(|p| p),
                              mode, cb)
        })
    }

    pub fn readdir(loop_: &Loop, path: &CString, flags: c_int)
        -> Result<Vec<CString>, UvError>
    {
        execute(|req, cb| unsafe {
            uvll::uv_fs_readdir(loop_.handle,
                                req, path.with_ref(|p| p), flags, cb)
        }).map(|req| unsafe {
            let mut paths = vec!();
            let path = CString::new(path.with_ref(|p| p), false);
            let parent = Path::new(path);
            let _ = c_str::from_c_multistring(req.get_ptr() as *const libc::c_char,
                                              Some(req.get_result() as uint),
                                              |rel| {
                let p = rel.as_bytes();
                paths.push(parent.join(p.slice_to(rel.len())).to_c_str());
            });
            paths
        })
    }

    pub fn readlink(loop_: &Loop, path: &CString) -> Result<CString, UvError> {
        execute(|req, cb| unsafe {
            uvll::uv_fs_readlink(loop_.handle, req,
                                 path.with_ref(|p| p), cb)
        }).map(|req| {
            // Be sure to clone the cstring so we get an independently owned
            // allocation to work with and return.
            unsafe {
                CString::new(req.get_ptr() as *const libc::c_char, false).clone()
            }
        })
    }

    pub fn chown(loop_: &Loop, path: &CString, uid: int, gid: int)
        -> Result<(), UvError>
    {
        execute_nop(|req, cb| unsafe {
            uvll::uv_fs_chown(loop_.handle,
                              req, path.with_ref(|p| p),
                              uid as uvll::uv_uid_t,
                              gid as uvll::uv_gid_t,
                              cb)
        })
    }

    pub fn truncate(loop_: &Loop, file: c_int, offset: i64)
        -> Result<(), UvError>
    {
        execute_nop(|req, cb| unsafe {
            uvll::uv_fs_ftruncate(loop_.handle, req, file, offset, cb)
        })
    }

    pub fn link(loop_: &Loop, src: &CString, dst: &CString)
        -> Result<(), UvError>
    {
        execute_nop(|req, cb| unsafe {
            uvll::uv_fs_link(loop_.handle, req,
                             src.with_ref(|p| p),
                             dst.with_ref(|p| p),
                             cb)
        })
    }

    pub fn symlink(loop_: &Loop, src: &CString, dst: &CString)
        -> Result<(), UvError>
    {
        execute_nop(|req, cb| unsafe {
            uvll::uv_fs_symlink(loop_.handle, req,
                                src.with_ref(|p| p),
                                dst.with_ref(|p| p),
                                0, cb)
        })
    }

    pub fn fsync(loop_: &Loop, fd: c_int) -> Result<(), UvError> {
        execute_nop(|req, cb| unsafe {
            uvll::uv_fs_fsync(loop_.handle, req, fd, cb)
        })
    }

    pub fn datasync(loop_: &Loop, fd: c_int) -> Result<(), UvError> {
        execute_nop(|req, cb| unsafe {
            uvll::uv_fs_fdatasync(loop_.handle, req, fd, cb)
        })
    }

    pub fn utime(loop_: &Loop, path: &CString, atime: u64, mtime: u64)
        -> Result<(), UvError>
    {
        // libuv takes seconds
        let atime = atime as libc::c_double / 1000.0;
        let mtime = mtime as libc::c_double / 1000.0;
        execute_nop(|req, cb| unsafe {
            uvll::uv_fs_utime(loop_.handle, req, path.with_ref(|p| p),
                              atime, mtime, cb)
        })
    }

    pub fn get_result(&self) -> ssize_t {
        unsafe { uvll::get_result_from_fs_req(self.req) }
    }

    pub fn get_stat(&self) -> uvll::uv_stat_t {
        let mut stat = uvll::uv_stat_t::new();
        unsafe { uvll::populate_stat(self.req, &mut stat); }
        stat
    }

    pub fn get_ptr(&self) -> *mut libc::c_void {
        unsafe { uvll::get_ptr_from_fs_req(self.req) }
    }

    pub fn mkstat(&self) -> rtio::FileStat {
        let stat = self.get_stat();
        fn to_msec(stat: uvll::uv_timespec_t) -> u64 {
            // Be sure to cast to u64 first to prevent overflowing if the tv_sec
            // field is a 32-bit integer.
            (stat.tv_sec as u64) * 1000 + (stat.tv_nsec as u64) / 1000000
        }
        rtio::FileStat {
            size: stat.st_size as u64,
            kind: stat.st_mode as u64,
            perm: stat.st_mode as u64,
            created: to_msec(stat.st_birthtim),
            modified: to_msec(stat.st_mtim),
            accessed: to_msec(stat.st_atim),
            device: stat.st_dev as u64,
            inode: stat.st_ino as u64,
            rdev: stat.st_rdev as u64,
            nlink: stat.st_nlink as u64,
            uid: stat.st_uid as u64,
            gid: stat.st_gid as u64,
            blksize: stat.st_blksize as u64,
            blocks: stat.st_blocks as u64,
            flags: stat.st_flags as u64,
            gen: stat.st_gen as u64,
        }
    }
}

impl Drop for FsRequest {
    fn drop(&mut self) {
        unsafe {
            if self.fired {
                uvll::uv_fs_req_cleanup(self.req);
            }
            uvll::free_req(self.req);
        }
    }
}

fn execute(f: |*mut uvll::uv_fs_t, uvll::uv_fs_cb| -> c_int)
    -> Result<FsRequest, UvError>
{
    let mut req = FsRequest {
        fired: false,
        req: unsafe { uvll::malloc_req(uvll::UV_FS) }
    };
    return match f(req.req, fs_cb) {
        0 => {
            req.fired = true;
            let mut slot = None;
            let loop_ = unsafe { uvll::get_loop_from_fs_req(req.req) };
            wait_until_woken_after(&mut slot, &Loop::wrap(loop_), || {
                unsafe { uvll::set_data_for_req(req.req, &mut slot) }
            });
            match req.get_result() {
                n if n < 0 => Err(UvError(n as i32)),
                _ => Ok(req),
            }
        }
        n => Err(UvError(n))
    };

    extern fn fs_cb(req: *mut uvll::uv_fs_t) {
        let slot: &mut Option<BlockedTask> = unsafe {
            mem::transmute(uvll::get_data_for_req(req))
        };
        wakeup(slot);
    }
}

fn execute_nop(f: |*mut uvll::uv_fs_t, uvll::uv_fs_cb| -> c_int)
    -> Result<(), UvError> {
    execute(f).map(|_| {})
}

impl HomingIO for FileWatcher {
    fn home<'r>(&'r mut self) -> &'r mut HomeHandle { &mut self.home }
}

impl FileWatcher {
    pub fn new(io: &mut UvIoFactory, fd: c_int,
               close: rtio::CloseBehavior) -> FileWatcher {
        FileWatcher {
            loop_: Loop::wrap(io.uv_loop()),
            fd: fd,
            close: close,
            home: io.make_handle(),
        }
    }

    fn base_read(&mut self, buf: &mut [u8], offset: i64) -> IoResult<int> {
        let _m = self.fire_homing_missile();
        let r = FsRequest::read(&self.loop_, self.fd, buf, offset);
        r.map_err(uv_error_to_io_error)
    }
    fn base_write(&mut self, buf: &[u8], offset: i64) -> IoResult<()> {
        let _m = self.fire_homing_missile();
        let r = FsRequest::write(&self.loop_, self.fd, buf, offset);
        r.map_err(uv_error_to_io_error)
    }
    fn seek_common(&self, pos: i64, whence: c_int) -> IoResult<u64>{
        match unsafe { libc::lseek(self.fd, pos as libc::off_t, whence) } {
            -1 => {
                Err(IoError {
                    code: os::errno() as uint,
                    extra: 0,
                    detail: None,
                })
            },
            n => Ok(n as u64)
        }
    }
}

impl Drop for FileWatcher {
    fn drop(&mut self) {
        let _m = self.fire_homing_missile();
        match self.close {
            rtio::DontClose => {}
            rtio::CloseAsynchronously => {
                unsafe {
                    let req = uvll::malloc_req(uvll::UV_FS);
                    assert_eq!(uvll::uv_fs_close(self.loop_.handle, req,
                                                 self.fd, close_cb), 0);
                }

                extern fn close_cb(req: *mut uvll::uv_fs_t) {
                    unsafe {
                        uvll::uv_fs_req_cleanup(req);
                        uvll::free_req(req);
                    }
                }
            }
            rtio::CloseSynchronously => {
                let _ = execute_nop(|req, cb| unsafe {
                    uvll::uv_fs_close(self.loop_.handle, req, self.fd, cb)
                });
            }
        }
    }
}

impl rtio::RtioFileStream for FileWatcher {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<int> {
        self.base_read(buf, -1)
    }
    fn write(&mut self, buf: &[u8]) -> IoResult<()> {
        self.base_write(buf, -1)
    }
    fn pread(&mut self, buf: &mut [u8], offset: u64) -> IoResult<int> {
        self.base_read(buf, offset as i64)
    }
    fn pwrite(&mut self, buf: &[u8], offset: u64) -> IoResult<()> {
        self.base_write(buf, offset as i64)
    }
    fn seek(&mut self, pos: i64, whence: rtio::SeekStyle) -> IoResult<u64> {
        use libc::{SEEK_SET, SEEK_CUR, SEEK_END};
        let whence = match whence {
            rtio::SeekSet => SEEK_SET,
            rtio::SeekCur => SEEK_CUR,
            rtio::SeekEnd => SEEK_END
        };
        self.seek_common(pos, whence)
    }
    fn tell(&self) -> IoResult<u64> {
        use libc::SEEK_CUR;

        self.seek_common(0, SEEK_CUR)
    }
    fn fsync(&mut self) -> IoResult<()> {
        let _m = self.fire_homing_missile();
        FsRequest::fsync(&self.loop_, self.fd).map_err(uv_error_to_io_error)
    }
    fn datasync(&mut self) -> IoResult<()> {
        let _m = self.fire_homing_missile();
        FsRequest::datasync(&self.loop_, self.fd).map_err(uv_error_to_io_error)
    }
    fn truncate(&mut self, offset: i64) -> IoResult<()> {
        let _m = self.fire_homing_missile();
        let r = FsRequest::truncate(&self.loop_, self.fd, offset);
        r.map_err(uv_error_to_io_error)
    }

    fn fstat(&mut self) -> IoResult<rtio::FileStat> {
        let _m = self.fire_homing_missile();
        FsRequest::fstat(&self.loop_, self.fd).map_err(uv_error_to_io_error)
    }
}

#[cfg(test)]
mod test {
    use libc::c_int;
    use libc::{O_CREAT, O_RDWR, O_RDONLY, S_IWUSR, S_IRUSR};
    use std::str;
    use super::FsRequest;
    use super::super::Loop;
    use super::super::local_loop;

    fn l() -> &mut Loop { &mut local_loop().loop_ }

    #[test]
    fn file_test_full_simple_sync() {
        let create_flags = O_RDWR | O_CREAT;
        let read_flags = O_RDONLY;
        let mode = S_IWUSR | S_IRUSR;
        let path_str = "./tmp/file_full_simple_sync.txt";

        {
            // open/create
            let result = FsRequest::open(local_loop(), &path_str.to_c_str(),
                                         create_flags as int, mode as int);
            assert!(result.is_ok());
            let result = result.unwrap();
            let fd = result.fd;

            // write
            let result = FsRequest::write(l(), fd, "hello".as_bytes(), -1);
            assert!(result.is_ok());
        }

        {
            // re-open
            let result = FsRequest::open(local_loop(), &path_str.to_c_str(),
                                         read_flags as int, 0);
            assert!(result.is_ok());
            let result = result.unwrap();
            let fd = result.fd;

            // read
            let mut read_mem = Vec::from_elem(1000, 0u8);
            let result = FsRequest::read(l(), fd, read_mem.as_mut_slice(), 0);
            assert!(result.is_ok());

            let nread = result.unwrap();
            assert!(nread > 0);
            let read_str = str::from_utf8(read_mem.slice_to(nread as uint)).unwrap();
            assert_eq!(read_str, "hello");
        }
        // unlink
        let result = FsRequest::unlink(l(), &path_str.to_c_str());
        assert!(result.is_ok());
    }

    #[test]
    fn file_test_stat() {
        let path = &"./tmp/file_test_stat_simple".to_c_str();
        let create_flags = (O_RDWR | O_CREAT) as int;
        let mode = (S_IWUSR | S_IRUSR) as int;

        let result = FsRequest::open(local_loop(), path, create_flags, mode);
        assert!(result.is_ok());
        let file = result.unwrap();

        let result = FsRequest::write(l(), file.fd, "hello".as_bytes(), 0);
        assert!(result.is_ok());

        let result = FsRequest::stat(l(), path);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().size, 5);

        let result = FsRequest::fstat(l(), file.fd);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().size, 5);

        fn free<T>(_: T) {}
        free(file);

        let result = FsRequest::unlink(l(), path);
        assert!(result.is_ok());
    }

    #[test]
    fn file_test_mk_rm_dir() {
        let path = &"./tmp/mk_rm_dir".to_c_str();
        let mode = S_IWUSR | S_IRUSR;

        let result = FsRequest::mkdir(l(), path, mode);
        assert!(result.is_ok());

        let result = FsRequest::rmdir(l(), path);
        assert!(result.is_ok());

        let result = FsRequest::stat(l(), path);
        assert!(result.is_err());
    }

    #[test]
    fn file_test_mkdir_chokes_on_double_create() {
        let path = &"./tmp/double_create_dir".to_c_str();
        let mode = S_IWUSR | S_IRUSR;

        let result = FsRequest::stat(l(), path);
        assert!(result.is_err(), "{:?}", result);
        let result = FsRequest::mkdir(l(), path, mode as c_int);
        assert!(result.is_ok(), "{:?}", result);
        let result = FsRequest::mkdir(l(), path, mode as c_int);
        assert!(result.is_err(), "{:?}", result);
        let result = FsRequest::rmdir(l(), path);
        assert!(result.is_ok(), "{:?}", result);
    }

    #[test]
    fn file_test_rmdir_chokes_on_nonexistant_path() {
        let path = &"./tmp/never_existed_dir".to_c_str();
        let result = FsRequest::rmdir(l(), path);
        assert!(result.is_err());
    }
}
