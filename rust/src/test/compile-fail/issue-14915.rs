// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::gc::{GC,Gc};

fn main() {
    let x: Box<int> = box 0;
    let y: Gc<int> = box (GC) 0;

    println!("{}", x + 1); //~ ERROR binary operation `+` cannot be applied to type `Box<int>`
    //~^ ERROR cannot determine a type for this bounded type parameter: unconstrained type
    println!("{}", y + 1);
    //~^ ERROR binary operation `+` cannot be applied to type `Gc<int>`
    //~^^ ERROR cannot determine a type for this bounded type parameter: unconstrained type
}
