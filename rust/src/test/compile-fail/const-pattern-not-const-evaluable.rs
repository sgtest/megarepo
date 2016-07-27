// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(const_fn)]

enum Cake {
    BlackForest,
    Marmor,
}
use Cake::*;

const BOO: (Cake, Cake) = (Marmor, BlackForest);
//~^ ERROR: constant evaluation error [E0080]
//~| unimplemented constant expression: enum variants
const FOO: Cake = BOO.1;

const fn foo() -> Cake {
    Marmor
        //~^ ERROR: constant evaluation error [E0080]
        //~| unimplemented constant expression: enum variants
        //~^^^ ERROR: constant evaluation error [E0080]
        //~| unimplemented constant expression: enum variants
}

const WORKS: Cake = Marmor;

const GOO: Cake = foo(); //~ NOTE for expression here

fn main() {
    match BlackForest {
        FOO => println!("hi"), //~ NOTE: for pattern here
        GOO => println!("meh"), //~ NOTE: for pattern here
        WORKS => println!("möp"),
        _ => println!("bye"),
    }
}
