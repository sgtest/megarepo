// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
#![feature(rustc_attrs)]
// compile-flags: -Z no-landing-pads
// error-pattern:diverging_fn called
use std::io::{self, Write};

struct Droppable;
impl Drop for Droppable {
    fn drop(&mut self) {
        ::std::process::exit(1)
    }
}

fn diverging_fn() -> ! {
    panic!("diverging_fn called")
}

#[rustc_mir]
fn mir(d: Droppable) {
    let x = Droppable;
    diverging_fn();
    drop(x);
    drop(d);
}

fn main() {
    mir(Droppable);
}
