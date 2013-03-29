// xfail-fast

// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

extern mod std;

use std::list::*;

fn pure_length_go<T:Copy>(ls: @List<T>, acc: uint) -> uint {
    match *ls { Nil => { acc } Cons(_, tl) => { pure_length_go(tl, acc + 1u) } }
}

fn pure_length<T:Copy>(ls: @List<T>) -> uint { pure_length_go(ls, 0u) }

fn nonempty_list<T:Copy>(ls: @List<T>) -> bool { pure_length(ls) > 0u }

fn safe_head<T:Copy>(ls: @List<T>) -> T {
    assert!(!is_empty(ls));
    return head(ls);
}

pub fn main() {
    let mylist = @Cons(@1u, @Nil);
    assert!((nonempty_list(mylist)));
    assert!((*safe_head(mylist) == 1u));
}
