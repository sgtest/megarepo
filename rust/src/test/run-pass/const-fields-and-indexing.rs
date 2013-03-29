// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

static x : [int, ..4] = [1,2,3,4];
static p : int = x[2];
static y : &'static [int] = &[1,2,3,4];
static q : int = y[2];

struct S {a: int, b: int}

static s : S = S {a: 10, b: 20};
static t : int = s.b;

struct K {a: int, b: int, c: D}
struct D { d: int, e: int }

static k : K = K {a: 10, b: 20, c: D {d: 30, e: 40}};
static m : int = k.c.e;

pub fn main() {
    io::println(fmt!("%?", p));
    io::println(fmt!("%?", q));
    io::println(fmt!("%?", t));
    assert!(p == 3);
    assert!(q == 3);
    assert!(t == 20);
}
