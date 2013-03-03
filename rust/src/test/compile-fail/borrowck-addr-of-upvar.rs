// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

fn foo(x: @int) -> @fn() -> &static/int {
    let result: @fn() -> &static/int = || &*x;  //~ ERROR illegal borrow
    result
}

fn bar(x: @int) -> @fn() -> &int {
    let result: @fn() -> &int = || &*x; //~ ERROR illegal borrow
    result
}

fn zed(x: @int) -> @fn() -> int {
    let result: @fn() -> int = || *&*x;
    result
}

fn main() {
}
