// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// pretty-expanded FIXME #23616

#![feature(collections)]

use std::collections::BitVec;

fn main() {
    // Generate sieve of Eratosthenes for n up to 1e6
    let n = 1000000;
    let mut sieve = BitVec::from_elem(n+1, true);
    let limit: usize = (n as f32).sqrt() as usize;
    for i in 2..limit+1 {
        if sieve[i] {
            let mut j = 0;
            while i*i + j*i <= n {
                sieve.set(i*i+j*i, false);
                j += 1;
            }
        }
    }
    for i in 2..n+1 {
        if sieve[i] {
        }
    }
}
