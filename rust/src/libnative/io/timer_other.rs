// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Timers for non-linux/non-windows OSes
//!
//! This module implements timers with a worker thread, select(), and a lot of
//! witchcraft that turns out to be horribly inaccurate timers. The unfortunate
//! part is that I'm at a loss of what else to do one these OSes. This is also
//! why linux has a specialized timerfd implementation and windows has its own
//! implementation (they're more accurate than this one).
//!
//! The basic idea is that there is a worker thread that's communicated to via a
//! channel and a pipe, the pipe is used by the worker thread in a select()
//! syscall with a timeout. The timeout is the "next timer timeout" while the
//! channel is used to send data over to the worker thread.
//!
//! Whenever the call to select() times out, then a channel receives a message.
//! Whenever the call returns that the file descriptor has information, then the
//! channel from timers is drained, enqueueing all incoming requests.
//!
//! The actual implementation of the helper thread is a sorted array of
//! timers in terms of target firing date. The target is the absolute time at
//! which the timer should fire. Timers are then re-enqueued after a firing if
//! the repeat boolean is set.
//!
//! Naturally, all this logic of adding times and keeping track of
//! relative/absolute time is a little lossy and not quite exact. I've done the
//! best I could to reduce the amount of calls to 'now()', but there's likely
//! still inaccuracies trickling in here and there.
//!
//! One of the tricky parts of this implementation is that whenever a timer is
//! acted upon, it must cancel whatever the previous action was (if one is
//! active) in order to act like the other implementations of this timer. In
//! order to do this, the timer's inner pointer is transferred to the worker
//! thread. Whenever the timer is modified, it first takes ownership back from
//! the worker thread in order to modify the same data structure. This has the
//! side effect of "cancelling" the previous requests while allowing a
//! re-enqueueing later on.
//!
//! Note that all time units in this file are in *milliseconds*.

#[allow(non_camel_case_types)];

use std::comm::Data;
use std::hashmap::HashMap;
use std::libc;
use std::mem;
use std::os;
use std::ptr;
use std::rt::rtio;
use std::sync::atomics;

use io::file::FileDesc;
use io::IoResult;
use io::timer_helper;

pub struct Timer {
    priv id: uint,
    priv inner: Option<~Inner>,
}

struct Inner {
    chan: Option<Chan<()>>,
    interval: u64,
    repeat: bool,
    target: u64,
    id: uint,
}

pub enum Req {
    // Add a new timer to the helper thread.
    NewTimer(~Inner),

    // Remove a timer based on its id and then send it back on the channel
    // provided
    RemoveTimer(uint, Chan<~Inner>),

    // Shut down the loop and then ACK this channel once it's shut down
    Shutdown,
}

// returns the current time (in milliseconds)
fn now() -> u64 {
    unsafe {
        let mut now: libc::timeval = mem::init();
        assert_eq!(imp::gettimeofday(&mut now, ptr::null()), 0);
        return (now.tv_sec as u64) * 1000 + (now.tv_usec as u64) / 1000;
    }
}

fn helper(input: libc::c_int, messages: Port<Req>) {
    let mut set: imp::fd_set = unsafe { mem::init() };

    let mut fd = FileDesc::new(input, true);
    let mut timeout: libc::timeval = unsafe { mem::init() };

    // active timers are those which are able to be selected upon (and it's a
    // sorted list, and dead timers are those which have expired, but ownership
    // hasn't yet been transferred back to the timer itself.
    let mut active: ~[~Inner] = ~[];
    let mut dead = HashMap::new();

    // inserts a timer into an array of timers (sorted by firing time)
    fn insert(t: ~Inner, active: &mut ~[~Inner]) {
        match active.iter().position(|tm| tm.target > t.target) {
            Some(pos) => { active.insert(pos, t); }
            None => { active.push(t); }
        }
    }

    // signals the first requests in the queue, possible re-enqueueing it.
    fn signal(active: &mut ~[~Inner], dead: &mut HashMap<uint, ~Inner>) {
        let mut timer = match active.shift() {
            Some(timer) => timer, None => return
        };
        let chan = timer.chan.take_unwrap();
        if chan.try_send(()) && timer.repeat {
            timer.chan = Some(chan);
            timer.target += timer.interval;
            insert(timer, active);
        } else {
            drop(chan);
            dead.insert(timer.id, timer);
        }
    }

    'outer: loop {
        let timeout = if active.len() == 0 {
            // Empty array? no timeout (wait forever for the next request)
            ptr::null()
        } else {
            let now = now();
            // If this request has already expired, then signal it and go
            // through another iteration
            if active[0].target <= now {
                signal(&mut active, &mut dead);
                continue;
            }

            // The actual timeout listed in the requests array is an
            // absolute date, so here we translate the absolute time to a
            // relative time.
            let tm = active[0].target - now;
            timeout.tv_sec = (tm / 1000) as libc::time_t;
            timeout.tv_usec = ((tm % 1000) * 1000) as libc::suseconds_t;
            &timeout as *libc::timeval
        };

        imp::fd_set(&mut set, input);
        match unsafe {
            imp::select(input + 1, &set, ptr::null(), ptr::null(), timeout)
        } {
            // timed out
            0 => signal(&mut active, &mut dead),

            // file descriptor write woke us up, we've got some new requests
            1 => {
                loop {
                    match messages.try_recv() {
                        Data(Shutdown) => {
                            assert!(active.len() == 0);
                            break 'outer;
                        }

                        Data(NewTimer(timer)) => insert(timer, &mut active),

                        Data(RemoveTimer(id, ack)) => {
                            match dead.pop(&id) {
                                Some(i) => { ack.send(i); continue }
                                None => {}
                            }
                            let i = active.iter().position(|i| i.id == id);
                            let i = i.expect("no timer found");
                            let t = active.remove(i).unwrap();
                            ack.send(t);
                        }
                        _ => break
                    }
                }

                // drain the file descriptor
                let mut buf = [0];
                assert_eq!(fd.inner_read(buf).unwrap(), 1);
            }

            -1 if os::errno() == libc::EINTR as int => {}
            n => fail!("helper thread failed in select() with error: {} ({})",
                       n, os::last_os_error())
        }
    }
}

impl Timer {
    pub fn new() -> IoResult<Timer> {
        timer_helper::boot(helper);

        static mut ID: atomics::AtomicUint = atomics::INIT_ATOMIC_UINT;
        let id = unsafe { ID.fetch_add(1, atomics::Relaxed) };
        Ok(Timer {
            id: id,
            inner: Some(~Inner {
                chan: None,
                interval: 0,
                target: 0,
                repeat: false,
                id: id,
            })
        })
    }

    pub fn sleep(ms: u64) {
        // FIXME: this can fail because of EINTR, what do do?
        let _ = unsafe { libc::usleep((ms * 1000) as libc::c_uint) };
    }

    fn inner(&mut self) -> ~Inner {
        match self.inner.take() {
            Some(i) => i,
            None => {
                let (p, c) = Chan::new();
                timer_helper::send(RemoveTimer(self.id, c));
                p.recv()
            }
        }
    }
}

impl rtio::RtioTimer for Timer {
    fn sleep(&mut self, msecs: u64) {
        let mut inner = self.inner();
        inner.chan = None; // cancel any previous request
        self.inner = Some(inner);

        Timer::sleep(msecs);
    }

    fn oneshot(&mut self, msecs: u64) -> Port<()> {
        let now = now();
        let mut inner = self.inner();

        let (p, c) = Chan::new();
        inner.repeat = false;
        inner.chan = Some(c);
        inner.interval = msecs;
        inner.target = now + msecs;

        timer_helper::send(NewTimer(inner));
        return p;
    }

    fn period(&mut self, msecs: u64) -> Port<()> {
        let now = now();
        let mut inner = self.inner();

        let (p, c) = Chan::new();
        inner.repeat = true;
        inner.chan = Some(c);
        inner.interval = msecs;
        inner.target = now + msecs;

        timer_helper::send(NewTimer(inner));
        return p;
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        self.inner = Some(self.inner());
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::libc;

    pub static FD_SETSIZE: uint = 1024;

    pub struct fd_set {
        fds_bits: [i32, ..(FD_SETSIZE / 32)]
    }

    pub fn fd_set(set: &mut fd_set, fd: i32) {
        set.fds_bits[fd / 32] |= 1 << (fd % 32);
    }

    extern {
        pub fn select(nfds: libc::c_int,
                      readfds: *fd_set,
                      writefds: *fd_set,
                      errorfds: *fd_set,
                      timeout: *libc::timeval) -> libc::c_int;

        pub fn gettimeofday(timeval: *mut libc::timeval,
                            tzp: *libc::c_void) -> libc::c_int;
    }
}

#[cfg(target_os = "android")]
#[cfg(target_os = "freebsd")]
mod imp {
    use std::libc;

    pub static FD_SETSIZE: uint = 1024;

    pub struct fd_set {
        fds_bits: [u64, ..(FD_SETSIZE / 64)]
    }

    pub fn fd_set(set: &mut fd_set, fd: i32) {
        set.fds_bits[fd / 64] |= (1 << (fd % 64)) as u64;
    }

    extern {
        pub fn select(nfds: libc::c_int,
                      readfds: *fd_set,
                      writefds: *fd_set,
                      errorfds: *fd_set,
                      timeout: *libc::timeval) -> libc::c_int;

        pub fn gettimeofday(timeval: *mut libc::timeval,
                            tzp: *libc::c_void) -> libc::c_int;
    }
}
