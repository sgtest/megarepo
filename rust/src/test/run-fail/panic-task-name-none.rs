// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// error-pattern:thread '<unnamed>' panicked at 'test'

use std::thread::Thread;

fn main() {
    let r: Result<int,_> = Thread::scoped(move|| {
        panic!("test");
        1
    }).join();
    assert!(r.is_ok());
}
