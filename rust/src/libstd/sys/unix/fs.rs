// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use prelude::v1::*;
use io::prelude::*;
use os::unix::prelude::*;

use ffi::{CString, CStr, OsString, OsStr};
use fmt;
use io::{self, Error, ErrorKind, SeekFrom};
use libc::{self, dirent, c_int, off_t, mode_t};
use mem;
use path::{Path, PathBuf};
use ptr;
use sync::Arc;
use sys::fd::FileDesc;
use sys::time::SystemTime;
use sys::{cvt, cvt_r};
use sys_common::{AsInner, FromInner};

#[cfg(target_os = "linux")]
use libc::{stat64, fstat64, lstat64};
#[cfg(not(target_os = "linux"))]
use libc::{stat as stat64, fstat as fstat64, lstat as lstat64};

pub struct File(FileDesc);

#[derive(Clone)]
pub struct FileAttr {
    stat: stat64,
}

pub struct ReadDir {
    dirp: Dir,
    root: Arc<PathBuf>,
}

struct Dir(*mut libc::DIR);

unsafe impl Send for Dir {}
unsafe impl Sync for Dir {}

pub struct DirEntry {
    entry: dirent,
    root: Arc<PathBuf>,
    // We need to store an owned copy of the directory name
    // on Solaris because a) it uses a zero-length array to
    // store the name, b) its lifetime between readdir calls
    // is not guaranteed.
    #[cfg(target_os = "solaris")]
    name: Box<[u8]>
}

#[derive(Clone)]
pub struct OpenOptions {
    // generic
    read: bool,
    write: bool,
    append: bool,
    truncate: bool,
    create: bool,
    create_new: bool,
    // system-specific
    custom_flags: i32,
    mode: mode_t,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FilePermissions { mode: mode_t }

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct FileType { mode: mode_t }

pub struct DirBuilder { mode: mode_t }

impl FileAttr {
    pub fn size(&self) -> u64 { self.stat.st_size as u64 }
    pub fn perm(&self) -> FilePermissions {
        FilePermissions { mode: (self.stat.st_mode as mode_t) & 0o777 }
    }

    pub fn file_type(&self) -> FileType {
        FileType { mode: self.stat.st_mode as mode_t }
    }
}

#[cfg(any(target_os = "ios", target_os = "macos"))]
// FIXME: update SystemTime to store a timespec and don't lose precision
impl FileAttr {
    pub fn modified(&self) -> io::Result<SystemTime> {
        Ok(SystemTime::from(libc::timeval {
            tv_sec: self.stat.st_mtime,
            tv_usec: (self.stat.st_mtime_nsec / 1000) as libc::suseconds_t,
        }))
    }

    pub fn accessed(&self) -> io::Result<SystemTime> {
        Ok(SystemTime::from(libc::timeval {
            tv_sec: self.stat.st_atime,
            tv_usec: (self.stat.st_atime_nsec / 1000) as libc::suseconds_t,
        }))
    }

    pub fn created(&self) -> io::Result<SystemTime> {
        Ok(SystemTime::from(libc::timeval {
            tv_sec: self.stat.st_birthtime,
            tv_usec: (self.stat.st_birthtime_nsec / 1000) as libc::suseconds_t,
        }))
    }
}

#[cfg(not(any(target_os = "ios", target_os = "macos")))]
impl FileAttr {
    pub fn modified(&self) -> io::Result<SystemTime> {
        Ok(SystemTime::from(libc::timespec {
            tv_sec: self.stat.st_mtime as libc::time_t,
            tv_nsec: self.stat.st_mtime_nsec as libc::c_long,
        }))
    }

    pub fn accessed(&self) -> io::Result<SystemTime> {
        Ok(SystemTime::from(libc::timespec {
            tv_sec: self.stat.st_atime as libc::time_t,
            tv_nsec: self.stat.st_atime_nsec as libc::c_long,
        }))
    }

    #[cfg(any(target_os = "bitrig",
              target_os = "freebsd",
              target_os = "openbsd"))]
    pub fn created(&self) -> io::Result<SystemTime> {
        Ok(SystemTime::from(libc::timespec {
            tv_sec: self.stat.st_birthtime as libc::time_t,
            tv_nsec: self.stat.st_birthtime_nsec as libc::c_long,
        }))
    }

    #[cfg(not(any(target_os = "bitrig",
                  target_os = "freebsd",
                  target_os = "openbsd")))]
    pub fn created(&self) -> io::Result<SystemTime> {
        Err(io::Error::new(io::ErrorKind::Other,
                           "creation time is not available on this platform \
                            currently"))
    }
}

impl AsInner<stat64> for FileAttr {
    fn as_inner(&self) -> &stat64 { &self.stat }
}

impl FilePermissions {
    pub fn readonly(&self) -> bool { self.mode & 0o222 == 0 }
    pub fn set_readonly(&mut self, readonly: bool) {
        if readonly {
            self.mode &= !0o222;
        } else {
            self.mode |= 0o222;
        }
    }
    pub fn mode(&self) -> u32 { self.mode as u32 }
}

impl FileType {
    pub fn is_dir(&self) -> bool { self.is(libc::S_IFDIR) }
    pub fn is_file(&self) -> bool { self.is(libc::S_IFREG) }
    pub fn is_symlink(&self) -> bool { self.is(libc::S_IFLNK) }

    pub fn is(&self, mode: mode_t) -> bool { self.mode & libc::S_IFMT == mode }
}

impl FromInner<u32> for FilePermissions {
    fn from_inner(mode: u32) -> FilePermissions {
        FilePermissions { mode: mode as mode_t }
    }
}

impl Iterator for ReadDir {
    type Item = io::Result<DirEntry>;

    #[cfg(target_os = "solaris")]
    fn next(&mut self) -> Option<io::Result<DirEntry>> {
        unsafe {
            loop {
                // Although readdir_r(3) would be a correct function to use here because
                // of the thread safety, on Illumos the readdir(3C) function is safe to use
                // in threaded applications and it is generally preferred over the
                // readdir_r(3C) function.
                let entry_ptr = libc::readdir(self.dirp.0);
                if entry_ptr.is_null() {
                    return None
                }

                let name = (*entry_ptr).d_name.as_ptr();
                let namelen = libc::strlen(name) as usize;

                let ret = DirEntry {
                    entry: *entry_ptr,
                    name: ::slice::from_raw_parts(name as *const u8,
                                                  namelen as usize).to_owned().into_boxed_slice(),
                    root: self.root.clone()
                };
                if ret.name_bytes() != b"." && ret.name_bytes() != b".." {
                    return Some(Ok(ret))
                }
            }
        }
    }

    #[cfg(not(target_os = "solaris"))]
    fn next(&mut self) -> Option<io::Result<DirEntry>> {
        unsafe {
            let mut ret = DirEntry {
                entry: mem::zeroed(),
                root: self.root.clone()
            };
            let mut entry_ptr = ptr::null_mut();
            loop {
                if libc::readdir_r(self.dirp.0, &mut ret.entry, &mut entry_ptr) != 0 {
                    return Some(Err(Error::last_os_error()))
                }
                if entry_ptr.is_null() {
                    return None
                }
                if ret.name_bytes() != b"." && ret.name_bytes() != b".." {
                    return Some(Ok(ret))
                }
            }
        }
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let r = unsafe { libc::closedir(self.0) };
        debug_assert_eq!(r, 0);
    }
}

impl DirEntry {
    pub fn path(&self) -> PathBuf {
        self.root.join(OsStr::from_bytes(self.name_bytes()))
    }

    pub fn file_name(&self) -> OsString {
        OsStr::from_bytes(self.name_bytes()).to_os_string()
    }

    pub fn metadata(&self) -> io::Result<FileAttr> {
        lstat(&self.path())
    }

    #[cfg(target_os = "solaris")]
    pub fn file_type(&self) -> io::Result<FileType> {
        stat(&self.path()).map(|m| m.file_type())
    }

    #[cfg(not(target_os = "solaris"))]
    pub fn file_type(&self) -> io::Result<FileType> {
        match self.entry.d_type {
            libc::DT_CHR => Ok(FileType { mode: libc::S_IFCHR }),
            libc::DT_FIFO => Ok(FileType { mode: libc::S_IFIFO }),
            libc::DT_LNK => Ok(FileType { mode: libc::S_IFLNK }),
            libc::DT_REG => Ok(FileType { mode: libc::S_IFREG }),
            libc::DT_SOCK => Ok(FileType { mode: libc::S_IFSOCK }),
            libc::DT_DIR => Ok(FileType { mode: libc::S_IFDIR }),
            libc::DT_BLK => Ok(FileType { mode: libc::S_IFBLK }),
            _ => lstat(&self.path()).map(|m| m.file_type()),
        }
    }

    #[cfg(any(target_os = "macos",
              target_os = "ios",
              target_os = "linux",
              target_os = "emscripten",
              target_os = "android",
              target_os = "solaris"))]
    pub fn ino(&self) -> u64 {
        self.entry.d_ino as u64
    }

    #[cfg(any(target_os = "freebsd",
              target_os = "openbsd",
              target_os = "bitrig",
              target_os = "netbsd",
              target_os = "dragonfly"))]
    pub fn ino(&self) -> u64 {
        self.entry.d_fileno as u64
    }

    #[cfg(any(target_os = "macos",
              target_os = "ios",
              target_os = "netbsd",
              target_os = "openbsd",
              target_os = "freebsd",
              target_os = "dragonfly",
              target_os = "bitrig"))]
    fn name_bytes(&self) -> &[u8] {
        unsafe {
            ::slice::from_raw_parts(self.entry.d_name.as_ptr() as *const u8,
                                    self.entry.d_namlen as usize)
        }
    }
    #[cfg(any(target_os = "android",
              target_os = "linux",
              target_os = "emscripten"))]
    fn name_bytes(&self) -> &[u8] {
        unsafe {
            CStr::from_ptr(self.entry.d_name.as_ptr()).to_bytes()
        }
    }
    #[cfg(target_os = "solaris")]
    fn name_bytes(&self) -> &[u8] {
        &*self.name
    }
}

impl OpenOptions {
    pub fn new() -> OpenOptions {
        OpenOptions {
            // generic
            read: false,
            write: false,
            append: false,
            truncate: false,
            create: false,
            create_new: false,
            // system-specific
            custom_flags: 0,
            mode: 0o666,
        }
    }

    pub fn read(&mut self, read: bool) { self.read = read; }
    pub fn write(&mut self, write: bool) { self.write = write; }
    pub fn append(&mut self, append: bool) { self.append = append; }
    pub fn truncate(&mut self, truncate: bool) { self.truncate = truncate; }
    pub fn create(&mut self, create: bool) { self.create = create; }
    pub fn create_new(&mut self, create_new: bool) { self.create_new = create_new; }

    pub fn custom_flags(&mut self, flags: i32) { self.custom_flags = flags; }
    pub fn mode(&mut self, mode: u32) { self.mode = mode as mode_t; }

    fn get_access_mode(&self) -> io::Result<c_int> {
        match (self.read, self.write, self.append) {
            (true,  false, false) => Ok(libc::O_RDONLY),
            (false, true,  false) => Ok(libc::O_WRONLY),
            (true,  true,  false) => Ok(libc::O_RDWR),
            (false, _,     true)  => Ok(libc::O_WRONLY | libc::O_APPEND),
            (true,  _,     true)  => Ok(libc::O_RDWR | libc::O_APPEND),
            (false, false, false) => Err(Error::from_raw_os_error(libc::EINVAL)),
        }
    }

    fn get_creation_mode(&self) -> io::Result<c_int> {
        match (self.write, self.append) {
            (true, false) => {}
            (false, false) =>
                if self.truncate || self.create || self.create_new {
                    return Err(Error::from_raw_os_error(libc::EINVAL));
                },
            (_, true) =>
                if self.truncate && !self.create_new {
                    return Err(Error::from_raw_os_error(libc::EINVAL));
                },
        }

        Ok(match (self.create, self.truncate, self.create_new) {
                (false, false, false) => 0,
                (true,  false, false) => libc::O_CREAT,
                (false, true,  false) => libc::O_TRUNC,
                (true,  true,  false) => libc::O_CREAT | libc::O_TRUNC,
                (_,      _,    true)  => libc::O_CREAT | libc::O_EXCL,
           })
    }
}

impl File {
    pub fn open(path: &Path, opts: &OpenOptions) -> io::Result<File> {
        let path = try!(cstr(path));
        File::open_c(&path, opts)
    }

    pub fn open_c(path: &CStr, opts: &OpenOptions) -> io::Result<File> {
        let flags = libc::O_CLOEXEC |
                    try!(opts.get_access_mode()) |
                    try!(opts.get_creation_mode()) |
                    (opts.custom_flags as c_int & !libc::O_ACCMODE);
        let fd = try!(cvt_r(|| unsafe {
            libc::open(path.as_ptr(), flags, opts.mode as c_int)
        }));
        let fd = FileDesc::new(fd);

        // Currently the standard library supports Linux 2.6.18 which did not
        // have the O_CLOEXEC flag (passed above). If we're running on an older
        // Linux kernel then the flag is just ignored by the OS, so we continue
        // to explicitly ask for a CLOEXEC fd here.
        //
        // The CLOEXEC flag, however, is supported on versions of OSX/BSD/etc
        // that we support, so we only do this on Linux currently.
        if cfg!(target_os = "linux") {
            fd.set_cloexec();
        }

        Ok(File(fd))
    }

    pub fn file_attr(&self) -> io::Result<FileAttr> {
        let mut stat: stat64 = unsafe { mem::zeroed() };
        try!(cvt(unsafe {
            fstat64(self.0.raw(), &mut stat)
        }));
        Ok(FileAttr { stat: stat })
    }

    pub fn fsync(&self) -> io::Result<()> {
        try!(cvt_r(|| unsafe { libc::fsync(self.0.raw()) }));
        Ok(())
    }

    pub fn datasync(&self) -> io::Result<()> {
        try!(cvt_r(|| unsafe { os_datasync(self.0.raw()) }));
        return Ok(());

        #[cfg(any(target_os = "macos", target_os = "ios"))]
        unsafe fn os_datasync(fd: c_int) -> c_int {
            libc::fcntl(fd, libc::F_FULLFSYNC)
        }
        #[cfg(target_os = "linux")]
        unsafe fn os_datasync(fd: c_int) -> c_int { libc::fdatasync(fd) }
        #[cfg(not(any(target_os = "macos",
                      target_os = "ios",
                      target_os = "linux")))]
        unsafe fn os_datasync(fd: c_int) -> c_int { libc::fsync(fd) }
    }

    pub fn truncate(&self, size: u64) -> io::Result<()> {
        try!(cvt_r(|| unsafe {
            libc::ftruncate(self.0.raw(), size as libc::off_t)
        }));
        Ok(())
    }

    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }

    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    pub fn flush(&self) -> io::Result<()> { Ok(()) }

    pub fn seek(&self, pos: SeekFrom) -> io::Result<u64> {
        let (whence, pos) = match pos {
            SeekFrom::Start(off) => (libc::SEEK_SET, off as off_t),
            SeekFrom::End(off) => (libc::SEEK_END, off as off_t),
            SeekFrom::Current(off) => (libc::SEEK_CUR, off as off_t),
        };
        let n = try!(cvt(unsafe { libc::lseek(self.0.raw(), pos, whence) }));
        Ok(n as u64)
    }

    pub fn duplicate(&self) -> io::Result<File> {
        self.0.duplicate().map(File)
    }

    pub fn fd(&self) -> &FileDesc { &self.0 }

    pub fn into_fd(self) -> FileDesc { self.0 }
}

impl DirBuilder {
    pub fn new() -> DirBuilder {
        DirBuilder { mode: 0o777 }
    }

    pub fn mkdir(&self, p: &Path) -> io::Result<()> {
        let p = try!(cstr(p));
        try!(cvt(unsafe { libc::mkdir(p.as_ptr(), self.mode) }));
        Ok(())
    }

    pub fn set_mode(&mut self, mode: u32) {
        self.mode = mode as mode_t;
    }
}

fn cstr(path: &Path) -> io::Result<CString> {
    Ok(try!(CString::new(path.as_os_str().as_bytes())))
}

impl FromInner<c_int> for File {
    fn from_inner(fd: c_int) -> File {
        File(FileDesc::new(fd))
    }
}

impl fmt::Debug for File {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        #[cfg(target_os = "linux")]
        fn get_path(fd: c_int) -> Option<PathBuf> {
            use string::ToString;
            let mut p = PathBuf::from("/proc/self/fd");
            p.push(&fd.to_string());
            readlink(&p).ok()
        }

        #[cfg(target_os = "macos")]
        fn get_path(fd: c_int) -> Option<PathBuf> {
            // FIXME: The use of PATH_MAX is generally not encouraged, but it
            // is inevitable in this case because OS X defines `fcntl` with
            // `F_GETPATH` in terms of `MAXPATHLEN`, and there are no
            // alternatives. If a better method is invented, it should be used
            // instead.
            let mut buf = vec![0;libc::PATH_MAX as usize];
            let n = unsafe { libc::fcntl(fd, libc::F_GETPATH, buf.as_ptr()) };
            if n == -1 {
                return None;
            }
            let l = buf.iter().position(|&c| c == 0).unwrap();
            buf.truncate(l as usize);
            buf.shrink_to_fit();
            Some(PathBuf::from(OsString::from_vec(buf)))
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        fn get_path(_fd: c_int) -> Option<PathBuf> {
            // FIXME(#24570): implement this for other Unix platforms
            None
        }

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        fn get_mode(fd: c_int) -> Option<(bool, bool)> {
            let mode = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if mode == -1 {
                return None;
            }
            match mode & libc::O_ACCMODE {
                libc::O_RDONLY => Some((true, false)),
                libc::O_RDWR => Some((true, true)),
                libc::O_WRONLY => Some((false, true)),
                _ => None
            }
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        fn get_mode(_fd: c_int) -> Option<(bool, bool)> {
            // FIXME(#24570): implement this for other Unix platforms
            None
        }

        let fd = self.0.raw();
        let mut b = f.debug_struct("File");
        b.field("fd", &fd);
        if let Some(path) = get_path(fd) {
            b.field("path", &path);
        }
        if let Some((read, write)) = get_mode(fd) {
            b.field("read", &read).field("write", &write);
        }
        b.finish()
    }
}

pub fn readdir(p: &Path) -> io::Result<ReadDir> {
    let root = Arc::new(p.to_path_buf());
    let p = try!(cstr(p));
    unsafe {
        let ptr = libc::opendir(p.as_ptr());
        if ptr.is_null() {
            Err(Error::last_os_error())
        } else {
            Ok(ReadDir { dirp: Dir(ptr), root: root })
        }
    }
}

pub fn unlink(p: &Path) -> io::Result<()> {
    let p = try!(cstr(p));
    try!(cvt(unsafe { libc::unlink(p.as_ptr()) }));
    Ok(())
}

pub fn rename(old: &Path, new: &Path) -> io::Result<()> {
    let old = try!(cstr(old));
    let new = try!(cstr(new));
    try!(cvt(unsafe { libc::rename(old.as_ptr(), new.as_ptr()) }));
    Ok(())
}

pub fn set_perm(p: &Path, perm: FilePermissions) -> io::Result<()> {
    let p = try!(cstr(p));
    try!(cvt_r(|| unsafe { libc::chmod(p.as_ptr(), perm.mode) }));
    Ok(())
}

pub fn rmdir(p: &Path) -> io::Result<()> {
    let p = try!(cstr(p));
    try!(cvt(unsafe { libc::rmdir(p.as_ptr()) }));
    Ok(())
}

pub fn remove_dir_all(path: &Path) -> io::Result<()> {
    let filetype = try!(lstat(path)).file_type();
    if filetype.is_symlink() {
        unlink(path)
    } else {
        remove_dir_all_recursive(path)
    }
}

fn remove_dir_all_recursive(path: &Path) -> io::Result<()> {
    for child in try!(readdir(path)) {
        let child = try!(child);
        if try!(child.file_type()).is_dir() {
            try!(remove_dir_all_recursive(&child.path()));
        } else {
            try!(unlink(&child.path()));
        }
    }
    rmdir(path)
}

pub fn readlink(p: &Path) -> io::Result<PathBuf> {
    let c_path = try!(cstr(p));
    let p = c_path.as_ptr();

    let mut buf = Vec::with_capacity(256);

    loop {
        let buf_read = try!(cvt(unsafe {
            libc::readlink(p, buf.as_mut_ptr() as *mut _, buf.capacity() as libc::size_t)
        })) as usize;

        unsafe { buf.set_len(buf_read); }

        if buf_read != buf.capacity() {
            buf.shrink_to_fit();

            return Ok(PathBuf::from(OsString::from_vec(buf)));
        }

        // Trigger the internal buffer resizing logic of `Vec` by requiring
        // more space than the current capacity. The length is guaranteed to be
        // the same as the capacity due to the if statement above.
        buf.reserve(1);
    }
}

pub fn symlink(src: &Path, dst: &Path) -> io::Result<()> {
    let src = try!(cstr(src));
    let dst = try!(cstr(dst));
    try!(cvt(unsafe { libc::symlink(src.as_ptr(), dst.as_ptr()) }));
    Ok(())
}

pub fn link(src: &Path, dst: &Path) -> io::Result<()> {
    let src = try!(cstr(src));
    let dst = try!(cstr(dst));
    try!(cvt(unsafe { libc::link(src.as_ptr(), dst.as_ptr()) }));
    Ok(())
}

pub fn stat(p: &Path) -> io::Result<FileAttr> {
    let p = try!(cstr(p));
    let mut stat: stat64 = unsafe { mem::zeroed() };
    try!(cvt(unsafe {
        stat64(p.as_ptr(), &mut stat as *mut _ as *mut _)
    }));
    Ok(FileAttr { stat: stat })
}

pub fn lstat(p: &Path) -> io::Result<FileAttr> {
    let p = try!(cstr(p));
    let mut stat: stat64 = unsafe { mem::zeroed() };
    try!(cvt(unsafe {
        lstat64(p.as_ptr(), &mut stat as *mut _ as *mut _)
    }));
    Ok(FileAttr { stat: stat })
}

pub fn canonicalize(p: &Path) -> io::Result<PathBuf> {
    let path = try!(CString::new(p.as_os_str().as_bytes()));
    let buf;
    unsafe {
        let r = libc::realpath(path.as_ptr(), ptr::null_mut());
        if r.is_null() {
            return Err(io::Error::last_os_error())
        }
        buf = CStr::from_ptr(r).to_bytes().to_vec();
        libc::free(r as *mut _);
    }
    Ok(PathBuf::from(OsString::from_vec(buf)))
}

pub fn copy(from: &Path, to: &Path) -> io::Result<u64> {
    use fs::{File, set_permissions};
    if !from.is_file() {
        return Err(Error::new(ErrorKind::InvalidInput,
                              "the source path is not an existing regular file"))
    }

    let mut reader = try!(File::open(from));
    let mut writer = try!(File::create(to));
    let perm = try!(reader.metadata()).permissions();

    let ret = try!(io::copy(&mut reader, &mut writer));
    try!(set_permissions(to, perm));
    Ok(ret)
}
