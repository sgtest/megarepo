#[doc = "
Low-level bindings to the libuv library.

This module contains a set of direct, 'bare-metal' wrappers around
the libuv C-API.

Also contained herein are a set of rust records that map, in
approximate memory-size, to the libuv data structures. The record
implementations are adjusted, per-platform, to match their respective
representations.

There are also a collection of helper functions to ease interacting
with the low-level API (such as a function to return the latest
libuv error as a rust-formatted string).

As new functionality, existant in uv.h, is added to the rust stdlib,
the mappings should be added in this module.

This module's implementation will hopefully be, eventually, replaced
with per-platform, generated source files from rust-bindgen.
"];

// libuv struct mappings
type uv_ip4_addr = {
    ip: [u8],
    port: int
};
type uv_ip6_addr = uv_ip4_addr;

enum uv_handle_type {
    UNKNOWN_HANDLE = 0,
    UV_TCP,
    UV_UDP,
    UV_NAMED_PIPE,
    UV_TTY,
    UV_FILE,
    UV_TIMER,
    UV_PREPARE,
    UV_CHECK,
    UV_IDLE,
    UV_ASYNC,
    UV_ARES_TASK,
    UV_ARES_EVENT,
    UV_PROCESS,
    UV_FS_EVENT
}

type handle_type = libc::c_uint;

type uv_handle_fields = {
   loop_handle: *libc::c_void,
   type_: handle_type,
   close_cb: *u8,
   mut data: *libc::c_void,
};

// unix size: 8
type uv_err_t = {
    code: libc::c_int,
    sys_errno_: libc::c_int
};

// don't create one of these directly. instead,
// count on it appearing in libuv callbacks or embedded
// in other types as a pointer to be used in other
// operations (so mostly treat it as opaque, once you
// have it in this form..)
#[cfg(target_os = "linux")]
#[cfg(target_os = "macos")]
#[cfg(target_os = "freebsd")]
#[cfg(target_os = "win32")]
type uv_stream_t = {
    fields: uv_handle_fields
};

// 64bit unix size: 272
#[cfg(target_os = "linux")]
#[cfg(target_os = "macos")]
#[cfg(target_os = "freebsd")]
type uv_tcp_t = {
    fields: uv_handle_fields,
    a00: *u8, a01: *u8, a02: *u8, a03: *u8,
    a04: *u8, a05: *u8, a06: *u8, a07: *u8,
    a08: *u8, a09: *u8, a10: *u8, a11: *u8,
    a12: *u8, a13: *u8, a14: *u8, a15: *u8,
    a16: *u8, a17: *u8, a18: *u8, a19: *u8,
    a20: *u8, a21: *u8, a22: *u8, a23: *u8,
    a24: *u8, a25: *u8, a26: *u8, a27: *u8,
    a28: *u8,
    a30: uv_tcp_t_32bit_unix_riders
};
// 32bit unix size: 328 (164)
#[cfg(target_arch="x86_64")]
type uv_tcp_t_32bit_unix_riders = {
    a29: *u8
};
#[cfg(target_arch="x86")]
type uv_tcp_t_32bit_unix_riders = {
    a29: *u8, a30: *u8, a31: *u8,
    a32: *u8, a33: *u8, a34: *u8,
    a35: *u8, a36: *u8
};

// 32bit win32 size: 240 (120)
#[cfg(target_os = "win32")]
type uv_tcp_t = {
    fields: uv_handle_fields,
    a00: *u8, a01: *u8, a02: *u8, a03: *u8,
    a04: *u8, a05: *u8, a06: *u8, a07: *u8,
    a08: *u8, a09: *u8, a10: *u8, a11: *u8,
    a12: *u8, a13: *u8, a14: *u8, a15: *u8,
    a16: *u8, a17: *u8, a18: *u8, a19: *u8,
    a20: *u8, a21: *u8, a22: *u8, a23: *u8,
    a24: *u8, a25: *u8
};

// unix size: 48
#[cfg(target_os = "linux")]
#[cfg(target_os = "macos")]
#[cfg(target_os = "freebsd")]
type uv_connect_t = {
    a00: *u8, a01: *u8, a02: *u8, a03: *u8,
    a04: *u8, a05: *u8
};
// win32 size: 88 (44)
#[cfg(target_os = "win32")]
type uv_connect_t = {
    a00: *u8, a01: *u8, a02: *u8, a03: *u8,
    a04: *u8, a05: *u8, a06: *u8, a07: *u8,
    a08: *u8, a09: *u8, a10: *u8
};

// unix size: 16
#[cfg(target_os = "linux")]
#[cfg(target_os = "macos")]
#[cfg(target_os = "freebsd")]
#[cfg(target_os = "win32")]
type uv_buf_t = {
    base: *u8,
    len: libc::size_t
};
// no gen stub method.. should create
// it via uv::direct::buf_init()

// unix size: 144
#[cfg(target_os = "linux")]
#[cfg(target_os = "macos")]
#[cfg(target_os = "freebsd")]
type uv_write_t = {
    fields: uv_handle_fields,
    a00: *u8, a01: *u8, a02: *u8, a03: *u8,
    a04: *u8, a05: *u8, a06: *u8, a07: *u8,
    a08: *u8, a09: *u8, a10: *u8, a11: *u8,
    a12: *u8,
    a14: uv_write_t_32bit_unix_riders
};
#[cfg(target_arch="x86_64")]
type uv_write_t_32bit_unix_riders = {
    a13: *u8
};
#[cfg(target_arch="x86")]
type uv_write_t_32bit_unix_riders = {
    a13: *u8, a14: *u8
};
// win32 size: 136 (68)
#[cfg(target_os = "win32")]
type uv_write_t = {
    fields: uv_handle_fields,
    a00: *u8, a01: *u8, a02: *u8, a03: *u8,
    a04: *u8, a05: *u8, a06: *u8, a07: *u8,
    a08: *u8, a09: *u8, a10: *u8, a11: *u8,
    a12: *u8
};
// 64bit unix size: 120
// 32bit unix size: 152 (76)
#[cfg(target_os = "linux")]
#[cfg(target_os = "macos")]
#[cfg(target_os = "freebsd")]
type uv_async_t = {
    fields: uv_handle_fields,
    a00: *u8, a01: *u8, a02: *u8, a03: *u8,
    a04: *u8, a05: *u8, a06: *u8, a07: *u8,
    a08: *u8, a09: *u8,
    a11: uv_async_t_32bit_unix_riders
};
#[cfg(target_arch="x86_64")]
type uv_async_t_32bit_unix_riders = {
    a10: *u8
};
#[cfg(target_arch="x86")]
type uv_async_t_32bit_unix_riders = {
    a10: *u8, a11: *u8, a12: *u8, a13: *u8
};
// win32 size 132 (68)
#[cfg(target_os = "win32")]
type uv_async_t = {
    fields: uv_handle_fields,
    a00: *u8, a01: *u8, a02: *u8, a03: *u8,
    a04: *u8, a05: *u8, a06: *u8, a07: *u8,
    a08: *u8, a09: *u8, a10: *u8, a11: *u8,
    a12: *u8
};

// 64bit unix size: 128
// 32bit unix size: 84
#[cfg(target_os = "linux")]
#[cfg(target_os = "macos")]
#[cfg(target_os = "freebsd")]
type uv_timer_t = {
    fields: uv_handle_fields,
    a00: *u8, a01: *u8, a02: *u8, a03: *u8,
    a04: *u8, a05: *u8, a06: *u8, a07: *u8,
    a08: *u8, a09: *u8,
    a11: uv_timer_t_32bit_unix_riders
};
#[cfg(target_arch="x86_64")]
type uv_timer_t_32bit_unix_riders = {
    a10: *u8, a11: *u8
};
#[cfg(target_arch="x86")]
type uv_timer_t_32bit_unix_riders = {
    a10: *u8, a11: *u8, a12: *u8, a13: *u8,
    a14: *u8, a15: *u8, a16: *u8
};
// win32 size: 64
#[cfg(target_os = "win32")]
type uv_timer_t = {
    fields: uv_handle_fields,
    a00: *u8, a01: *u8, a02: *u8, a03: *u8,
    a04: *u8, a05: *u8, a06: *u8, a07: *u8,
    a08: *u8, a09: *u8, a10: *u8, a11: *u8
};

// unix size: 16
#[cfg(target_os = "linux")]
#[cfg(target_os = "macos")]
#[cfg(target_os = "freebsd")]
#[cfg(target_os = "win32")]
type sockaddr_in = {
    mut sin_family: u16,
    mut sin_port: u16,
    mut sin_addr: u32, // in_addr: this is an opaque, per-platform struct
    mut sin_zero: (u8, u8, u8, u8, u8, u8, u8, u8)
};

// unix size: 28 .. make due w/ 32
#[cfg(target_os = "linux")]
#[cfg(target_os = "macos")]
#[cfg(target_os = "freebsd")]
#[cfg(target_os = "win32")]
type sockaddr_in6 = {
    a0: *u8, a1: *u8,
    a2: *u8, a3: (u8, u8, u8, u8)
};

mod uv_ll_struct_stubgen {
    fn gen_stub_uv_tcp_t() -> uv_tcp_t {
        ret gen_stub_os();
        #[cfg(target_os = "linux")]
        #[cfg(target_os = "macos")]
        #[cfg(target_os = "freebsd")]
        fn gen_stub_os() -> uv_tcp_t {
            ret gen_stub_arch();
            #[cfg(target_arch="x86_64")]
            fn gen_stub_arch() -> uv_tcp_t {
                ret { fields: { loop_handle: ptr::null(), type_: 0u32,
                                close_cb: ptr::null(),
                                mut data: ptr::null() },
                    a00: 0 as *u8, a01: 0 as *u8, a02: 0 as *u8,
                    a03: 0 as *u8,
                    a04: 0 as *u8, a05: 0 as *u8, a06: 0 as *u8,
                    a07: 0 as *u8,
                    a08: 0 as *u8, a09: 0 as *u8, a10: 0 as *u8,
                    a11: 0 as *u8,
                    a12: 0 as *u8, a13: 0 as *u8, a14: 0 as *u8,
                    a15: 0 as *u8,
                    a16: 0 as *u8, a17: 0 as *u8, a18: 0 as *u8,
                    a19: 0 as *u8,
                    a20: 0 as *u8, a21: 0 as *u8, a22: 0 as *u8,
                    a23: 0 as *u8,
                    a24: 0 as *u8, a25: 0 as *u8, a26: 0 as *u8,
                    a27: 0 as *u8,
                    a28: 0 as *u8,
                    a30: {
                        a29: 0 as *u8
                    }
                };
            }
            #[cfg(target_arch="x86")]
            fn gen_stub_arch() -> uv_tcp_t {
                ret { fields: { loop_handle: ptr::null(), type_: 0u32,
                                close_cb: ptr::null(),
                                mut data: ptr::null() },
                    a00: 0 as *u8, a01: 0 as *u8, a02: 0 as *u8,
                    a03: 0 as *u8,
                    a04: 0 as *u8, a05: 0 as *u8, a06: 0 as *u8,
                    a07: 0 as *u8,
                    a08: 0 as *u8, a09: 0 as *u8, a10: 0 as *u8,
                    a11: 0 as *u8,
                    a12: 0 as *u8, a13: 0 as *u8, a14: 0 as *u8,
                    a15: 0 as *u8,
                    a16: 0 as *u8, a17: 0 as *u8, a18: 0 as *u8,
                    a19: 0 as *u8,
                    a20: 0 as *u8, a21: 0 as *u8, a22: 0 as *u8,
                    a23: 0 as *u8,
                    a24: 0 as *u8, a25: 0 as *u8, a26: 0 as *u8,
                    a27: 0 as *u8,
                    a28: 0 as *u8,
                    a30: {
                        a29: 0 as *u8, a30: 0 as *u8, a31: 0 as *u8,
                        a32: 0 as *u8, a33: 0 as *u8, a34: 0 as *u8,
                        a35: 0 as *u8, a36: 0 as *u8
                    }
                };
            }
        }
        #[cfg(target_os = "win32")]
        fn gen_stub_os() -> uv_tcp_t {
            ret { fields: { loop_handle: ptr::null(), type_: 0u32,
                            close_cb: ptr::null(),
                            mut data: ptr::null() },
                a00: 0 as *u8, a01: 0 as *u8, a02: 0 as *u8,
                a03: 0 as *u8,
                a04: 0 as *u8, a05: 0 as *u8, a06: 0 as *u8,
                a07: 0 as *u8,
                a08: 0 as *u8, a09: 0 as *u8, a10: 0 as *u8,
                a11: 0 as *u8,
                a12: 0 as *u8, a13: 0 as *u8, a14: 0 as *u8,
                a15: 0 as *u8,
                a16: 0 as *u8, a17: 0 as *u8, a18: 0 as *u8,
                a19: 0 as *u8,
                a20: 0 as *u8, a21: 0 as *u8, a22: 0 as *u8,
                a23: 0 as *u8,
                a24: 0 as *u8, a25: 0 as *u8
            };
        }
    }
    #[cfg(target_os = "linux")]
    #[cfg(target_os = "macos")]
    #[cfg(target_os = "freebsd")]
    fn gen_stub_uv_connect_t() -> uv_connect_t {
        ret {
            a00: 0 as *u8, a01: 0 as *u8, a02: 0 as *u8,
            a03: 0 as *u8,
            a04: 0 as *u8, a05: 0 as *u8
        };
    }
    #[cfg(target_os = "win32")]
    fn gen_stub_uv_connect_t() -> uv_connect_t {
        ret {
            a00: 0 as *u8, a01: 0 as *u8, a02: 0 as *u8,
            a03: 0 as *u8,
            a04: 0 as *u8, a05: 0 as *u8, a06: 0 as *u8,
            a07: 0 as *u8,
            a08: 0 as *u8, a09: 0 as *u8, a10: 0 as *u8
        };
    }
    #[cfg(target_os = "linux")]
    #[cfg(target_os = "macos")]
    #[cfg(target_os = "freebsd")]
    fn gen_stub_uv_async_t() -> uv_async_t {
        ret gen_stub_arch();
        #[cfg(target_arch = "x86_64")]
        fn gen_stub_arch() -> uv_async_t {
            ret { fields: { loop_handle: ptr::null(), type_: 0u32,
                            close_cb: ptr::null(),
                            mut data: ptr::null() },
                a00: 0 as *u8, a01: 0 as *u8, a02: 0 as *u8,
                a03: 0 as *u8,
                a04: 0 as *u8, a05: 0 as *u8, a06: 0 as *u8,
                a07: 0 as *u8,
                a08: 0 as *u8, a09: 0 as *u8,
                a11: {
                    a10: 0 as *u8
                }
            };
        }
        #[cfg(target_arch = "x86")]
        fn gen_stub_arch() -> uv_async_t {
            ret { fields: { loop_handle: ptr::null(), type_: 0u32,
                            close_cb: ptr::null(),
                            mut data: ptr::null() },
                a00: 0 as *u8, a01: 0 as *u8, a02: 0 as *u8,
                a03: 0 as *u8,
                a04: 0 as *u8, a05: 0 as *u8, a06: 0 as *u8,
                a07: 0 as *u8,
                a08: 0 as *u8, a09: 0 as *u8,
                a11: {
                    a10: 0 as *u8, a11: 0 as *u8,
                    a12: 0 as *u8, a13: 0 as *u8
                }
            };
        }
    }
    #[cfg(target_os = "win32")]
    fn gen_stub_uv_async_t() -> uv_async_t {
        ret { fields: { loop_handle: ptr::null(), type_: 0u32,
                        close_cb: ptr::null(),
                        mut data: ptr::null() },
            a00: 0 as *u8, a01: 0 as *u8, a02: 0 as *u8,
            a03: 0 as *u8,
            a04: 0 as *u8, a05: 0 as *u8, a06: 0 as *u8,
            a07: 0 as *u8,
            a08: 0 as *u8, a09: 0 as *u8, a10: 0 as *u8,
            a11: 0 as *u8,
            a12: 0 as *u8
        };
    }
    #[cfg(target_os = "linux")]
    #[cfg(target_os = "macos")]
    #[cfg(target_os = "freebsd")]
    fn gen_stub_uv_timer_t() -> uv_timer_t {
        ret gen_stub_arch();
        #[cfg(target_arch = "x86_64")]
        fn gen_stub_arch() -> uv_timer_t {
            ret { fields: { loop_handle: ptr::null(), type_: 0u32,
                            close_cb: ptr::null(),
                            mut data: ptr::null() },
                a00: 0 as *u8, a01: 0 as *u8, a02: 0 as *u8,
                a03: 0 as *u8,
                a04: 0 as *u8, a05: 0 as *u8, a06: 0 as *u8,
                a07: 0 as *u8,
                a08: 0 as *u8, a09: 0 as *u8,
                a11: {
                    a10: 0 as *u8, a11: 0 as *u8
                }
            };
        }
        #[cfg(target_arch = "x86")]
        fn gen_stub_arch() -> uv_timer_t {
            ret { fields: { loop_handle: ptr::null(), type_: 0u32,
                            close_cb: ptr::null(),
                            mut data: ptr::null() },
                a00: 0 as *u8, a01: 0 as *u8, a02: 0 as *u8,
                a03: 0 as *u8,
                a04: 0 as *u8, a05: 0 as *u8, a06: 0 as *u8,
                a07: 0 as *u8,
                a08: 0 as *u8, a09: 0 as *u8,
                a11: {
                    a10: 0 as *u8, a11: 0 as *u8,
                    a12: 0 as *u8, a13: 0 as *u8,
                    a14: 0 as *u8, a15: 0 as *u8,
                    a16: 0 as *u8
                }
            };
        }
    }
    #[cfg(target_os = "win32")]
    fn gen_stub_uv_timer_t() -> uv_timer_t {
        ret { fields: { loop_handle: ptr::null(), type_: 0u32,
                        close_cb: ptr::null(),
                        mut data: ptr::null() },
            a00: 0 as *u8, a01: 0 as *u8, a02: 0 as *u8,
            a03: 0 as *u8,
            a04: 0 as *u8, a05: 0 as *u8, a06: 0 as *u8,
            a07: 0 as *u8,
            a08: 0 as *u8, a09: 0 as *u8, a10: 0 as *u8,
            a11: 0 as *u8
        };
    }
    #[cfg(target_os = "linux")]
    #[cfg(target_os = "macos")]
    #[cfg(target_os = "freebsd")]
    fn gen_stub_uv_write_t() -> uv_write_t {
        ret gen_stub_arch();
        #[cfg(target_arch="x86_64")]
        fn gen_stub_arch() -> uv_write_t {
            ret { fields: { loop_handle: ptr::null(), type_: 0u32,
                            close_cb: ptr::null(),
                            mut data: ptr::null() },
                a00: 0 as *u8, a01: 0 as *u8, a02: 0 as *u8,
                a03: 0 as *u8,
                a04: 0 as *u8, a05: 0 as *u8, a06: 0 as *u8,
                a07: 0 as *u8,
                a08: 0 as *u8, a09: 0 as *u8, a10: 0 as *u8,
                a11: 0 as *u8,
                a12: 0 as *u8, a14: { a13: 0 as *u8 }
            };
        }
        #[cfg(target_arch="x86")]
        fn gen_stub_arch() -> uv_write_t {
            ret { fields: { loop_handle: ptr::null(), type_: 0u32,
                            close_cb: ptr::null(),
                            mut data: ptr::null() },
                a00: 0 as *u8, a01: 0 as *u8, a02: 0 as *u8,
                a03: 0 as *u8,
                a04: 0 as *u8, a05: 0 as *u8, a06: 0 as *u8,
                a07: 0 as *u8,
                a08: 0 as *u8, a09: 0 as *u8, a10: 0 as *u8,
                a11: 0 as *u8,
                a12: 0 as *u8, a14: { a13: 0 as *u8, a14: 0 as *u8 }
            };
        }
    }
    #[cfg(target_os = "win32")]
    fn gen_stub_uv_write_t() -> uv_write_t {
        ret { fields: { loop_handle: ptr::null(), type_: 0u32,
                        close_cb: ptr::null(),
                        mut data: ptr::null() },
            a00: 0 as *u8, a01: 0 as *u8, a02: 0 as *u8,
            a03: 0 as *u8,
            a04: 0 as *u8, a05: 0 as *u8, a06: 0 as *u8,
            a07: 0 as *u8,
            a08: 0 as *u8, a09: 0 as *u8, a10: 0 as *u8,
            a11: 0 as *u8,
            a12: 0 as *u8
        };
    }
}

#[nolink]
native mod rustrt {
    fn rust_uv_loop_new() -> *libc::c_void;
    fn rust_uv_loop_delete(lp: *libc::c_void);
    fn rust_uv_loop_refcount(loop_ptr: *libc::c_void) -> libc::c_int;
    fn rust_uv_run(loop_handle: *libc::c_void);
    fn rust_uv_close(handle: *libc::c_void, cb: *u8);
    fn rust_uv_async_send(handle: *uv_async_t);
    fn rust_uv_async_init(loop_handle: *libc::c_void,
                          async_handle: *uv_async_t,
                          cb: *u8) -> libc::c_int;
    fn rust_uv_tcp_init(
        loop_handle: *libc::c_void,
        handle_ptr: *uv_tcp_t) -> libc::c_int;
    // FIXME ref #2604 .. ?
    fn rust_uv_buf_init(out_buf: *uv_buf_t, base: *u8,
                        len: libc::size_t);
    fn rust_uv_last_error(loop_handle: *libc::c_void) -> uv_err_t;
    // FIXME ref #2064
    fn rust_uv_strerror(err: *uv_err_t) -> *libc::c_char;
    // FIXME ref #2064
    fn rust_uv_err_name(err: *uv_err_t) -> *libc::c_char;
    fn rust_uv_ip4_addr(ip: *u8, port: libc::c_int)
        -> sockaddr_in;
    // FIXME ref #2064
    fn rust_uv_tcp_connect(connect_ptr: *uv_connect_t,
                           tcp_handle_ptr: *uv_tcp_t,
                           ++after_cb: *u8,
                           ++addr: *sockaddr_in) -> libc::c_int;
    // FIXME ref 2064
    fn rust_uv_tcp_bind(tcp_server: *uv_tcp_t,
                        ++addr: *sockaddr_in) -> libc::c_int;
    fn rust_uv_listen(stream: *libc::c_void, backlog: libc::c_int,
                      cb: *u8) -> libc::c_int;
    fn rust_uv_accept(server: *libc::c_void, client: *libc::c_void)
        -> libc::c_int;
    fn rust_uv_write(req: *libc::c_void, stream: *libc::c_void,
             ++buf_in: *uv_buf_t, buf_cnt: libc::c_int,
             cb: *u8) -> libc::c_int;
    fn rust_uv_read_start(stream: *libc::c_void, on_alloc: *u8,
                          on_read: *u8) -> libc::c_int;
    fn rust_uv_read_stop(stream: *libc::c_void) -> libc::c_int;
    fn rust_uv_timer_init(loop_handle: *libc::c_void,
                          timer_handle: *uv_timer_t) -> libc::c_int;
    fn rust_uv_timer_start(
        timer_handle: *uv_timer_t,
        cb: *u8,
        timeout: libc::c_uint,
        repeat: libc::c_uint) -> libc::c_int;
    fn rust_uv_timer_stop(handle: *uv_timer_t) -> libc::c_int;

    // data accessors/helpers for rust-mapped uv structs
    fn rust_uv_malloc_buf_base_of(sug_size: libc::size_t) -> *u8;
    fn rust_uv_free_base_of_buf(++buf: uv_buf_t);
    fn rust_uv_get_stream_handle_from_connect_req(
        connect_req: *uv_connect_t)
        -> *uv_stream_t;
    fn rust_uv_get_stream_handle_from_write_req(
        write_req: *uv_write_t)
        -> *uv_stream_t;
    fn rust_uv_get_loop_for_uv_handle(handle: *libc::c_void)
        -> *libc::c_void;
    fn rust_uv_get_data_for_uv_loop(loop_ptr: *libc::c_void) -> *libc::c_void;
    fn rust_uv_set_data_for_uv_loop(loop_ptr: *libc::c_void,
                                    data: *libc::c_void);
    fn rust_uv_get_data_for_uv_handle(handle: *libc::c_void)
        -> *libc::c_void;
    fn rust_uv_set_data_for_uv_handle(handle: *libc::c_void,
                                      data: *libc::c_void);
    fn rust_uv_get_data_for_req(req: *libc::c_void) -> *libc::c_void;
    fn rust_uv_set_data_for_req(req: *libc::c_void,
                                data: *libc::c_void);
    fn rust_uv_get_base_from_buf(++buf: uv_buf_t) -> *u8;
    fn rust_uv_get_len_from_buf(++buf: uv_buf_t) -> libc::size_t;

    // sizeof testing helpers
    fn rust_uv_helper_uv_tcp_t_size() -> libc::c_uint;
    fn rust_uv_helper_uv_connect_t_size() -> libc::c_uint;
    fn rust_uv_helper_uv_buf_t_size() -> libc::c_uint;
    fn rust_uv_helper_uv_write_t_size() -> libc::c_uint;
    fn rust_uv_helper_uv_err_t_size() -> libc::c_uint;
    fn rust_uv_helper_sockaddr_in_size() -> libc::c_uint;
    fn rust_uv_helper_uv_async_t_size() -> libc::c_uint;
    fn rust_uv_helper_uv_timer_t_size() -> libc::c_uint;
}

unsafe fn loop_new() -> *libc::c_void {
    ret rustrt::rust_uv_loop_new();
}

unsafe fn loop_delete(loop_handle: *libc::c_void) {
    rustrt::rust_uv_loop_delete(loop_handle);
}

unsafe fn loop_refcount(loop_ptr: *libc::c_void) -> libc::c_int {
    ret rustrt::rust_uv_loop_refcount(loop_ptr);
}

unsafe fn run(loop_handle: *libc::c_void) {
    rustrt::rust_uv_run(loop_handle);
}

unsafe fn close<T>(handle: *T, cb: *u8) {
    rustrt::rust_uv_close(handle as *libc::c_void, cb);
}

unsafe fn tcp_init(loop_handle: *libc::c_void, handle: *uv_tcp_t)
    -> libc::c_int {
    ret rustrt::rust_uv_tcp_init(loop_handle, handle);
}
// FIXME ref #2064
unsafe fn tcp_connect(connect_ptr: *uv_connect_t,
                      tcp_handle_ptr: *uv_tcp_t,
                      addr_ptr: *sockaddr_in,
                      ++after_connect_cb: *u8)
-> libc::c_int {
    let address = *addr_ptr;
    log(debug, #fmt("b4 native tcp_connect--addr port: %u cb: %u",
                     address.sin_port as uint, after_connect_cb as uint));
    ret rustrt::rust_uv_tcp_connect(connect_ptr, tcp_handle_ptr,
                                after_connect_cb, addr_ptr);
}
// FIXME ref #2064
unsafe fn tcp_bind(tcp_server_ptr: *uv_tcp_t,
                   addr_ptr: *sockaddr_in) -> libc::c_int {
    ret rustrt::rust_uv_tcp_bind(tcp_server_ptr,
                                 addr_ptr);
}

unsafe fn listen<T>(stream: *T, backlog: libc::c_int,
                 cb: *u8) -> libc::c_int {
    ret rustrt::rust_uv_listen(stream as *libc::c_void, backlog, cb);
}

unsafe fn accept<T, U>(server: *T, client: *T)
    -> libc::c_int {
    ret rustrt::rust_uv_accept(server as *libc::c_void,
                               client as *libc::c_void);
}

unsafe fn write<T>(req: *uv_write_t, stream: *T,
         buf_in: *[uv_buf_t], cb: *u8) -> libc::c_int {
    let buf_ptr = vec::unsafe::to_ptr(*buf_in);
    let buf_cnt = vec::len(*buf_in) as i32;
    ret rustrt::rust_uv_write(req as *libc::c_void,
                              stream as *libc::c_void,
                              buf_ptr, buf_cnt, cb);
}
unsafe fn read_start(stream: *uv_stream_t, on_alloc: *u8,
                     on_read: *u8) -> libc::c_int {
    ret rustrt::rust_uv_read_start(stream as *libc::c_void,
                                   on_alloc, on_read);
}

unsafe fn read_stop(stream: *uv_stream_t) -> libc::c_int {
    ret rustrt::rust_uv_read_stop(stream as *libc::c_void);
}

unsafe fn last_error(loop_handle: *libc::c_void) -> uv_err_t {
    ret rustrt::rust_uv_last_error(loop_handle);
}

unsafe fn strerror(err: *uv_err_t) -> *libc::c_char {
    ret rustrt::rust_uv_strerror(err);
}
unsafe fn err_name(err: *uv_err_t) -> *libc::c_char {
    ret rustrt::rust_uv_err_name(err);
}

unsafe fn async_init(loop_handle: *libc::c_void,
                     async_handle: *uv_async_t,
                     cb: *u8) -> libc::c_int {
    ret rustrt::rust_uv_async_init(loop_handle,
                                   async_handle,
                                   cb);
}

unsafe fn async_send(async_handle: *uv_async_t) {
    ret rustrt::rust_uv_async_send(async_handle);
}
unsafe fn buf_init(++input: *u8, len: uint) -> uv_buf_t {
    let out_buf = { base: ptr::null(), len: 0 as libc::size_t };
    let out_buf_ptr = ptr::addr_of(out_buf);
    log(debug, #fmt("buf_init - input %u len %u out_buf: %u",
                     input as uint,
                     len as uint,
                     out_buf_ptr as uint));
    // yuck :/
    rustrt::rust_uv_buf_init(out_buf_ptr, input, len);
    //let result = rustrt::rust_uv_buf_init_2(input, len);
    log(debug, "after rust_uv_buf_init");
    let res_base = get_base_from_buf(out_buf);
    let res_len = get_len_from_buf(out_buf);
    //let res_base = get_base_from_buf(result);
    log(debug, #fmt("buf_init - result %u len %u",
                     res_base as uint,
                     res_len as uint));
    ret out_buf;
    //ret result;
}
unsafe fn ip4_addr(ip: str, port: int)
-> sockaddr_in {
    let mut addr_vec = str::bytes(ip);
    addr_vec += [0u8]; // add null terminator
    let addr_vec_ptr = vec::unsafe::to_ptr(addr_vec);
    let ip_back = str::from_bytes(addr_vec);
    log(debug, #fmt("vec val: '%s' length: %u",
                     ip_back, vec::len(addr_vec)));
    ret rustrt::rust_uv_ip4_addr(addr_vec_ptr,
                                 port as libc::c_int);
}

unsafe fn timer_init(loop_ptr: *libc::c_void,
                     timer_ptr: *uv_timer_t) -> libc::c_int {
    ret rustrt::rust_uv_timer_init(loop_ptr, timer_ptr);
}
unsafe fn timer_start(timer_ptr: *uv_timer_t, cb: *u8, timeout: uint,
                      repeat: uint) -> libc::c_int {
    ret rustrt::rust_uv_timer_start(timer_ptr, cb, timeout as libc::c_uint,
                                    repeat as libc::c_uint);
}
unsafe fn timer_stop(timer_ptr: *uv_timer_t) -> libc::c_int {
    ret rustrt::rust_uv_timer_stop(timer_ptr);
}

// libuv struct initializers
unsafe fn tcp_t() -> uv_tcp_t {
    ret uv_ll_struct_stubgen::gen_stub_uv_tcp_t();
}
unsafe fn connect_t() -> uv_connect_t {
    ret uv_ll_struct_stubgen::gen_stub_uv_connect_t();
}
unsafe fn write_t() -> uv_write_t {
    ret uv_ll_struct_stubgen::gen_stub_uv_write_t();
}
unsafe fn async_t() -> uv_async_t {
    ret uv_ll_struct_stubgen::gen_stub_uv_async_t();
}
unsafe fn timer_t() -> uv_timer_t {
    ret uv_ll_struct_stubgen::gen_stub_uv_timer_t();
}

// data access helpers
unsafe fn get_loop_for_uv_handle<T>(handle: *T)
    -> *libc::c_void {
    ret rustrt::rust_uv_get_loop_for_uv_handle(handle as *libc::c_void);
}
unsafe fn get_stream_handle_from_connect_req(connect: *uv_connect_t)
    -> *uv_stream_t {
    ret rustrt::rust_uv_get_stream_handle_from_connect_req(
        connect);
}
unsafe fn get_stream_handle_from_write_req(
    write_req: *uv_write_t)
    -> *uv_stream_t {
    ret rustrt::rust_uv_get_stream_handle_from_write_req(
        write_req);
}
unsafe fn get_data_for_uv_loop(loop_ptr: *libc::c_void) -> *libc::c_void {
    rustrt::rust_uv_get_data_for_uv_loop(loop_ptr)
}
unsafe fn set_data_for_uv_loop(loop_ptr: *libc::c_void, data: *libc::c_void) {
    rustrt::rust_uv_set_data_for_uv_loop(loop_ptr, data);
}
unsafe fn get_data_for_uv_handle<T>(handle: *T) -> *libc::c_void {
    ret rustrt::rust_uv_get_data_for_uv_handle(handle as *libc::c_void);
}
unsafe fn set_data_for_uv_handle<T, U>(handle: *T,
                    data: *U) {
    rustrt::rust_uv_set_data_for_uv_handle(handle as *libc::c_void,
                                           data as *libc::c_void);
}
unsafe fn get_data_for_req<T>(req: *T) -> *libc::c_void {
    ret rustrt::rust_uv_get_data_for_req(req as *libc::c_void);
}
unsafe fn set_data_for_req<T, U>(req: *T,
                    data: *U) {
    rustrt::rust_uv_set_data_for_req(req as *libc::c_void,
                                     data as *libc::c_void);
}
unsafe fn get_base_from_buf(buf: uv_buf_t) -> *u8 {
    ret rustrt::rust_uv_get_base_from_buf(buf);
}
unsafe fn get_len_from_buf(buf: uv_buf_t) -> libc::size_t {
    ret rustrt::rust_uv_get_len_from_buf(buf);
}
unsafe fn malloc_buf_base_of(suggested_size: libc::size_t)
    -> *u8 {
    ret rustrt::rust_uv_malloc_buf_base_of(suggested_size);
}
unsafe fn free_base_of_buf(buf: uv_buf_t) {
    rustrt::rust_uv_free_base_of_buf(buf);
}

unsafe fn get_last_err_info(uv_loop: *libc::c_void) -> str {
    let err = last_error(uv_loop);
    let err_ptr = ptr::addr_of(err);
    let err_name = str::unsafe::from_c_str(err_name(err_ptr));
    let err_msg = str::unsafe::from_c_str(strerror(err_ptr));
    ret #fmt("LIBUV ERROR: name: %s msg: %s",
                    err_name, err_msg);
}

unsafe fn get_last_err_data(uv_loop: *libc::c_void) -> uv_err_data {
    let err = last_error(uv_loop);
    let err_ptr = ptr::addr_of(err);
    let err_name = str::unsafe::from_c_str(err_name(err_ptr));
    let err_msg = str::unsafe::from_c_str(strerror(err_ptr));
    { err_name: err_name, err_msg: err_msg }
}

type uv_err_data = {
    err_name: str,
    err_msg: str
};

#[cfg(test)]
mod test {
    enum tcp_read_data {
        tcp_read_eof,
        tcp_read_more([u8]),
        tcp_read_error
    }

    type request_wrapper = {
        write_req: *uv_write_t,
        req_buf: *[uv_buf_t],
        read_chan: *comm::chan<str>
    };

    crust fn after_close_cb(handle: *libc::c_void) {
        log(debug, #fmt("after uv_close! handle ptr: %?",
                        handle));
    }

    crust fn on_alloc_cb(handle: *libc::c_void,
                         ++suggested_size: libc::size_t)
        -> uv_buf_t unsafe {
        log(debug, "on_alloc_cb!");
        let char_ptr = malloc_buf_base_of(suggested_size);
        log(debug, #fmt("on_alloc_cb h: %? char_ptr: %u sugsize: %u",
                         handle,
                         char_ptr as uint,
                         suggested_size as uint));
        ret buf_init(char_ptr, suggested_size);
    }

    crust fn on_read_cb(stream: *uv_stream_t,
                        nread: libc::ssize_t,
                        ++buf: uv_buf_t) unsafe {
        log(debug, #fmt("CLIENT entering on_read_cb nred: %d", nread));
        if (nread > 0) {
            // we have data
            log(debug, #fmt("CLIENT read: data! nread: %d", nread));
            read_stop(stream);
            let client_data =
                get_data_for_uv_handle(stream as *libc::c_void)
                  as *request_wrapper;
            let buf_base = get_base_from_buf(buf);
            let buf_len = get_len_from_buf(buf);
            let bytes = vec::unsafe::from_buf(buf_base, buf_len);
            let read_chan = *((*client_data).read_chan);
            let msg_from_server = str::from_bytes(bytes);
            comm::send(read_chan, msg_from_server);
            close(stream as *libc::c_void, after_close_cb)
        }
        else if (nread == -1) {
            // err .. possibly EOF
            log(debug, "read: eof!");
        }
        else {
            // nread == 0 .. do nothing, just free buf as below
            log(debug, "read: do nothing!");
        }
        // when we're done
        free_base_of_buf(buf);
        log(debug, "CLIENT exiting on_read_cb");
    }

    crust fn on_write_complete_cb(write_req: *uv_write_t,
                                  status: libc::c_int) unsafe {
        log(debug, #fmt("CLIENT beginning on_write_complete_cb status: %d",
                         status as int));
        let stream = get_stream_handle_from_write_req(write_req);
        log(debug, #fmt("CLIENT on_write_complete_cb: tcp:%d write_handle:%d",
            stream as int, write_req as int));
        let result = read_start(stream, on_alloc_cb, on_read_cb);
        log(debug, #fmt("CLIENT ending on_write_complete_cb .. status: %d",
                         result as int));
    }

    crust fn on_connect_cb(connect_req_ptr: *uv_connect_t,
                                 status: libc::c_int) unsafe {
        log(debug, #fmt("beginning on_connect_cb .. status: %d",
                         status as int));
        let stream =
            get_stream_handle_from_connect_req(connect_req_ptr);
        if (status == 0i32) {
            log(debug, "on_connect_cb: in status=0 if..");
            let client_data = get_data_for_req(
                connect_req_ptr as *libc::c_void)
                as *request_wrapper;
            let write_handle = (*client_data).write_req;
            log(debug, #fmt("on_connect_cb: tcp: %d write_hdl: %d",
                            stream as int, write_handle as int));
            let write_result = write(write_handle,
                              stream as *libc::c_void,
                              (*client_data).req_buf,
                              on_write_complete_cb);
            log(debug, #fmt("on_connect_cb: write() status: %d",
                             write_result as int));
        }
        else {
            let test_loop = get_loop_for_uv_handle(
                stream as *libc::c_void);
            let err_msg = get_last_err_info(test_loop);
            log(debug, err_msg);
            assert false;
        }
        log(debug, "finishing on_connect_cb");
    }

    fn impl_uv_tcp_request(ip: str, port: int, req_str: str,
                          client_chan: *comm::chan<str>) unsafe {
        let test_loop = loop_new();
        let tcp_handle = tcp_t();
        let tcp_handle_ptr = ptr::addr_of(tcp_handle);
        let connect_handle = connect_t();
        let connect_req_ptr = ptr::addr_of(connect_handle);

        // this is the persistent payload of data that we
        // need to pass around to get this example to work.
        // In C, this would be a malloc'd or stack-allocated
        // struct that we'd cast to a void* and store as the
        // data field in our uv_connect_t struct
        let req_str_bytes = str::bytes(req_str);
        let req_msg_ptr: *u8 = vec::unsafe::to_ptr(req_str_bytes);
        log(debug, #fmt("req_msg ptr: %u", req_msg_ptr as uint));
        let req_msg = [
            buf_init(req_msg_ptr, vec::len(req_str_bytes))
        ];
        // this is the enclosing record, we'll pass a ptr to
        // this to C..
        let write_handle = write_t();
        let write_handle_ptr = ptr::addr_of(write_handle);
        log(debug, #fmt("tcp req: tcp stream: %d write_handle: %d",
                         tcp_handle_ptr as int,
                         write_handle_ptr as int));
        let client_data = { writer_handle: write_handle_ptr,
                    req_buf: ptr::addr_of(req_msg),
                    read_chan: client_chan };

        let tcp_init_result = tcp_init(
            test_loop as *libc::c_void, tcp_handle_ptr);
        if (tcp_init_result == 0i32) {
            log(debug, "sucessful tcp_init_result");

            log(debug, "building addr...");
            let addr = ip4_addr(ip, port);
            // FIXME ref #2064
            let addr_ptr = ptr::addr_of(addr);
            log(debug, #fmt("after build addr in rust. port: %u",
                             addr.sin_port as uint));

            // this should set up the connection request..
            log(debug, #fmt("b4 call tcp_connect connect cb: %u ",
                            on_connect_cb as uint));
            let tcp_connect_result = tcp_connect(
                connect_req_ptr, tcp_handle_ptr,
                addr_ptr, on_connect_cb);
            if (tcp_connect_result == 0i32) {
                // not set the data on the connect_req
                // until its initialized
                set_data_for_req(
                    connect_req_ptr as *libc::c_void,
                    ptr::addr_of(client_data) as *libc::c_void);
                set_data_for_uv_handle(
                    tcp_handle_ptr as *libc::c_void,
                    ptr::addr_of(client_data) as *libc::c_void);
                log(debug, "before run tcp req loop");
                run(test_loop);
                log(debug, "after run tcp req loop");
            }
            else {
               log(debug, "tcp_connect() failure");
               assert false;
            }
        }
        else {
            log(debug, "tcp_init() failure");
            assert false;
        }
        loop_delete(test_loop);

    }

    crust fn server_after_close_cb(handle: *libc::c_void) unsafe {
        log(debug, #fmt("SERVER server stream closed, should exit.. h: %?",
                   handle));
    }

    crust fn client_stream_after_close_cb(handle: *libc::c_void)
        unsafe {
        log(debug, "SERVER: closed client stream, now closing server stream");
        let client_data = get_data_for_uv_handle(
            handle) as
            *tcp_server_data;
        close((*client_data).server as *libc::c_void,
                      server_after_close_cb);
    }

    crust fn after_server_resp_write(req: *uv_write_t) unsafe {
        let client_stream_ptr =
            get_stream_handle_from_write_req(req);
        log(debug, "SERVER: resp sent... closing client stream");
        close(client_stream_ptr as *libc::c_void,
                      client_stream_after_close_cb)
    }

    crust fn on_server_read_cb(client_stream_ptr: *uv_stream_t,
                               nread: libc::ssize_t,
                               ++buf: uv_buf_t) unsafe {
        if (nread > 0) {
            // we have data
            log(debug, #fmt("SERVER read: data! nread: %d", nread));

            // pull out the contents of the write from the client
            let buf_base = get_base_from_buf(buf);
            let buf_len = get_len_from_buf(buf);
            log(debug, #fmt("SERVER buf base: %u, len: %u, nread: %d",
                             buf_base as uint,
                             buf_len as uint,
                             nread));
            let bytes = vec::unsafe::from_buf(buf_base, buf_len);
            let request_str = str::from_bytes(bytes);

            let client_data = get_data_for_uv_handle(
                client_stream_ptr as *libc::c_void) as *tcp_server_data;

            let server_kill_msg = (*client_data).server_kill_msg;
            let write_req = (*client_data).server_write_req;
            if (str::contains(request_str, server_kill_msg)) {
                log(debug, "SERVER: client req contains kill_msg!");
                log(debug, "SERVER: sending response to client");
                read_stop(client_stream_ptr);
                let server_chan = *((*client_data).server_chan);
                comm::send(server_chan, request_str);
                let write_result = write(
                    write_req,
                    client_stream_ptr as *libc::c_void,
                    (*client_data).server_resp_buf,
                    after_server_resp_write);
                log(debug, #fmt("SERVER: resp write result: %d",
                            write_result as int));
                if (write_result != 0i32) {
                    log(debug, "bad result for server resp write()");
                    log(debug, get_last_err_info(
                        get_loop_for_uv_handle(client_stream_ptr
                            as *libc::c_void)));
                    assert false;
                }
            }
            else {
                log(debug, "SERVER: client req !contain kill_msg!");
            }
        }
        else if (nread == -1) {
            // err .. possibly EOF
            log(debug, "read: eof!");
        }
        else {
            // nread == 0 .. do nothing, just free buf as below
            log(debug, "read: do nothing!");
        }
        // when we're done
        free_base_of_buf(buf);
        log(debug, "SERVER exiting on_read_cb");
    }

    crust fn server_connection_cb(server_stream_ptr:
                                    *uv_stream_t,
                                  status: libc::c_int) unsafe {
        log(debug, "client connecting!");
        let test_loop = get_loop_for_uv_handle(
                               server_stream_ptr as *libc::c_void);
        if status != 0i32 {
            let err_msg = get_last_err_info(test_loop);
            log(debug, #fmt("server_connect_cb: non-zero status: %?",
                         err_msg));
            ret;
        }
        let server_data = get_data_for_uv_handle(
            server_stream_ptr as *libc::c_void) as *tcp_server_data;
        let client_stream_ptr = (*server_data).client;
        let client_init_result = tcp_init(test_loop,
                                                  client_stream_ptr);
        set_data_for_uv_handle(
            client_stream_ptr as *libc::c_void,
            server_data as *libc::c_void);
        if (client_init_result == 0i32) {
            log(debug, "successfully initialized client stream");
            let accept_result = accept(server_stream_ptr as
                                                 *libc::c_void,
                                               client_stream_ptr as
                                                 *libc::c_void);
            if (accept_result == 0i32) {
                // start reading
                let read_result = read_start(
                    client_stream_ptr as *uv_stream_t,
                                                     on_alloc_cb,
                                                     on_server_read_cb);
                if (read_result == 0i32) {
                    log(debug, "successful server read start");
                }
                else {
                    log(debug, #fmt("server_connection_cb: bad read:%d",
                                    read_result as int));
                    assert false;
                }
            }
            else {
                log(debug, #fmt("server_connection_cb: bad accept: %d",
                            accept_result as int));
                assert false;
            }
        }
        else {
            log(debug, #fmt("server_connection_cb: bad client init: %d",
                        client_init_result as int));
            assert false;
        }
    }

    type tcp_server_data = {
        client: *uv_tcp_t,
        server: *uv_tcp_t,
        server_kill_msg: str,
        server_resp_buf: *[uv_buf_t],
        server_chan: *comm::chan<str>,
        server_write_req: *uv_write_t
    };

    type async_handle_data = {
        continue_chan: *comm::chan<bool>
    };

    crust fn async_close_cb(handle: *libc::c_void) {
        log(debug, #fmt("SERVER: closing async cb... h: %?",
                   handle));
    }

    crust fn continue_async_cb(async_handle: *uv_async_t,
                               status: libc::c_int) unsafe {
        // once we're in the body of this callback,
        // the tcp server's loop is set up, so we
        // can continue on to let the tcp client
        // do its thang
        let data = get_data_for_uv_handle(
            async_handle as *libc::c_void) as *async_handle_data;
        let continue_chan = *((*data).continue_chan);
        let should_continue = status == 0i32;
        comm::send(continue_chan, should_continue);
        close(async_handle as *libc::c_void, async_close_cb);
    }

    fn impl_uv_tcp_server(server_ip: str,
                          server_port: int,
                          kill_server_msg: str,
                          server_resp_msg: str,
                          server_chan: *comm::chan<str>,
                          continue_chan: *comm::chan<bool>) unsafe {
        let test_loop = loop_new();
        let tcp_server = tcp_t();
        let tcp_server_ptr = ptr::addr_of(tcp_server);

        let tcp_client = tcp_t();
        let tcp_client_ptr = ptr::addr_of(tcp_client);

        let server_write_req = write_t();
        let server_write_req_ptr = ptr::addr_of(server_write_req);

        let resp_str_bytes = str::bytes(server_resp_msg);
        let resp_msg_ptr: *u8 = vec::unsafe::to_ptr(resp_str_bytes);
        log(debug, #fmt("resp_msg ptr: %u", resp_msg_ptr as uint));
        let resp_msg = [
            buf_init(resp_msg_ptr, vec::len(resp_str_bytes))
        ];

        let continue_async_handle = async_t();
        let continue_async_handle_ptr =
            ptr::addr_of(continue_async_handle);
        let async_data =
            { continue_chan: continue_chan };
        let async_data_ptr = ptr::addr_of(async_data);

        let server_data: tcp_server_data = {
            client: tcp_client_ptr,
            server: tcp_server_ptr,
            server_kill_msg: kill_server_msg,
            server_resp_buf: ptr::addr_of(resp_msg),
            server_chan: server_chan,
            server_write_req: server_write_req_ptr
        };
        let server_data_ptr = ptr::addr_of(server_data);
        set_data_for_uv_handle(tcp_server_ptr as *libc::c_void,
                                       server_data_ptr as *libc::c_void);

        // uv_tcp_init()
        let tcp_init_result = tcp_init(
            test_loop as *libc::c_void, tcp_server_ptr);
        if (tcp_init_result == 0i32) {
            let server_addr = ip4_addr(server_ip, server_port);
            // FIXME ref #2064
            let server_addr_ptr = ptr::addr_of(server_addr);

            // uv_tcp_bind()
            let bind_result = tcp_bind(tcp_server_ptr,
                                               server_addr_ptr);
            if (bind_result == 0i32) {
                log(debug, "successful uv_tcp_bind, listening");

                // uv_listen()
                let listen_result = listen(tcp_server_ptr as
                                                     *libc::c_void,
                                                   128i32,
                                                   server_connection_cb);
                if (listen_result == 0i32) {
                    // let the test know it can set up the tcp server,
                    // now.. this may still present a race, not sure..
                    let async_result = async_init(test_loop,
                                       continue_async_handle_ptr,
                                       continue_async_cb);
                    if (async_result == 0i32) {
                        set_data_for_uv_handle(
                            continue_async_handle_ptr as *libc::c_void,
                            async_data_ptr as *libc::c_void);
                        async_send(continue_async_handle_ptr);
                        // uv_run()
                        run(test_loop);
                        log(debug, "server uv::run() has returned");
                    }
                    else {
                        log(debug, #fmt("uv_async_init failure: %d",
                                async_result as int));
                        assert false;
                    }
                }
                else {
                    log(debug, #fmt("non-zero result on uv_listen: %d",
                                listen_result as int));
                    assert false;
                }
            }
            else {
                log(debug, #fmt("non-zero result on uv_tcp_bind: %d",
                            bind_result as int));
                assert false;
            }
        }
        else {
            log(debug, #fmt("non-zero result on uv_tcp_init: %d",
                        tcp_init_result as int));
            assert false;
        }
        loop_delete(test_loop);
    }

    // this is the impl for a test that is (maybe) ran on a
    // per-platform/arch basis below
    fn impl_uv_tcp_server_and_request() unsafe {
        let bind_ip = "0.0.0.0";
        let request_ip = "127.0.0.1";
        let port = 8888;
        let kill_server_msg = "does a dog have buddha nature?";
        let server_resp_msg = "mu!";
        let client_port = comm::port::<str>();
        let client_chan = comm::chan::<str>(client_port);
        let server_port = comm::port::<str>();
        let server_chan = comm::chan::<str>(server_port);

        let continue_port = comm::port::<bool>();
        let continue_chan = comm::chan::<bool>(continue_port);
        let continue_chan_ptr = ptr::addr_of(continue_chan);

        task::spawn_sched(task::manual_threads(1u)) {||
            impl_uv_tcp_server(bind_ip, port,
                               kill_server_msg,
                               server_resp_msg,
                               ptr::addr_of(server_chan),
                               continue_chan_ptr);
        };

        // block until the server up is.. possibly a race?
        log(debug, "before receiving on server continue_port");
        comm::recv(continue_port);
        log(debug, "received on continue port, set up tcp client");

        task::spawn_sched(task::manual_threads(1u)) {||
            impl_uv_tcp_request(request_ip, port,
                               kill_server_msg,
                               ptr::addr_of(client_chan));
        };

        let msg_from_client = comm::recv(server_port);
        let msg_from_server = comm::recv(client_port);

        assert str::contains(msg_from_client, kill_server_msg);
        assert str::contains(msg_from_server, server_resp_msg);
    }

    // don't run this test on fbsd or 32bit linux
    #[cfg(target_os="win32")]
    #[cfg(target_os="darwin")]
    #[cfg(target_os="linux")]
    mod tcp_and_server_client_test {
        #[cfg(target_arch="x86_64")]
        mod impl64 {
            #[test]
            fn test_uv_ll_tcp_server_and_request() unsafe {
                impl_uv_tcp_server_and_request();
            }
        }
        #[cfg(target_arch="x86")]
        mod impl32 {
            #[test]
            #[ignore(cfg(target_os = "linux"))]
            fn test_uv_ll_tcp_server_and_request() unsafe {
                impl_uv_tcp_server_and_request();
            }
        }
    }

    // struct size tests
    #[test]
    #[ignore(cfg(target_os = "freebsd"))]
    fn test_uv_ll_struct_size_uv_tcp_t() {
        let native_handle_size = rustrt::rust_uv_helper_uv_tcp_t_size();
        let rust_handle_size = sys::size_of::<uv_tcp_t>();
        let output = #fmt("uv_tcp_t -- native: %u rust: %u",
                          native_handle_size as uint, rust_handle_size);
        log(debug, output);
        assert native_handle_size as uint == rust_handle_size;
    }
    #[test]
    #[ignore(cfg(target_os = "freebsd"))]
    fn test_uv_ll_struct_size_uv_connect_t() {
        let native_handle_size =
            rustrt::rust_uv_helper_uv_connect_t_size();
        let rust_handle_size = sys::size_of::<uv_connect_t>();
        let output = #fmt("uv_connect_t -- native: %u rust: %u",
                          native_handle_size as uint, rust_handle_size);
        log(debug, output);
        assert native_handle_size as uint == rust_handle_size;
    }
    #[test]
    #[ignore(cfg(target_os = "freebsd"))]
    fn test_uv_ll_struct_size_uv_buf_t() {
        let native_handle_size =
            rustrt::rust_uv_helper_uv_buf_t_size();
        let rust_handle_size = sys::size_of::<uv_buf_t>();
        let output = #fmt("uv_buf_t -- native: %u rust: %u",
                          native_handle_size as uint, rust_handle_size);
        log(debug, output);
        assert native_handle_size as uint == rust_handle_size;
    }
    #[test]
    #[ignore(cfg(target_os = "freebsd"))]
    fn test_uv_ll_struct_size_uv_write_t() {
        let native_handle_size =
            rustrt::rust_uv_helper_uv_write_t_size();
        let rust_handle_size = sys::size_of::<uv_write_t>();
        let output = #fmt("uv_write_t -- native: %u rust: %u",
                          native_handle_size as uint, rust_handle_size);
        log(debug, output);
        assert native_handle_size as uint == rust_handle_size;
    }

    #[test]
    #[ignore(cfg(target_os = "freebsd"))]
    fn test_uv_ll_struct_size_sockaddr_in() {
        let native_handle_size =
            rustrt::rust_uv_helper_sockaddr_in_size();
        let rust_handle_size = sys::size_of::<sockaddr_in>();
        let output = #fmt("sockaddr_in -- native: %u rust: %u",
                          native_handle_size as uint, rust_handle_size);
        log(debug, output);
        assert native_handle_size as uint == rust_handle_size;
    }

    #[test]
    #[ignore(cfg(target_os = "freebsd"))]
    fn test_uv_ll_struct_size_uv_async_t() {
        let native_handle_size =
            rustrt::rust_uv_helper_uv_async_t_size();
        let rust_handle_size = sys::size_of::<uv_async_t>();
        let output = #fmt("uv_async_t -- native: %u rust: %u",
                          native_handle_size as uint, rust_handle_size);
        log(debug, output);
        assert native_handle_size as uint == rust_handle_size;
    }

    #[test]
    #[ignore(cfg(target_os = "freebsd"))]
    fn test_uv_ll_struct_size_uv_timer_t() {
        let native_handle_size =
            rustrt::rust_uv_helper_uv_timer_t_size();
        let rust_handle_size = sys::size_of::<uv_timer_t>();
        let output = #fmt("uv_timer_t -- native: %u rust: %u",
                          native_handle_size as uint, rust_handle_size);
        log(debug, output);
        assert native_handle_size as uint == rust_handle_size;
    }
}