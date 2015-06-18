// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


#![feature(hash_default)]

use std::hash::{Hash, SipHasher};

#[derive(Hash)]
struct Person {
    id: usize,
    name: String,
    phone: usize,
}

fn hash<T: Hash>(t: &T) -> u64 {
    std::hash::hash::<T, SipHasher>(t)
}

fn main() {
    let person1 = Person {
        id: 5,
        name: "Janet".to_string(),
        phone: 555_666_7777
    };
    let person2 = Person {
        id: 5,
        name: "Bob".to_string(),
        phone: 555_666_7777
    };
    assert_eq!(hash(&person1), hash(&person1));
    assert!(hash(&person1) != hash(&person2));
}
