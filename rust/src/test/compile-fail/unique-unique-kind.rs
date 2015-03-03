// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::rc::Rc;

fn f<T:Send>(__isize: T) {
}

fn main() {
    // FIXME (#22405): Replace `Box::new` with `box` here when/if possible.
    let i = Box::new(Rc::new(100));
    f(i);
    //~^ ERROR `core::marker::Send` is not implemented
}
