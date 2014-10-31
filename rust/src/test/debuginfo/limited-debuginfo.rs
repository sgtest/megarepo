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

// ignore-lldb

// compile-flags:--debuginfo=1

// Make sure functions have proper names
// gdb-command:info functions
// gdb-check:[...]void[...]main([...]);
// gdb-check:[...]void[...]some_function([...]);
// gdb-check:[...]void[...]some_other_function([...]);
// gdb-check:[...]void[...]zzz([...]);

// gdb-command:rbreak zzz
// gdb-command:run

// Make sure there is no information about locals
// gdb-command:finish
// gdb-command:info locals
// gdb-check:No locals.
// gdb-command:continue


#![allow(unused_variables)]

struct Struct {
    a: i64,
    b: i32
}

fn main() {
    some_function(101, 202);
    some_other_function(1, 2);
}


fn zzz() {()}

fn some_function(a: int, b: int) {
    let some_variable = Struct { a: 11, b: 22 };
    let some_other_variable = 23i;
    zzz();
}

fn some_other_function(a: int, b: int) -> bool { true }
