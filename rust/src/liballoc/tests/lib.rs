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

#![feature(attr_literals)]
#![feature(box_syntax)]
#![feature(inclusive_range_syntax)]
#![feature(collection_placement)]
#![feature(const_fn)]
#![feature(drain_filter)]
#![feature(exact_size_is_empty)]
#![feature(iterator_step_by)]
#![feature(pattern)]
#![feature(placement_in_syntax)]
#![feature(rand)]
#![feature(repr_align)]
#![feature(slice_rotate)]
#![feature(splice)]
#![feature(str_escape)]
#![feature(string_retain)]
#![feature(unboxed_closures)]
#![feature(unicode)]

extern crate std_unicode;

use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

mod binary_heap;
mod btree;
mod cow_str;
mod fmt;
mod linked_list;
mod slice;
mod str;
mod string;
mod vec_deque;
mod vec;

fn hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}
