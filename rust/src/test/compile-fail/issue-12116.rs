// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(box_syntax)]

enum IntList {
    Cons(int, Box<IntList>),
    Nil
}

fn tail(source_list: &IntList) -> IntList {
    match source_list {
        &IntList::Cons(val, box ref next_list) => tail(next_list),
        &IntList::Cons(val, box Nil)           => IntList::Cons(val, box Nil),
//~^ ERROR unreachable pattern
//~^^ WARN pattern binding `Nil` is named the same as one of the variants of the type `IntList`
        _                          => panic!()
    }
}

fn main() {}
