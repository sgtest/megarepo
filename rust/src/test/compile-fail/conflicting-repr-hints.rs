// Copyright 2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(dead_code)]
#![feature(attr_literals)]
#![feature(repr_align)]

#[repr(C)]
enum A { A }

#[repr(u64)]
enum B { B }

#[repr(C, u64)] //~ WARNING conflicting representation hints
enum C { C }

#[repr(u32, u64)] //~ WARNING conflicting representation hints
enum D { D }

#[repr(C, packed)]
struct E(i32);

#[repr(packed, align(8))]
struct F(i32); //~ ERROR type has conflicting packed and align representation hints

#[repr(packed)]
#[repr(align(8))]
struct G(i32); //~ ERROR type has conflicting packed and align representation hints

#[repr(align(8))]
#[repr(packed)]
struct H(i32); //~ ERROR type has conflicting packed and align representation hints

#[repr(packed, align(8))]
union X { //~ ERROR type has conflicting packed and align representation hints
    i: i32
}

#[repr(packed)]
#[repr(align(8))]
union Y { //~ ERROR type has conflicting packed and align representation hints
    i: i32
}

#[repr(align(8))]
#[repr(packed)]
union Z { //~ ERROR type has conflicting packed and align representation hints
    i: i32
}

fn main() {}
