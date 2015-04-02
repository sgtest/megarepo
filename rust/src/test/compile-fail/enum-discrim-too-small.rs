// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(negate_unsigned)]

#[repr(u8)] //~ NOTE discriminant type specified here
enum Eu8 {
    Au8 = 23,
    Bu8 = 223,
    Cu8 = -23, //~ ERROR discriminant value outside specified type
}

#[repr(i8)] //~ NOTE discriminant type specified here
enum Ei8 {
    Ai8 = 23,
    Bi8 = -23,
    Ci8 = 223, //~ ERROR discriminant value outside specified type
}

#[repr(u16)] //~ NOTE discriminant type specified here
enum Eu16 {
    Au16 = 23,
    Bu16 = 55555,
    Cu16 = -22333, //~ ERROR discriminant value outside specified type
}

#[repr(i16)] //~ NOTE discriminant type specified here
enum Ei16 {
    Ai16 = 23,
    Bi16 = -22333,
    Ci16 = 55555, //~ ERROR discriminant value outside specified type
}

#[repr(u32)] //~ NOTE discriminant type specified here
enum Eu32 {
    Au32 = 23,
    Bu32 = 3_000_000_000,
    Cu32 = -2_000_000_000, //~ ERROR discriminant value outside specified type
}

#[repr(i32)] //~ NOTE discriminant type specified here
enum Ei32 {
    Ai32 = 23,
    Bi32 = -2_000_000_000,
    Ci32 = 3_000_000_000, //~ ERROR discriminant value outside specified type
}

// u64 currently allows negative numbers, and i64 allows numbers greater than `1<<63`.  This is a
// little counterintuitive, but since the discriminant can store all the bits, and extracting it
// with a cast requires specifying the signedness, there is no loss of information in those cases.
// This also applies to isize and usize on 64-bit targets.

pub fn main() { }
