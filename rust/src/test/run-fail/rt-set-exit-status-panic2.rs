// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// error-pattern:whatever

#[macro_use] extern crate log;
use std::os;
use std::thread::Thread;

struct r {
  x:int,
}

// Setting the exit status after the runtime has already
// panicked has no effect and the process exits with the
// runtime's exit code
impl Drop for r {
    fn drop(&mut self) {
        os::set_exit_status(50);
    }
}

fn r(x:int) -> r {
    r {
        x: x
    }
}

fn main() {
    error!("whatever");
    let _t = Thread::spawn(move|| {
      let _i = r(5);
    });
    panic!();
}
