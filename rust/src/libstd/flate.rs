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

Simple compression

*/

use libc;
use core::libc::{c_void, size_t, c_int};
use vec;

#[cfg(test)] use core::rand;
#[cfg(test)] use core::rand::RngUtil;

pub mod rustrt {
    use core::libc::{c_int, c_void, size_t};

    #[link_name = "rustrt"]
    pub extern {
        unsafe fn tdefl_compress_mem_to_heap(psrc_buf: *const c_void,
                                             src_buf_len: size_t,
                                             pout_len: *mut size_t,
                                             flags: c_int)
                                          -> *c_void;

        unsafe fn tinfl_decompress_mem_to_heap(psrc_buf: *const c_void,
                                               src_buf_len: size_t,
                                               pout_len: *mut size_t,
                                               flags: c_int)
                                            -> *c_void;
    }
}

static lz_none : c_int = 0x0;   // Huffman-coding only.
static lz_fast : c_int = 0x1;   // LZ with only one probe
static lz_norm : c_int = 0x80;  // LZ with 128 probes, "normal"
static lz_best : c_int = 0xfff; // LZ with 4095 probes, "best"

pub fn deflate_bytes(bytes: &const [u8]) -> ~[u8] {
    do vec::as_const_buf(bytes) |b, len| {
        unsafe {
            let mut outsz : size_t = 0;
            let res =
                rustrt::tdefl_compress_mem_to_heap(b as *c_void,
                                                   len as size_t,
                                                   &mut outsz,
                                                   lz_norm);
            assert!(res as int != 0);
            let out = vec::raw::from_buf_raw(res as *u8,
                                             outsz as uint);
            libc::free(res);
            out
        }
    }
}

pub fn inflate_bytes(bytes: &const [u8]) -> ~[u8] {
    do vec::as_const_buf(bytes) |b, len| {
        unsafe {
            let mut outsz : size_t = 0;
            let res =
                rustrt::tinfl_decompress_mem_to_heap(b as *c_void,
                                                     len as size_t,
                                                     &mut outsz,
                                                     0);
            assert!(res as int != 0);
            let out = vec::raw::from_buf_raw(res as *u8,
                                            outsz as uint);
            libc::free(res);
            out
        }
    }
}

#[test]
#[allow(non_implicitly_copyable_typarams)]
fn test_flate_round_trip() {
    let mut r = rand::rng();
    let mut words = ~[];
    for 20.times {
        let range = r.gen_uint_range(1, 10);
        words.push(r.gen_bytes(range));
    }
    for 20.times {
        let mut in = ~[];
        for 2000.times {
            in.push_all(r.choose(words));
        }
        debug!("de/inflate of %u bytes of random word-sequences",
               in.len());
        let cmp = deflate_bytes(in);
        let out = inflate_bytes(cmp);
        debug!("%u bytes deflated to %u (%.1f%% size)",
               in.len(), cmp.len(),
               100.0 * ((cmp.len() as float) / (in.len() as float)));
        assert_eq!(in, out);
    }
}
