// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Various utility functions used throughout rustbuild.
//!
//! Simple things like testing the various filesystem operations here and there,
//! not a lot of interesting happenings here unfortunately.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use filetime::{self, FileTime};

/// Returns the `name` as the filename of a static library for `target`.
pub fn staticlib(name: &str, target: &str) -> String {
    if target.contains("windows") {
        format!("{}.lib", name)
    } else {
        format!("lib{}.a", name)
    }
}

/// Copies a file from `src` to `dst`, attempting to use hard links and then
/// falling back to an actually filesystem copy if necessary.
pub fn copy(src: &Path, dst: &Path) {
    // A call to `hard_link` will fail if `dst` exists, so remove it if it
    // already exists so we can try to help `hard_link` succeed.
    let _ = fs::remove_file(&dst);

    // Attempt to "easy copy" by creating a hard link (symlinks don't work on
    // windows), but if that fails just fall back to a slow `copy` operation.
    // let res = fs::hard_link(src, dst);
    let res = fs::copy(src, dst);
    if let Err(e) = res {
        panic!("failed to copy `{}` to `{}`: {}", src.display(),
               dst.display(), e)
    }
    let metadata = t!(src.metadata());
    t!(fs::set_permissions(dst, metadata.permissions()));
    let atime = FileTime::from_last_access_time(&metadata);
    let mtime = FileTime::from_last_modification_time(&metadata);
    t!(filetime::set_file_times(dst, atime, mtime));

}

/// Copies the `src` directory recursively to `dst`. Both are assumed to exist
/// when this function is called.
pub fn cp_r(src: &Path, dst: &Path) {
    for f in t!(fs::read_dir(src)) {
        let f = t!(f);
        let path = f.path();
        let name = path.file_name().unwrap();
        let dst = dst.join(name);
        if t!(f.file_type()).is_dir() {
            t!(fs::create_dir_all(&dst));
            cp_r(&path, &dst);
        } else {
            let _ = fs::remove_file(&dst);
            copy(&path, &dst);
        }
    }
}

/// Copies the `src` directory recursively to `dst`. Both are assumed to exist
/// when this function is called. Unwanted files or directories can be skipped
/// by returning `false` from the filter function.
pub fn cp_filtered(src: &Path, dst: &Path, filter: &Fn(&Path) -> bool) {
    // Inner function does the actual work
    fn recurse(src: &Path, dst: &Path, relative: &Path, filter: &Fn(&Path) -> bool) {
        for f in t!(fs::read_dir(src)) {
            let f = t!(f);
            let path = f.path();
            let name = path.file_name().unwrap();
            let dst = dst.join(name);
            let relative = relative.join(name);
            // Only copy file or directory if the filter function returns true
            if filter(&relative) {
                if t!(f.file_type()).is_dir() {
                    let _ = fs::remove_dir_all(&dst);
                    t!(fs::create_dir(&dst));
                    recurse(&path, &dst, &relative, filter);
                } else {
                    let _ = fs::remove_file(&dst);
                    copy(&path, &dst);
                }
            }
        }
    }
    // Immediately recurse with an empty relative path
    recurse(src, dst, Path::new(""), filter)
}

/// Given an executable called `name`, return the filename for the
/// executable for a particular target.
pub fn exe(name: &str, target: &str) -> String {
    if target.contains("windows") {
        format!("{}.exe", name)
    } else {
        name.to_string()
    }
}

/// Returns whether the file name given looks like a dynamic library.
pub fn is_dylib(name: &str) -> bool {
    name.ends_with(".dylib") || name.ends_with(".so") || name.ends_with(".dll")
}

/// Returns the corresponding relative library directory that the compiler's
/// dylibs will be found in.
pub fn libdir(target: &str) -> &'static str {
    if target.contains("windows") {"bin"} else {"lib"}
}

/// Adds a list of lookup paths to `cmd`'s dynamic library lookup path.
pub fn add_lib_path(path: Vec<PathBuf>, cmd: &mut Command) {
    let mut list = dylib_path();
    for path in path {
        list.insert(0, path);
    }
    cmd.env(dylib_path_var(), t!(env::join_paths(list)));
}

/// Returns the environment variable which the dynamic library lookup path
/// resides in for this platform.
pub fn dylib_path_var() -> &'static str {
    if cfg!(target_os = "windows") {
        "PATH"
    } else if cfg!(target_os = "macos") {
        "DYLD_LIBRARY_PATH"
    } else {
        "LD_LIBRARY_PATH"
    }
}

/// Parses the `dylib_path_var()` environment variable, returning a list of
/// paths that are members of this lookup path.
pub fn dylib_path() -> Vec<PathBuf> {
    env::split_paths(&env::var_os(dylib_path_var()).unwrap_or(OsString::new()))
        .collect()
}

/// `push` all components to `buf`. On windows, append `.exe` to the last component.
pub fn push_exe_path(mut buf: PathBuf, components: &[&str]) -> PathBuf {
    let (&file, components) = components.split_last().expect("at least one component required");
    let mut file = file.to_owned();

    if cfg!(windows) {
        file.push_str(".exe");
    }

    for c in components {
        buf.push(c);
    }

    buf.push(file);

    buf
}

pub struct TimeIt(Instant);

/// Returns an RAII structure that prints out how long it took to drop.
pub fn timeit() -> TimeIt {
    TimeIt(Instant::now())
}

impl Drop for TimeIt {
    fn drop(&mut self) {
        let time = self.0.elapsed();
        println!("\tfinished in {}.{:03}",
                 time.as_secs(),
                 time.subsec_nanos() / 1_000_000);
    }
}

/// Symlinks two directories, using junctions on Windows and normal symlinks on
/// Unix.
pub fn symlink_dir(src: &Path, dest: &Path) -> io::Result<()> {
    let _ = fs::remove_dir(dest);
    return symlink_dir_inner(src, dest);

    #[cfg(not(windows))]
    fn symlink_dir_inner(src: &Path, dest: &Path) -> io::Result<()> {
        use std::os::unix::fs;
        fs::symlink(src, dest)
    }

    // Creating a directory junction on windows involves dealing with reparse
    // points and the DeviceIoControl function, and this code is a skeleton of
    // what can be found here:
    //
    // http://www.flexhex.com/docs/articles/hard-links.phtml
    //
    // Copied from std
    #[cfg(windows)]
    #[allow(bad_style)]
    fn symlink_dir_inner(target: &Path, junction: &Path) -> io::Result<()> {
        use std::ptr;
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        const MAXIMUM_REPARSE_DATA_BUFFER_SIZE: usize = 16 * 1024;
        const GENERIC_WRITE: DWORD = 0x40000000;
        const OPEN_EXISTING: DWORD = 3;
        const FILE_FLAG_OPEN_REPARSE_POINT: DWORD = 0x00200000;
        const FILE_FLAG_BACKUP_SEMANTICS: DWORD = 0x02000000;
        const FSCTL_SET_REPARSE_POINT: DWORD = 0x900a4;
        const IO_REPARSE_TAG_MOUNT_POINT: DWORD = 0xa0000003;
        const FILE_SHARE_DELETE: DWORD = 0x4;
        const FILE_SHARE_READ: DWORD = 0x1;
        const FILE_SHARE_WRITE: DWORD = 0x2;

        type BOOL = i32;
        type DWORD = u32;
        type HANDLE = *mut u8;
        type LPCWSTR = *const u16;
        type LPDWORD = *mut DWORD;
        type LPOVERLAPPED = *mut u8;
        type LPSECURITY_ATTRIBUTES = *mut u8;
        type LPVOID = *mut u8;
        type WCHAR = u16;
        type WORD = u16;

        #[repr(C)]
        struct REPARSE_MOUNTPOINT_DATA_BUFFER {
            ReparseTag: DWORD,
            ReparseDataLength: DWORD,
            Reserved: WORD,
            ReparseTargetLength: WORD,
            ReparseTargetMaximumLength: WORD,
            Reserved1: WORD,
            ReparseTarget: WCHAR,
        }

        extern "system" {
            fn CreateFileW(lpFileName: LPCWSTR,
                           dwDesiredAccess: DWORD,
                           dwShareMode: DWORD,
                           lpSecurityAttributes: LPSECURITY_ATTRIBUTES,
                           dwCreationDisposition: DWORD,
                           dwFlagsAndAttributes: DWORD,
                           hTemplateFile: HANDLE)
                           -> HANDLE;
            fn DeviceIoControl(hDevice: HANDLE,
                               dwIoControlCode: DWORD,
                               lpInBuffer: LPVOID,
                               nInBufferSize: DWORD,
                               lpOutBuffer: LPVOID,
                               nOutBufferSize: DWORD,
                               lpBytesReturned: LPDWORD,
                               lpOverlapped: LPOVERLAPPED) -> BOOL;
        }

        fn to_u16s<S: AsRef<OsStr>>(s: S) -> io::Result<Vec<u16>> {
            Ok(s.as_ref().encode_wide().chain(Some(0)).collect())
        }

        // We're using low-level APIs to create the junction, and these are more
        // picky about paths. For example, forward slashes cannot be used as a
        // path separator, so we should try to canonicalize the path first.
        let target = try!(fs::canonicalize(target));

        try!(fs::create_dir(junction));

        let path = try!(to_u16s(junction));

        unsafe {
            let h = CreateFileW(path.as_ptr(),
                                GENERIC_WRITE,
                                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                                0 as *mut _,
                                OPEN_EXISTING,
                                FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
                                ptr::null_mut());

            let mut data = [0u8; MAXIMUM_REPARSE_DATA_BUFFER_SIZE];
            let mut db = data.as_mut_ptr()
                            as *mut REPARSE_MOUNTPOINT_DATA_BUFFER;
            let buf = &mut (*db).ReparseTarget as *mut _;
            let mut i = 0;
            // FIXME: this conversion is very hacky
            let v = br"\??\";
            let v = v.iter().map(|x| *x as u16);
            for c in v.chain(target.as_os_str().encode_wide().skip(4)) {
                *buf.offset(i) = c;
                i += 1;
            }
            *buf.offset(i) = 0;
            i += 1;
            (*db).ReparseTag = IO_REPARSE_TAG_MOUNT_POINT;
            (*db).ReparseTargetMaximumLength = (i * 2) as WORD;
            (*db).ReparseTargetLength = ((i - 1) * 2) as WORD;
            (*db).ReparseDataLength =
                    (*db).ReparseTargetLength as DWORD + 12;

            let mut ret = 0;
            let res = DeviceIoControl(h as *mut _,
                                      FSCTL_SET_REPARSE_POINT,
                                      data.as_ptr() as *mut _,
                                      (*db).ReparseDataLength + 8,
                                      ptr::null_mut(), 0,
                                      &mut ret,
                                      ptr::null_mut());

            if res == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
    }
}
