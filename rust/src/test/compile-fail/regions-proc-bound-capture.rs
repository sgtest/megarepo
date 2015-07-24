// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// FIXME (#22405): Replace `Box::new` with `box` here when/if possible.

fn borrowed_proc<'a>(x: &'a isize) -> Box<FnMut()->(isize) + 'a> {
    // This is legal, because the region bound on `proc`
    // states that it captures `x`.
    Box::new(move|| { *x })
}

fn static_proc(x: &isize) -> Box<FnMut()->(isize) + 'static> {
    // This is illegal, because the region bound on `proc` is 'static.
    Box::new(move|| { *x }) //~ ERROR does not fulfill the required lifetime
}

fn main() { }
