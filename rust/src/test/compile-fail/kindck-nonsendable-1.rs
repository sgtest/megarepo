// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(managed_boxes)]

fn foo(_x: @uint) {}

fn main() {
    let x = @3u;
    let _: proc():Send = proc() foo(x); //~ ERROR does not fulfill `Send`
    let _: proc():Send = proc() foo(x); //~ ERROR does not fulfill `Send`
    let _: proc():Send = proc() foo(x); //~ ERROR does not fulfill `Send`
    let _: proc() = proc() foo(x);
}
