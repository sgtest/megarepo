// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
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

use any::Any;
use cell::RefCell;
use sys::stdio::Stderr;
use sys_common::backtrace;
use sys_common::thread_info;
use sys_common::unwind;

thread_local! {
    pub static LOCAL_STDERR: RefCell<Option<Box<Write + Send>>> = {
        RefCell::new(None)
    }
}

pub fn on_panic(obj: &(Any+Send), file: &'static str, line: u32) {
    let msg = match obj.downcast_ref::<&'static str>() {
        Some(s) => *s,
        None => match obj.downcast_ref::<String>() {
            Some(s) => &s[..],
            None => "Box<Any>",
        }
    };
    let mut err = Stderr::new().ok();
    let thread = thread_info::current_thread();
    let name = thread.as_ref().and_then(|t| t.name()).unwrap_or("<unnamed>");
    let prev = LOCAL_STDERR.with(|s| s.borrow_mut().take());
    match (prev, err.as_mut()) {
        (Some(mut stderr), _) => {
            // FIXME: what to do when the thread printing panics?
            let _ = writeln!(stderr,
                             "thread '{}' panicked at '{}', {}:{}\n",
                             name, msg, file, line);
            if backtrace::log_enabled() {
                let _ = backtrace::write(&mut *stderr);
            }
            let mut s = Some(stderr);
            LOCAL_STDERR.with(|slot| {
                *slot.borrow_mut() = s.take();
            });
        }
        (None, Some(ref mut err)) => {
            let _ = writeln!(err, "thread '{}' panicked at '{}', {}:{}",
                             name, msg, file, line);
            if backtrace::log_enabled() {
                let _ = backtrace::write(err);
            }
        }
        _ => {}
    }

    // If this is a double panic, make sure that we printed a backtrace
    // for this panic.
    match err {
        Some(ref mut err) if unwind::panicking() && !backtrace::log_enabled() => {
            let _ = backtrace::write(err);
        }
        _ => {}
    }
}
