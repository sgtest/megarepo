// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// ignore-test leaks
// error-pattern:ran out of stack

// Test that the task fails after hitting the recursion limit
// during unwinding

fn recurse() {
    info!("don't optimize me out");
    recurse();
}

struct r {
    recursed: *mut bool,
}

impl Drop for r {
    fn drop(&mut self) {
        unsafe {
            if !*(self.recursed) {
                *(self.recursed) = true;
                recurse();
            }
        }
    }
}

fn r(recursed: *mut bool) -> r {
    r { recursed: recursed }
}

fn main() {
    let mut recursed = false;
    let _r = r(&mut recursed);
    recurse();
}
