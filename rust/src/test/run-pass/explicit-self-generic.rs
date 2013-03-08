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

/**
 * A function that returns a hash of a value
 *
 * The hash should concentrate entropy in the lower bits.
 */
type HashFn<K> = ~pure fn(K) -> uint;
type EqFn<K> = ~pure fn(K, K) -> bool;

struct LM { resize_at: uint, size: uint }

enum LinearMap<K,V> {
    LinearMap_(LM)
}

fn linear_map<K,V>() -> LinearMap<K,V> {
    LinearMap_(LM{
        resize_at: 32,
        size: 0})
}

pub impl<K,V> LinearMap<K,V> {
    fn len(&mut self) -> uint {
        self.size
    }
}

pub fn main() {
    let mut m = ~linear_map::<(),()>();
    fail_unless!(m.len() == 0);
}

