// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// ignore-android: FIXME(#10381)

// compile-flags:-g
// gdb-command:rbreak zzz
// gdb-command:run

// gdb-command:finish
// gdb-command:print *t0
// gdb-check:$1 = 1
// gdb-command:print *t1
// gdb-check:$2 = 2.5
// gdb-command:print ret
// gdb-check:$3 = {{1, 2.5}, {2.5, 1}}
// gdb-command:continue

// gdb-command:finish
// gdb-command:print *t0
// gdb-check:$4 = 3.5
// gdb-command:print *t1
// gdb-check:$5 = 4
// gdb-command:print ret
// gdb-check:$6 = {{3.5, 4}, {4, 3.5}}
// gdb-command:continue

// gdb-command:finish
// gdb-command:print *t0
// gdb-check:$7 = 5
// gdb-command:print *t1
// gdb-check:$8 = {a = 6, b = 7.5}
// gdb-command:print ret
// gdb-check:$9 = {{5, {a = 6, b = 7.5}}, {{a = 6, b = 7.5}, 5}}
// gdb-command:continue

#[deriving(Clone)]
struct Struct {
    a: int,
    b: f64
}

fn dup_tup<T0: Clone, T1: Clone>(t0: &T0, t1: &T1) -> ((T0, T1), (T1, T0)) {
    let ret = ((t0.clone(), t1.clone()), (t1.clone(), t0.clone()));
    zzz();
    ret
}

fn main() {

    let _ = dup_tup(&1i, &2.5f64);
    let _ = dup_tup(&3.5f64, &4_u16);
    let _ = dup_tup(&5i, &Struct { a: 6, b: 7.5 });
}

fn zzz() {()}
