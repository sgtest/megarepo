// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![deny(warnings)]

#![feature(ascii)]
#![feature(binary_heap_extras)]
#![feature(box_syntax)]
#![feature(btree_range)]
#![feature(collections)]
#![feature(collections_bound)]
#![feature(copy_from_slice)]
#![feature(const_fn)]
#![feature(fn_traits)]
#![feature(enumset)]
#![feature(iter_arith)]
#![feature(map_entry_keys)]
#![feature(map_values_mut)]
#![feature(pattern)]
#![feature(rand)]
#![feature(set_recovery)]
#![feature(step_by)]
#![feature(str_char)]
#![feature(str_escape)]
#![feature(test)]
#![feature(unboxed_closures)]
#![feature(unicode)]

extern crate collections;
extern crate test;
extern crate rustc_unicode;

use std::hash::{Hash, Hasher, SipHasher};

#[cfg(test)] #[macro_use] mod bench;

mod binary_heap;
mod btree;
mod enum_set;
mod fmt;
mod linked_list;
mod slice;
mod str;
mod string;
mod vec_deque;
mod vec;

fn hash<T: Hash>(t: &T) -> u64 {
    let mut s = SipHasher::new();
    t.hash(&mut s);
    s.finish()
}
