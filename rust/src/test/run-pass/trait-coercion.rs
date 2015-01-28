// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(unknown_features)]
#![feature(box_syntax)]

use std::old_io;

trait Trait {
    fn f(&self);
}

#[derive(Copy)]
struct Struct {
    x: int,
    y: int,
}

impl Trait for Struct {
    fn f(&self) {
        println!("Hi!");
    }
}

fn foo(mut a: Box<Writer>) {
    a.write(b"Hello\n");
}

pub fn main() {
    let a = Struct { x: 1, y: 2 };
    let b: Box<Trait> = box a;
    b.f();
    let c: &Trait = &a;
    c.f();

    let out = old_io::stdout();
    foo(box out);
}

