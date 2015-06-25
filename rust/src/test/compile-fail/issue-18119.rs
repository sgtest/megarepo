// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

const X: u8 = 1;
static Y: u8 = 1;
fn foo() {}

impl X {}
//~^ ERROR use of undeclared type name `X`
impl Y {}
//~^ ERROR use of undeclared type name `Y`
impl foo {}
//~^ ERROR use of undeclared type name `foo`

fn main() {}
