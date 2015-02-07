// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Test moving array structures, e.g. `[T; 3]` as well as moving
// elements in and out of such arrays.
//
// Note also that the `test_move_array_then_overwrite` tests represent
// cases that we probably should make illegal.

pub struct D { d: isize }
impl Drop for D { fn drop(&mut self) { } }

#[rustc_move_fragments]
pub fn test_move_array_via_return(a: [D; 3]) -> [D; 3] {
    //~^ ERROR                  assigned_leaf_path: `$(local a)`
    //~| ERROR                     moved_leaf_path: `$(local a)`
    return a;
}

#[rustc_move_fragments]
pub fn test_move_array_into_recv(a: [D; 3], recv: &mut [D; 3]) {
    //~^ ERROR                 parent_of_fragments: `$(local recv)`
    //~| ERROR                  assigned_leaf_path: `$(local a)`
    //~| ERROR                     moved_leaf_path: `$(local a)`
    //~| ERROR                  assigned_leaf_path: `$(local recv).*`
    *recv = a;
}

#[rustc_move_fragments]
pub fn test_overwrite_array_elem(mut a: [D; 3], i: usize, d: D) {
    //~^ ERROR                 parent_of_fragments: `$(local mut a)`
    //~| ERROR                  assigned_leaf_path: `$(local i)`
    //~| ERROR                  assigned_leaf_path: `$(local d)`
    //~| ERROR                     moved_leaf_path: `$(local d)`
    //~| ERROR                  assigned_leaf_path: `$(local mut a).[]`
    //~| ERROR                    unmoved_fragment: `$(allbutone $(local mut a).[])`
    a[i] = d;
}

pub fn main() { }
