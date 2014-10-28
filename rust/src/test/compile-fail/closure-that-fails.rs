// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

fn foo(f: || -> !) {}

fn main() {
    // Type inference didn't use to be able to handle this:
    foo(|| fail!());
    foo(|| -> ! fail!());
    foo(|| 22i); //~ ERROR computation may converge in a function marked as diverging
    foo(|| -> ! 22i); //~ ERROR computation may converge in a function marked as diverging
    let x = || -> ! 1i; //~ ERROR computation may converge in a function marked as diverging
}
