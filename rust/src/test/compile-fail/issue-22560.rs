// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// ignore-tidy-linelength

use std::ops::{Add, Sub};

type Test = Add +
            //~^ ERROR the type parameter `RHS` must be explicitly specified in an object type because its default value `Self` references the type `Self`
            //~^^ ERROR the value of the associated type `Output` (from the trait `core::ops::Add`) must be specified [E0191]
            Sub;
            //~^ ERROR only the builtin traits can be used as closure or object bounds

fn main() { }
