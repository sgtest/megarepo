// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(unboxed_closures)]

use std::ops::{Deref, DerefMut};

struct X(Box<int>);

static mut DESTRUCTOR_RAN: bool = false;

impl Drop for X {
    fn drop(&mut self) {
        unsafe {
            assert!(!DESTRUCTOR_RAN);
            DESTRUCTOR_RAN = true;
        }
    }
}

impl Deref for X {
    type Target = int;

    fn deref(&self) -> &int {
        let &X(box ref x) = self;
        x
    }
}

impl DerefMut for X {
    fn deref_mut(&mut self) -> &mut int {
        let &X(box ref mut x) = self;
        x
    }
}

fn main() {
    {
        let mut test = X(box 5i);
        {
            let mut change = |&mut:| { *test = 10 };
            change();
        }
        assert_eq!(*test, 10);
    }
    assert!(unsafe { DESTRUCTOR_RAN });
}
