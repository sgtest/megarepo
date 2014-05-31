// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!
 * Low-level bindings to the libuv library.
 *
 * This module contains a set of direct, 'bare-metal' wrappers around
 * the libuv C-API.
 *
 * We're not bothering yet to redefine uv's structs as Rust structs
 * because they are quite large and change often between versions.
 * The maintenance burden is just too high. Instead we use the uv's
 * `uv_handle_size` and `uv_req_size` to find the correct size of the
 * structs and allocate them on the heap. This can be revisited later.
 *
 * There are also a collection of helper functions to ease interacting
 * with the low-level API.
 *
 * As new functionality, existent in uv.h, is added to the rust stdlib,
 * the mappings should be added in this module.
 */

#![allow(non_camel_case_types)] // C types

use libc::{size_t, c_int, c_uint, c_void, c_char, c_double};
use libc::{ssize_t, sockaddr, free, addrinfo};
use libc;
use std::rt::libc_heap::malloc_raw;

#[cfg(test)]
use libc::uintptr_t;

pub use self::errors::{EACCES, ECONNREFUSED, ECONNRESET, EPIPE, ECONNABORTED,
                       ECANCELED, EBADF, ENOTCONN, ENOENT, EADDRNOTAVAIL};

pub static OK: c_int = 0;
pub static EOF: c_int = -4095;
pub static UNKNOWN: c_int = -4094;

// uv-errno.h redefines error codes for windows, but not for unix...
// https://github.com/joyent/libuv/blob/master/include/uv-errno.h

#[cfg(windows)]
pub mod errors {
    use libc::c_int;

    pub static EACCES: c_int = -4092;
    pub static ECONNREFUSED: c_int = -4078;
    pub static ECONNRESET: c_int = -4077;
    pub static ENOENT: c_int = -4058;
    pub static ENOTCONN: c_int = -4053;
    pub static EPIPE: c_int = -4047;
    pub static ECONNABORTED: c_int = -4079;
    pub static ECANCELED: c_int = -4081;
    pub static EBADF: c_int = -4083;
    pub static EADDRNOTAVAIL: c_int = -4090;
}
#[cfg(not(windows))]
pub mod errors {
    use libc;
    use libc::c_int;

    pub static EACCES: c_int = -libc::EACCES;
    pub static ECONNREFUSED: c_int = -libc::ECONNREFUSED;
    pub static ECONNRESET: c_int = -libc::ECONNRESET;
    pub static ENOENT: c_int = -libc::ENOENT;
    pub static ENOTCONN: c_int = -libc::ENOTCONN;
    pub static EPIPE: c_int = -libc::EPIPE;
    pub static ECONNABORTED: c_int = -libc::ECONNABORTED;
    pub static ECANCELED : c_int = -libc::ECANCELED;
    pub static EBADF : c_int = -libc::EBADF;
    pub static EADDRNOTAVAIL : c_int = -libc::EADDRNOTAVAIL;
}

pub static PROCESS_SETUID: c_int = 1 << 0;
pub static PROCESS_SETGID: c_int = 1 << 1;
pub static PROCESS_WINDOWS_VERBATIM_ARGUMENTS: c_int = 1 << 2;
pub static PROCESS_DETACHED: c_int = 1 << 3;
pub static PROCESS_WINDOWS_HIDE: c_int = 1 << 4;

pub static STDIO_IGNORE: c_int = 0x00;
pub static STDIO_CREATE_PIPE: c_int = 0x01;
pub static STDIO_INHERIT_FD: c_int = 0x02;
pub static STDIO_INHERIT_STREAM: c_int = 0x04;
pub static STDIO_READABLE_PIPE: c_int = 0x10;
pub static STDIO_WRITABLE_PIPE: c_int = 0x20;

#[cfg(unix)]
pub type uv_buf_len_t = libc::size_t;
#[cfg(windows)]
pub type uv_buf_len_t = libc::c_ulong;

// see libuv/include/uv-unix.h
#[cfg(unix)]
pub struct uv_buf_t {
    pub base: *u8,
    pub len: uv_buf_len_t,
}

// see libuv/include/uv-win.h
#[cfg(windows)]
pub struct uv_buf_t {
    pub len: uv_buf_len_t,
    pub base: *u8,
}

#[repr(C)]
pub enum uv_run_mode {
    RUN_DEFAULT = 0,
    RUN_ONCE,
    RUN_NOWAIT,
}

pub struct uv_process_options_t {
    pub exit_cb: uv_exit_cb,
    pub file: *libc::c_char,
    pub args: **libc::c_char,
    pub env: **libc::c_char,
    pub cwd: *libc::c_char,
    pub flags: libc::c_uint,
    pub stdio_count: libc::c_int,
    pub stdio: *uv_stdio_container_t,
    pub uid: uv_uid_t,
    pub gid: uv_gid_t,
}

// These fields are private because they must be interfaced with through the
// functions below.
pub struct uv_stdio_container_t {
    flags: libc::c_int,
    stream: *uv_stream_t,
}

pub type uv_handle_t = c_void;
pub type uv_req_t = c_void;
pub type uv_loop_t = c_void;
pub type uv_idle_t = c_void;
pub type uv_tcp_t = c_void;
pub type uv_udp_t = c_void;
pub type uv_connect_t = c_void;
pub type uv_connection_t = c_void;
pub type uv_write_t = c_void;
pub type uv_async_t = c_void;
pub type uv_timer_t = c_void;
pub type uv_stream_t = c_void;
pub type uv_fs_t = c_void;
pub type uv_udp_send_t = c_void;
pub type uv_getaddrinfo_t = c_void;
pub type uv_process_t = c_void;
pub type uv_pipe_t = c_void;
pub type uv_tty_t = c_void;
pub type uv_signal_t = c_void;
pub type uv_shutdown_t = c_void;

pub struct uv_timespec_t {
    pub tv_sec: libc::c_long,
    pub tv_nsec: libc::c_long
}

pub struct uv_stat_t {
    pub st_dev: libc::uint64_t,
    pub st_mode: libc::uint64_t,
    pub st_nlink: libc::uint64_t,
    pub st_uid: libc::uint64_t,
    pub st_gid: libc::uint64_t,
    pub st_rdev: libc::uint64_t,
    pub st_ino: libc::uint64_t,
    pub st_size: libc::uint64_t,
    pub st_blksize: libc::uint64_t,
    pub st_blocks: libc::uint64_t,
    pub st_flags: libc::uint64_t,
    pub st_gen: libc::uint64_t,
    pub st_atim: uv_timespec_t,
    pub st_mtim: uv_timespec_t,
    pub st_ctim: uv_timespec_t,
    pub st_birthtim: uv_timespec_t
}

impl uv_stat_t {
    pub fn new() -> uv_stat_t {
        uv_stat_t {
            st_dev: 0,
            st_mode: 0,
            st_nlink: 0,
            st_uid: 0,
            st_gid: 0,
            st_rdev: 0,
            st_ino: 0,
            st_size: 0,
            st_blksize: 0,
            st_blocks: 0,
            st_flags: 0,
            st_gen: 0,
            st_atim: uv_timespec_t { tv_sec: 0, tv_nsec: 0 },
            st_mtim: uv_timespec_t { tv_sec: 0, tv_nsec: 0 },
            st_ctim: uv_timespec_t { tv_sec: 0, tv_nsec: 0 },
            st_birthtim: uv_timespec_t { tv_sec: 0, tv_nsec: 0 }
        }
    }
    pub fn is_file(&self) -> bool {
        ((self.st_mode) & libc::S_IFMT as libc::uint64_t) == libc::S_IFREG as libc::uint64_t
    }
    pub fn is_dir(&self) -> bool {
        ((self.st_mode) & libc::S_IFMT as libc::uint64_t) == libc::S_IFDIR as libc::uint64_t
    }
}

pub type uv_idle_cb = extern "C" fn(handle: *uv_idle_t);
pub type uv_alloc_cb = extern "C" fn(stream: *uv_stream_t,
                                     suggested_size: size_t,
                                     buf: *mut uv_buf_t);
pub type uv_read_cb = extern "C" fn(stream: *uv_stream_t,
                                    nread: ssize_t,
                                    buf: *uv_buf_t);
pub type uv_udp_send_cb = extern "C" fn(req: *uv_udp_send_t,
                                        status: c_int);
pub type uv_udp_recv_cb = extern "C" fn(handle: *uv_udp_t,
                                        nread: ssize_t,
                                        buf: *uv_buf_t,
                                        addr: *sockaddr,
                                        flags: c_uint);
pub type uv_close_cb = extern "C" fn(handle: *uv_handle_t);
pub type uv_walk_cb = extern "C" fn(handle: *uv_handle_t,
                                    arg: *c_void);
pub type uv_async_cb = extern "C" fn(handle: *uv_async_t);
pub type uv_connect_cb = extern "C" fn(handle: *uv_connect_t,
                                       status: c_int);
pub type uv_connection_cb = extern "C" fn(handle: *uv_connection_t,
                                          status: c_int);
pub type uv_timer_cb = extern "C" fn(handle: *uv_timer_t);
pub type uv_write_cb = extern "C" fn(handle: *uv_write_t,
                                     status: c_int);
pub type uv_getaddrinfo_cb = extern "C" fn(req: *uv_getaddrinfo_t,
                                           status: c_int,
                                           res: *addrinfo);
pub type uv_exit_cb = extern "C" fn(handle: *uv_process_t,
                                    exit_status: i64,
                                    term_signal: c_int);
pub type uv_signal_cb = extern "C" fn(handle: *uv_signal_t,
                                      signum: c_int);
pub type uv_fs_cb = extern "C" fn(req: *uv_fs_t);
pub type uv_shutdown_cb = extern "C" fn(req: *uv_shutdown_t, status: c_int);

#[cfg(unix)] pub type uv_uid_t = libc::types::os::arch::posix88::uid_t;
#[cfg(unix)] pub type uv_gid_t = libc::types::os::arch::posix88::gid_t;
#[cfg(windows)] pub type uv_uid_t = libc::c_uchar;
#[cfg(windows)] pub type uv_gid_t = libc::c_uchar;

#[repr(C)]
#[deriving(PartialEq)]
pub enum uv_handle_type {
    UV_UNKNOWN_HANDLE,
    UV_ASYNC,
    UV_CHECK,
    UV_FS_EVENT,
    UV_FS_POLL,
    UV_HANDLE,
    UV_IDLE,
    UV_NAMED_PIPE,
    UV_POLL,
    UV_PREPARE,
    UV_PROCESS,
    UV_STREAM,
    UV_TCP,
    UV_TIMER,
    UV_TTY,
    UV_UDP,
    UV_SIGNAL,
    UV_FILE,
    UV_HANDLE_TYPE_MAX
}

#[repr(C)]
#[cfg(unix)]
#[deriving(PartialEq)]
pub enum uv_req_type {
    UV_UNKNOWN_REQ,
    UV_REQ,
    UV_CONNECT,
    UV_WRITE,
    UV_SHUTDOWN,
    UV_UDP_SEND,
    UV_FS,
    UV_WORK,
    UV_GETADDRINFO,
    UV_REQ_TYPE_MAX
}

// uv_req_type may have additional fields defined by UV_REQ_TYPE_PRIVATE.
// See UV_REQ_TYPE_PRIVATE at libuv/include/uv-win.h
#[repr(C)]
#[cfg(windows)]
#[deriving(PartialEq)]
pub enum uv_req_type {
    UV_UNKNOWN_REQ,
    UV_REQ,
    UV_CONNECT,
    UV_WRITE,
    UV_SHUTDOWN,
    UV_UDP_SEND,
    UV_FS,
    UV_WORK,
    UV_GETADDRINFO,
    UV_ACCEPT,
    UV_FS_EVENT_REQ,
    UV_POLL_REQ,
    UV_PROCESS_EXIT,
    UV_READ,
    UV_UDP_RECV,
    UV_WAKEUP,
    UV_SIGNAL_REQ,
    UV_REQ_TYPE_MAX
}

#[repr(C)]
#[deriving(PartialEq)]
pub enum uv_membership {
    UV_LEAVE_GROUP,
    UV_JOIN_GROUP
}

pub unsafe fn malloc_handle(handle: uv_handle_type) -> *c_void {
    assert!(handle != UV_UNKNOWN_HANDLE && handle != UV_HANDLE_TYPE_MAX);
    let size = uv_handle_size(handle);
    malloc_raw(size as uint) as *c_void
}

pub unsafe fn free_handle(v: *c_void) {
    free(v as *mut c_void)
}

pub unsafe fn malloc_req(req: uv_req_type) -> *c_void {
    assert!(req != UV_UNKNOWN_REQ && req != UV_REQ_TYPE_MAX);
    let size = uv_req_size(req);
    malloc_raw(size as uint) as *c_void
}

pub unsafe fn free_req(v: *c_void) {
    free(v as *mut c_void)
}

#[test]
fn handle_sanity_check() {
    unsafe {
        assert_eq!(UV_HANDLE_TYPE_MAX as uint, rust_uv_handle_type_max());
    }
}

#[test]
fn request_sanity_check() {
    unsafe {
        assert_eq!(UV_REQ_TYPE_MAX as uint, rust_uv_req_type_max());
    }
}

// FIXME Event loops ignore SIGPIPE by default.
pub unsafe fn loop_new() -> *c_void {
    return rust_uv_loop_new();
}

pub unsafe fn uv_write(req: *uv_write_t,
                       stream: *uv_stream_t,
                       buf_in: &[uv_buf_t],
                       cb: uv_write_cb) -> c_int {
    extern {
        fn uv_write(req: *uv_write_t, stream: *uv_stream_t,
                    buf_in: *uv_buf_t, buf_cnt: c_int,
                    cb: uv_write_cb) -> c_int;
    }

    let buf_ptr = buf_in.as_ptr();
    let buf_cnt = buf_in.len() as i32;
    return uv_write(req, stream, buf_ptr, buf_cnt, cb);
}

pub unsafe fn uv_udp_send(req: *uv_udp_send_t,
                          handle: *uv_udp_t,
                          buf_in: &[uv_buf_t],
                          addr: *sockaddr,
                          cb: uv_udp_send_cb) -> c_int {
    extern {
        fn uv_udp_send(req: *uv_write_t, stream: *uv_stream_t,
                       buf_in: *uv_buf_t, buf_cnt: c_int, addr: *sockaddr,
                       cb: uv_udp_send_cb) -> c_int;
    }

    let buf_ptr = buf_in.as_ptr();
    let buf_cnt = buf_in.len() as i32;
    return uv_udp_send(req, handle, buf_ptr, buf_cnt, addr, cb);
}

pub unsafe fn get_udp_handle_from_send_req(send_req: *uv_udp_send_t) -> *uv_udp_t {
    return rust_uv_get_udp_handle_from_send_req(send_req);
}

pub unsafe fn process_pid(p: *uv_process_t) -> c_int {

    return rust_uv_process_pid(p);
}

pub unsafe fn set_stdio_container_flags(c: *uv_stdio_container_t,
                                        flags: libc::c_int) {

    rust_set_stdio_container_flags(c, flags);
}

pub unsafe fn set_stdio_container_fd(c: *uv_stdio_container_t,
                                     fd: libc::c_int) {

    rust_set_stdio_container_fd(c, fd);
}

pub unsafe fn set_stdio_container_stream(c: *uv_stdio_container_t,
                                         stream: *uv_stream_t) {
    rust_set_stdio_container_stream(c, stream);
}

// data access helpers
pub unsafe fn get_result_from_fs_req(req: *uv_fs_t) -> ssize_t {
    rust_uv_get_result_from_fs_req(req)
}
pub unsafe fn get_ptr_from_fs_req(req: *uv_fs_t) -> *libc::c_void {
    rust_uv_get_ptr_from_fs_req(req)
}
pub unsafe fn get_path_from_fs_req(req: *uv_fs_t) -> *c_char {
    rust_uv_get_path_from_fs_req(req)
}
pub unsafe fn get_loop_from_fs_req(req: *uv_fs_t) -> *uv_loop_t {
    rust_uv_get_loop_from_fs_req(req)
}
pub unsafe fn get_loop_from_getaddrinfo_req(req: *uv_getaddrinfo_t) -> *uv_loop_t {
    rust_uv_get_loop_from_getaddrinfo_req(req)
}
pub unsafe fn get_loop_for_uv_handle<T>(handle: *T) -> *c_void {
    return rust_uv_get_loop_for_uv_handle(handle as *c_void);
}
pub unsafe fn get_stream_handle_from_connect_req(connect: *uv_connect_t) -> *uv_stream_t {
    return rust_uv_get_stream_handle_from_connect_req(connect);
}
pub unsafe fn get_stream_handle_from_write_req(write_req: *uv_write_t) -> *uv_stream_t {
    return rust_uv_get_stream_handle_from_write_req(write_req);
}
pub unsafe fn get_data_for_uv_loop(loop_ptr: *c_void) -> *c_void {
    rust_uv_get_data_for_uv_loop(loop_ptr)
}
pub unsafe fn set_data_for_uv_loop(loop_ptr: *c_void, data: *c_void) {
    rust_uv_set_data_for_uv_loop(loop_ptr, data);
}
pub unsafe fn get_data_for_uv_handle<T>(handle: *T) -> *c_void {
    return rust_uv_get_data_for_uv_handle(handle as *c_void);
}
pub unsafe fn set_data_for_uv_handle<T, U>(handle: *T, data: *U) {
    rust_uv_set_data_for_uv_handle(handle as *c_void, data as *c_void);
}
pub unsafe fn get_data_for_req<T>(req: *T) -> *c_void {
    return rust_uv_get_data_for_req(req as *c_void);
}
pub unsafe fn set_data_for_req<T, U>(req: *T, data: *U) {
    rust_uv_set_data_for_req(req as *c_void, data as *c_void);
}
pub unsafe fn populate_stat(req_in: *uv_fs_t, stat_out: *uv_stat_t) {
    rust_uv_populate_uv_stat(req_in, stat_out)
}
pub unsafe fn guess_handle(handle: c_int) -> c_int {
    rust_uv_guess_handle(handle)
}


// uv_support is the result of compiling rust_uv.cpp
//
// Note that this is in a cfg'd block so it doesn't get linked during testing.
// There's a bit of a conundrum when testing in that we're actually assuming
// that the tests are running in a uv loop, but they were created from the
// statically linked uv to the original rustuv crate. When we create the test
// executable, on some platforms if we re-link against uv, it actually creates
// second copies of everything. We obviously don't want this, so instead of
// dying horribly during testing, we allow all of the test rustuv's references
// to get resolved to the original rustuv crate.
#[cfg(not(test))]
#[link(name = "uv_support", kind = "static")]
#[link(name = "uv", kind = "static")]
extern {}

extern {
    fn rust_uv_loop_new() -> *c_void;

    #[cfg(test)]
    fn rust_uv_handle_type_max() -> uintptr_t;
    #[cfg(test)]
    fn rust_uv_req_type_max() -> uintptr_t;
    fn rust_uv_get_udp_handle_from_send_req(req: *uv_udp_send_t) -> *uv_udp_t;

    fn rust_uv_populate_uv_stat(req_in: *uv_fs_t, stat_out: *uv_stat_t);
    fn rust_uv_get_result_from_fs_req(req: *uv_fs_t) -> ssize_t;
    fn rust_uv_get_ptr_from_fs_req(req: *uv_fs_t) -> *libc::c_void;
    fn rust_uv_get_path_from_fs_req(req: *uv_fs_t) -> *c_char;
    fn rust_uv_get_loop_from_fs_req(req: *uv_fs_t) -> *uv_loop_t;
    fn rust_uv_get_loop_from_getaddrinfo_req(req: *uv_fs_t) -> *uv_loop_t;
    fn rust_uv_get_stream_handle_from_connect_req(req: *uv_connect_t) -> *uv_stream_t;
    fn rust_uv_get_stream_handle_from_write_req(req: *uv_write_t) -> *uv_stream_t;
    fn rust_uv_get_loop_for_uv_handle(handle: *c_void) -> *c_void;
    fn rust_uv_get_data_for_uv_loop(loop_ptr: *c_void) -> *c_void;
    fn rust_uv_set_data_for_uv_loop(loop_ptr: *c_void, data: *c_void);
    fn rust_uv_get_data_for_uv_handle(handle: *c_void) -> *c_void;
    fn rust_uv_set_data_for_uv_handle(handle: *c_void, data: *c_void);
    fn rust_uv_get_data_for_req(req: *c_void) -> *c_void;
    fn rust_uv_set_data_for_req(req: *c_void, data: *c_void);
    fn rust_set_stdio_container_flags(c: *uv_stdio_container_t, flags: c_int);
    fn rust_set_stdio_container_fd(c: *uv_stdio_container_t, fd: c_int);
    fn rust_set_stdio_container_stream(c: *uv_stdio_container_t,
                                       stream: *uv_stream_t);
    fn rust_uv_process_pid(p: *uv_process_t) -> c_int;
    fn rust_uv_guess_handle(fd: c_int) -> c_int;

    // generic uv functions
    pub fn uv_loop_delete(l: *uv_loop_t);
    pub fn uv_ref(t: *uv_handle_t);
    pub fn uv_unref(t: *uv_handle_t);
    pub fn uv_handle_size(ty: uv_handle_type) -> size_t;
    pub fn uv_req_size(ty: uv_req_type) -> size_t;
    pub fn uv_run(l: *uv_loop_t, mode: uv_run_mode) -> c_int;
    pub fn uv_close(h: *uv_handle_t, cb: uv_close_cb);
    pub fn uv_walk(l: *uv_loop_t, cb: uv_walk_cb, arg: *c_void);
    pub fn uv_buf_init(base: *c_char, len: c_uint) -> uv_buf_t;
    pub fn uv_strerror(err: c_int) -> *c_char;
    pub fn uv_err_name(err: c_int) -> *c_char;
    pub fn uv_listen(s: *uv_stream_t, backlog: c_int,
                     cb: uv_connection_cb) -> c_int;
    pub fn uv_accept(server: *uv_stream_t, client: *uv_stream_t) -> c_int;
    pub fn uv_read_start(stream: *uv_stream_t,
                         on_alloc: uv_alloc_cb,
                         on_read: uv_read_cb) -> c_int;
    pub fn uv_read_stop(stream: *uv_stream_t) -> c_int;
    pub fn uv_shutdown(req: *uv_shutdown_t, handle: *uv_stream_t,
                       cb: uv_shutdown_cb) -> c_int;

    // idle bindings
    pub fn uv_idle_init(l: *uv_loop_t, i: *uv_idle_t) -> c_int;
    pub fn uv_idle_start(i: *uv_idle_t, cb: uv_idle_cb) -> c_int;
    pub fn uv_idle_stop(i: *uv_idle_t) -> c_int;

    // async bindings
    pub fn uv_async_init(l: *uv_loop_t, a: *uv_async_t,
                         cb: uv_async_cb) -> c_int;
    pub fn uv_async_send(a: *uv_async_t);

    // tcp bindings
    pub fn uv_tcp_init(l: *uv_loop_t, h: *uv_tcp_t) -> c_int;
    pub fn uv_tcp_connect(c: *uv_connect_t, h: *uv_tcp_t,
                          addr: *sockaddr, cb: uv_connect_cb) -> c_int;
    pub fn uv_tcp_bind(t: *uv_tcp_t, addr: *sockaddr) -> c_int;
    pub fn uv_tcp_nodelay(h: *uv_tcp_t, enable: c_int) -> c_int;
    pub fn uv_tcp_keepalive(h: *uv_tcp_t, enable: c_int,
                            delay: c_uint) -> c_int;
    pub fn uv_tcp_simultaneous_accepts(h: *uv_tcp_t, enable: c_int) -> c_int;
    pub fn uv_tcp_getsockname(h: *uv_tcp_t, name: *mut sockaddr,
                              len: *mut c_int) -> c_int;
    pub fn uv_tcp_getpeername(h: *uv_tcp_t, name: *mut sockaddr,
                              len: *mut c_int) -> c_int;

    // udp bindings
    pub fn uv_udp_init(l: *uv_loop_t, h: *uv_udp_t) -> c_int;
    pub fn uv_udp_bind(h: *uv_udp_t, addr: *sockaddr, flags: c_uint) -> c_int;
    pub fn uv_udp_recv_start(server: *uv_udp_t,
                             on_alloc: uv_alloc_cb,
                             on_recv: uv_udp_recv_cb) -> c_int;
    pub fn uv_udp_set_membership(handle: *uv_udp_t, multicast_addr: *c_char,
                                 interface_addr: *c_char,
                                 membership: uv_membership) -> c_int;
    pub fn uv_udp_recv_stop(server: *uv_udp_t) -> c_int;
    pub fn uv_udp_set_multicast_loop(handle: *uv_udp_t, on: c_int) -> c_int;
    pub fn uv_udp_set_multicast_ttl(handle: *uv_udp_t, ttl: c_int) -> c_int;
    pub fn uv_udp_set_ttl(handle: *uv_udp_t, ttl: c_int) -> c_int;
    pub fn uv_udp_set_broadcast(handle: *uv_udp_t, on: c_int) -> c_int;
    pub fn uv_udp_getsockname(h: *uv_udp_t, name: *mut sockaddr,
                              len: *mut c_int) -> c_int;

    // timer bindings
    pub fn uv_timer_init(l: *uv_loop_t, t: *uv_timer_t) -> c_int;
    pub fn uv_timer_start(t: *uv_timer_t, cb: uv_timer_cb,
                          timeout: libc::uint64_t,
                          repeat: libc::uint64_t) -> c_int;
    pub fn uv_timer_stop(handle: *uv_timer_t) -> c_int;

    // fs operations
    pub fn uv_fs_open(loop_ptr: *uv_loop_t, req: *uv_fs_t, path: *c_char,
                      flags: c_int, mode: c_int, cb: uv_fs_cb) -> c_int;
    pub fn uv_fs_unlink(loop_ptr: *uv_loop_t, req: *uv_fs_t, path: *c_char,
                        cb: uv_fs_cb) -> c_int;
    pub fn uv_fs_write(l: *uv_loop_t, req: *uv_fs_t, fd: c_int,
                       bufs: *uv_buf_t, nbufs: c_uint,
                       offset: i64, cb: uv_fs_cb) -> c_int;
    pub fn uv_fs_read(l: *uv_loop_t, req: *uv_fs_t, fd: c_int,
                      bufs: *uv_buf_t, nbufs: c_uint,
                      offset: i64, cb: uv_fs_cb) -> c_int;
    pub fn uv_fs_close(l: *uv_loop_t, req: *uv_fs_t, fd: c_int,
                       cb: uv_fs_cb) -> c_int;
    pub fn uv_fs_stat(l: *uv_loop_t, req: *uv_fs_t, path: *c_char,
                      cb: uv_fs_cb) -> c_int;
    pub fn uv_fs_fstat(l: *uv_loop_t, req: *uv_fs_t, fd: c_int,
                       cb: uv_fs_cb) -> c_int;
    pub fn uv_fs_mkdir(l: *uv_loop_t, req: *uv_fs_t, path: *c_char,
                       mode: c_int, cb: uv_fs_cb) -> c_int;
    pub fn uv_fs_rmdir(l: *uv_loop_t, req: *uv_fs_t, path: *c_char,
                       cb: uv_fs_cb) -> c_int;
    pub fn uv_fs_readdir(l: *uv_loop_t, req: *uv_fs_t, path: *c_char,
                         flags: c_int, cb: uv_fs_cb) -> c_int;
    pub fn uv_fs_req_cleanup(req: *uv_fs_t);
    pub fn uv_fs_fsync(handle: *uv_loop_t, req: *uv_fs_t, file: c_int,
                       cb: uv_fs_cb) -> c_int;
    pub fn uv_fs_fdatasync(handle: *uv_loop_t, req: *uv_fs_t, file: c_int,
                           cb: uv_fs_cb) -> c_int;
    pub fn uv_fs_ftruncate(handle: *uv_loop_t, req: *uv_fs_t, file: c_int,
                           offset: i64, cb: uv_fs_cb) -> c_int;
    pub fn uv_fs_readlink(handle: *uv_loop_t, req: *uv_fs_t, file: *c_char,
                          cb: uv_fs_cb) -> c_int;
    pub fn uv_fs_symlink(handle: *uv_loop_t, req: *uv_fs_t, src: *c_char,
                         dst: *c_char, flags: c_int, cb: uv_fs_cb) -> c_int;
    pub fn uv_fs_rename(handle: *uv_loop_t, req: *uv_fs_t, src: *c_char,
                        dst: *c_char, cb: uv_fs_cb) -> c_int;
    pub fn uv_fs_utime(handle: *uv_loop_t, req: *uv_fs_t, path: *c_char,
                       atime: c_double, mtime: c_double,
                       cb: uv_fs_cb) -> c_int;
    pub fn uv_fs_link(handle: *uv_loop_t, req: *uv_fs_t, src: *c_char,
                      dst: *c_char, cb: uv_fs_cb) -> c_int;
    pub fn uv_fs_chown(handle: *uv_loop_t, req: *uv_fs_t, src: *c_char,
                       uid: uv_uid_t, gid: uv_gid_t, cb: uv_fs_cb) -> c_int;
    pub fn uv_fs_chmod(handle: *uv_loop_t, req: *uv_fs_t, path: *c_char,
                       mode: c_int, cb: uv_fs_cb) -> c_int;
    pub fn uv_fs_lstat(handle: *uv_loop_t, req: *uv_fs_t, file: *c_char,
                       cb: uv_fs_cb) -> c_int;

    // getaddrinfo
    pub fn uv_getaddrinfo(loop_: *uv_loop_t, req: *uv_getaddrinfo_t,
                          getaddrinfo_cb: uv_getaddrinfo_cb,
                          node: *c_char, service: *c_char,
                          hints: *addrinfo) -> c_int;
    pub fn uv_freeaddrinfo(ai: *addrinfo);

    // process spawning
    pub fn uv_spawn(loop_ptr: *uv_loop_t, outptr: *uv_process_t,
                    options: *uv_process_options_t) -> c_int;
    pub fn uv_process_kill(p: *uv_process_t, signum: c_int) -> c_int;
    pub fn uv_kill(pid: c_int, signum: c_int) -> c_int;

    // pipes
    pub fn uv_pipe_init(l: *uv_loop_t, p: *uv_pipe_t, ipc: c_int) -> c_int;
    pub fn uv_pipe_open(pipe: *uv_pipe_t, file: c_int) -> c_int;
    pub fn uv_pipe_bind(pipe: *uv_pipe_t, name: *c_char) -> c_int;
    pub fn uv_pipe_connect(req: *uv_connect_t, handle: *uv_pipe_t,
                           name: *c_char, cb: uv_connect_cb);

    // tty
    pub fn uv_tty_init(l: *uv_loop_t, tty: *uv_tty_t, fd: c_int,
                       readable: c_int) -> c_int;
    pub fn uv_tty_set_mode(tty: *uv_tty_t, mode: c_int) -> c_int;
    pub fn uv_tty_get_winsize(tty: *uv_tty_t, width: *c_int,
                              height: *c_int) -> c_int;

    // signals
    pub fn uv_signal_init(loop_: *uv_loop_t, handle: *uv_signal_t) -> c_int;
    pub fn uv_signal_start(h: *uv_signal_t, cb: uv_signal_cb,
                           signum: c_int) -> c_int;
    pub fn uv_signal_stop(handle: *uv_signal_t) -> c_int;
}

// libuv requires other native libraries on various platforms. These are all
// listed here (for each platform)

// libuv doesn't use pthread on windows
// android libc (bionic) provides pthread, so no additional link is required
#[cfg(not(windows), not(target_os = "android"))]
#[link(name = "pthread")]
extern {}

#[cfg(target_os = "linux")]
#[link(name = "rt")]
extern {}

#[cfg(target_os = "win32")]
#[link(name = "ws2_32")]
#[link(name = "psapi")]
#[link(name = "iphlpapi")]
extern {}

#[cfg(target_os = "freebsd")]
#[link(name = "kvm")]
extern {}
