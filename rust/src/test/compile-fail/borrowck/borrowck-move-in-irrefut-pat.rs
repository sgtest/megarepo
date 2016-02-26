// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

fn with<F>(f: F) where F: FnOnce(&String) {}

fn arg_item(&_x: &String) {}
    //~^ ERROR cannot move out of borrowed content

fn arg_closure() {
    with(|&_x| ())
    //~^ ERROR cannot move out of borrowed content
}

fn let_pat() {
    let &_x = &"hi".to_string();
    //~^ ERROR cannot move out of borrowed content
}

pub fn main() {}
