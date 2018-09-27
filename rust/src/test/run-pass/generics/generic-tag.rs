// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// run-pass
#![allow(unused_assignments)]
#![allow(non_camel_case_types)]

// pretty-expanded FIXME #23616

#![allow(unused_variables)]
#![feature(box_syntax)]

enum option<T> { some(Box<T>), none, }

pub fn main() {
    let mut a: option<isize> = option::some::<isize>(box 10);
    a = option::none::<isize>;
}
