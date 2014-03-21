// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


fn bar(v: &mut [uint]) -> Vec<uint> {
    Vec::from_slice(v)
}

fn bip(v: &[uint]) -> Vec<uint> {
    Vec::from_slice(v)
}

pub fn main() {
    let mut the_vec = vec!(1u, 2, 3, 100);
    assert_eq!(the_vec.clone(), bar(the_vec.as_mut_slice()));
    assert_eq!(the_vec.clone(), bip(the_vec.as_slice()));
}
