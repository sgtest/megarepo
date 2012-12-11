// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// xfail-test
type T = { mut f: fn@() };
type S = { f: fn@() };

fn fooS(t: S) {
}

fn fooT(t: T) {
}

fn bar() {
}

fn main() {
    let x: fn@() = bar;
    fooS({f: x});
    fooS({f: bar});

    let x: fn@() = bar;
    fooT({mut f: x});
    fooT({mut f: bar});
}

