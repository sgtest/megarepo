// Copyright 2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// revisions: ast mir
//[mir]compile-flags: -Z emit-end-regions -Z borrowck-mir

use std::cell::RefCell;

fn main() {
    let mut r = 0;
    let s = 0;
    let x = RefCell::new((&mut r,s));

    let val: &_ = x.borrow().0;
    //[ast]~^ ERROR borrowed value does not live long enough [E0597]
    //[ast]~| NOTE temporary value dropped here while still borrowed
    //[ast]~| NOTE temporary value created here
    //[ast]~| NOTE consider using a `let` binding to increase its lifetime
    //[mir]~^^^^^ ERROR borrowed value does not live long enough (Ast) [E0597]
    //[mir]~| NOTE temporary value dropped here while still borrowed
    //[mir]~| NOTE temporary value created here
    //[mir]~| NOTE consider using a `let` binding to increase its lifetime
    //[mir]~| ERROR borrowed value does not live long enough (Mir) [E0597]
    //[mir]~| NOTE temporary value dropped here while still borrowed
    //[mir]~| NOTE temporary value created here
    //[mir]~| NOTE consider using a `let` binding to increase its lifetime
    println!("{}", val);
}
//[ast]~^ NOTE temporary value needs to live until here
//[mir]~^^ NOTE temporary value needs to live until here
//[mir]~| NOTE temporary value needs to live until here
