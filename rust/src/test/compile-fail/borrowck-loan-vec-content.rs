// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Here we check that it is allowed to lend out an element of a
// (locally rooted) mutable, unique vector, and that we then prevent
// modifications to the contents.

fn takes_imm_elt(_v: &int, f: fn()) {
    f();
}

fn has_mut_vec_and_does_not_try_to_change_it() {
    let v = ~[mut 1, 2, 3];
    do takes_imm_elt(&v[0]) {
    }
}

fn has_mut_vec_but_tries_to_change_it() {
    let v = ~[mut 1, 2, 3];
    do takes_imm_elt(&v[0]) { //~ NOTE loan of mutable vec content granted here
        v[1] = 4; //~ ERROR assigning to mutable vec content prohibited due to outstanding loan
    }
}

fn takes_const_elt(_v: &const int, f: fn()) {
    f();
}

fn has_mut_vec_and_tries_to_change_it() {
    let v = ~[mut 1, 2, 3];
    do takes_const_elt(&const v[0]) {
        v[1] = 4;
    }
}

fn main() {
}