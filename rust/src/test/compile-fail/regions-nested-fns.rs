// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// FIXME (#22405): Replace `Box::new` with `box` here when/if possible.

fn ignore<T>(t: T) {}

fn nested<'x>(x: &'x isize) {
    let y = 3;
    let mut ay = &y;

    ignore::<Box<for<'z> FnMut(&'z isize)>>(Box::new(|z| {
        ay = x; //~ ERROR cannot infer
        ay = &y;
        ay = z;
    }));

    ignore::< Box<for<'z> FnMut(&'z isize) -> &'z isize>>(Box::new(|z| {
        if false { return x; }  //~ ERROR cannot infer an appropriate lifetime for automatic
        if false { return ay; }
        return z;
    }));
}

fn main() {}
