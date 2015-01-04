// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

extern crate libc;
use std::thread::Thread;

mod rustrt {
    extern crate libc;

    #[link(name = "rust_test_helpers")]
    extern {
        pub fn rust_dbg_call(cb: extern "C" fn (libc::uintptr_t) -> libc::uintptr_t,
                             data: libc::uintptr_t)
                             -> libc::uintptr_t;
    }
}

extern fn cb(data: libc::uintptr_t) -> libc::uintptr_t {
    if data == 1 {
        data
    } else {
        count(data - 1) + count(data - 1)
    }
}

fn count(n: libc::uintptr_t) -> libc::uintptr_t {
    unsafe {
        Thread::yield_now();
        rustrt::rust_dbg_call(cb, n)
    }
}

pub fn main() {
    range(0, 10u).map(|i| {
        Thread::spawn(move|| {
            let result = count(5);
            println!("result = {}", result);
            assert_eq!(result, 16);
        })
    }).collect::<Vec<_>>();
}
