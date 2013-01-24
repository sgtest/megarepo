// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// This test attempts to force the dynamic linker to resolve
// external symbols as close to the red zone as possible.

extern mod rustrt {
    #[legacy_exports];
    fn debug_get_stk_seg() -> *u8;

    fn rust_get_sched_id() -> libc::intptr_t;
    fn last_os_error() -> ~str;
    fn rust_getcwd() -> ~str;
    fn get_task_id() -> libc::intptr_t;
    fn rust_sched_threads();
    fn rust_get_task();
}

fn calllink01() { unsafe { rustrt::rust_get_sched_id(); } }
fn calllink02() { unsafe { rustrt::last_os_error(); } }
fn calllink03() { unsafe { rustrt::rust_getcwd(); } }
fn calllink08() { unsafe { rustrt::get_task_id(); } }
fn calllink09() { unsafe { rustrt::rust_sched_threads(); } }
fn calllink10() { unsafe { rustrt::rust_get_task(); } }

fn runtest(f: fn~(), frame_backoff: u32) {
    runtest2(f, frame_backoff, 0 as *u8);
}

fn runtest2(f: fn~(), frame_backoff: u32, last_stk: *u8) -> u32 {
    unsafe {
        let curr_stk = rustrt::debug_get_stk_seg();
        if (last_stk != curr_stk && last_stk != 0 as *u8) {
            // We switched stacks, go back and try to hit the dynamic linker
            frame_backoff
        } else {
            let frame_backoff = runtest2(copy f, frame_backoff, curr_stk);
            if frame_backoff > 1u32 {
                frame_backoff - 1u32
            } else if frame_backoff == 1u32 {
                f();
                0u32
            } else {
                0u32
            }
        }
    }
}

fn main() {
    let fns = ~[
        calllink01,
        calllink02,
        calllink03,
        calllink08,
        calllink09,
        calllink10
    ];
    let rng = rand::Rng();
    for fns.each |f| {
        let f = *f;
        let sz = rng.next() % 256u32 + 256u32;
        let frame_backoff = rng.next() % 10u32 + 1u32;
        task::try(|move f| runtest(f, frame_backoff) );
    }
}
