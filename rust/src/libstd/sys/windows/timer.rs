// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Timers based on Windows WaitableTimers
//!
//! This implementation is meant to be used solely on windows. As with other
//! implementations, there is a worker thread which is doing all the waiting on
//! a large number of timers for all active timers in the system. This worker
//! thread uses the select() equivalent, WaitForMultipleObjects. One of the
//! objects being waited on is a signal into the worker thread to notify that
//! the incoming channel should be looked at.
//!
//! Other than that, the implementation is pretty straightforward in terms of
//! the other two implementations of timers with nothing *that* new showing up.

use libc;
use ptr;
use comm;

use sys::c;
use sys::fs::FileDesc;
use sys_common::helper_thread::Helper;
use prelude::*;
use io::IoResult;

helper_init!(static HELPER: Helper<Req>)

pub trait Callback {
    fn call(&mut self);
}

pub struct Timer {
    obj: libc::HANDLE,
    on_worker: bool,
}

pub enum Req {
    NewTimer(libc::HANDLE, Box<Callback + Send>, bool),
    RemoveTimer(libc::HANDLE, Sender<()>),
}

fn helper(input: libc::HANDLE, messages: Receiver<Req>, _: ()) {
    let mut objs = vec![input];
    let mut chans = vec![];

    'outer: loop {
        let idx = unsafe {
            imp::WaitForMultipleObjects(objs.len() as libc::DWORD,
                                        objs.as_ptr(),
                                        0 as libc::BOOL,
                                        libc::INFINITE)
        };

        if idx == 0 {
            loop {
                match messages.try_recv() {
                    Ok(NewTimer(obj, c, one)) => {
                        objs.push(obj);
                        chans.push((c, one));
                    }
                    Ok(RemoveTimer(obj, c)) => {
                        c.send(());
                        match objs.iter().position(|&o| o == obj) {
                            Some(i) => {
                                drop(objs.remove(i));
                                drop(chans.remove(i - 1));
                            }
                            None => {}
                        }
                    }
                    Err(comm::Disconnected) => {
                        assert_eq!(objs.len(), 1);
                        assert_eq!(chans.len(), 0);
                        break 'outer;
                    }
                    Err(..) => break
                }
            }
        } else {
            let remove = {
                match &mut chans[idx as uint - 1] {
                    &(ref mut c, oneshot) => { c.call(); oneshot }
                }
            };
            if remove {
                drop(objs.remove(idx as uint));
                drop(chans.remove(idx as uint - 1));
            }
        }
    }
}

// returns the current time (in milliseconds)
pub fn now() -> u64 {
    let mut ticks_per_s = 0;
    assert_eq!(unsafe { libc::QueryPerformanceFrequency(&mut ticks_per_s) }, 1);
    let ticks_per_s = if ticks_per_s == 0 {1} else {ticks_per_s};
    let mut ticks = 0;
    assert_eq!(unsafe { libc::QueryPerformanceCounter(&mut ticks) }, 1);

    return (ticks as u64 * 1000) / (ticks_per_s as u64);
}

impl Timer {
    pub fn new() -> IoResult<Timer> {
        HELPER.boot(|| {}, helper);

        let obj = unsafe {
            imp::CreateWaitableTimerA(ptr::null_mut(), 0, ptr::null())
        };
        if obj.is_null() {
            Err(super::last_error())
        } else {
            Ok(Timer { obj: obj, on_worker: false, })
        }
    }

    fn remove(&mut self) {
        if !self.on_worker { return }

        let (tx, rx) = channel();
        HELPER.send(RemoveTimer(self.obj, tx));
        rx.recv();

        self.on_worker = false;
    }

    pub fn sleep(&mut self, msecs: u64) {
        self.remove();

        // there are 10^6 nanoseconds in a millisecond, and the parameter is in
        // 100ns intervals, so we multiply by 10^4.
        let due = -(msecs as i64 * 10000) as libc::LARGE_INTEGER;
        assert_eq!(unsafe {
            imp::SetWaitableTimer(self.obj, &due, 0, ptr::null_mut(),
                                  ptr::null_mut(), 0)
        }, 1);

        let _ = unsafe { imp::WaitForSingleObject(self.obj, libc::INFINITE) };
    }

    pub fn oneshot(&mut self, msecs: u64, cb: Box<Callback + Send>) {
        self.remove();

        // see above for the calculation
        let due = -(msecs as i64 * 10000) as libc::LARGE_INTEGER;
        assert_eq!(unsafe {
            imp::SetWaitableTimer(self.obj, &due, 0, ptr::null_mut(),
                                  ptr::null_mut(), 0)
        }, 1);

        HELPER.send(NewTimer(self.obj, cb, true));
        self.on_worker = true;
    }

    pub fn period(&mut self, msecs: u64, cb: Box<Callback + Send>) {
        self.remove();

        // see above for the calculation
        let due = -(msecs as i64 * 10000) as libc::LARGE_INTEGER;
        assert_eq!(unsafe {
            imp::SetWaitableTimer(self.obj, &due, msecs as libc::LONG,
                                  ptr::null_mut(), ptr::null_mut(), 0)
        }, 1);

        HELPER.send(NewTimer(self.obj, cb, false));
        self.on_worker = true;
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        self.remove();
        assert!(unsafe { libc::CloseHandle(self.obj) != 0 });
    }
}

mod imp {
    use libc::{LPSECURITY_ATTRIBUTES, BOOL, LPCSTR, HANDLE, LARGE_INTEGER,
                    LONG, LPVOID, DWORD, c_void};

    pub type PTIMERAPCROUTINE = *mut c_void;

    extern "system" {
        pub fn CreateWaitableTimerA(lpTimerAttributes: LPSECURITY_ATTRIBUTES,
                                    bManualReset: BOOL,
                                    lpTimerName: LPCSTR) -> HANDLE;
        pub fn SetWaitableTimer(hTimer: HANDLE,
                                pDueTime: *const LARGE_INTEGER,
                                lPeriod: LONG,
                                pfnCompletionRoutine: PTIMERAPCROUTINE,
                                lpArgToCompletionRoutine: LPVOID,
                                fResume: BOOL) -> BOOL;
        pub fn WaitForMultipleObjects(nCount: DWORD,
                                      lpHandles: *const HANDLE,
                                      bWaitAll: BOOL,
                                      dwMilliseconds: DWORD) -> DWORD;
        pub fn WaitForSingleObject(hHandle: HANDLE,
                                   dwMilliseconds: DWORD) -> DWORD;
    }
}
