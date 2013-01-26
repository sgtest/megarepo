// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[legacy_modes];
#[legacy_exports];

use dvec::DVec;

struct Entry<A,B> {key: A, value: B}

struct alist<A,B> { eq_fn: fn@(A,A) -> bool, data: DVec<Entry<A,B>> }

fn alist_add<A: Copy, B: Copy>(lst: alist<A,B>, k: A, v: B) {
    lst.data.push(Entry{key:k, value:v});
}

fn alist_get<A: Copy, B: Copy>(lst: alist<A,B>, k: A) -> B {
    let eq_fn = lst.eq_fn;
    for lst.data.each |entry| {
        if eq_fn(entry.key, k) { return entry.value; }
    }
    fail;
}

#[inline]
fn new_int_alist<B: Copy>() -> alist<int, B> {
    fn eq_int(&&a: int, &&b: int) -> bool { a == b }
    return alist {eq_fn: eq_int, data: DVec()};
}

#[inline]
fn new_int_alist_2<B: Copy>() -> alist<int, B> {
    #[inline]
    fn eq_int(&&a: int, &&b: int) -> bool { a == b }
    return alist {eq_fn: eq_int, data: DVec()};
}
