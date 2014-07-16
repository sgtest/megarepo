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

// === GDB TESTS ===================================================================================

// gdb-command:rbreak zzz
// gdb-command:run
// gdb-command:finish

// gdb-command:print variable
// gdb-check:$1 = 1
// gdb-command:print constant
// gdb-check:$2 = 2
// gdb-command:print a_struct
// gdb-check:$3 = {a = -3, b = 4.5, c = 5}
// gdb-command:print *struct_ref
// gdb-check:$4 = {a = -3, b = 4.5, c = 5}
// gdb-command:print *owned
// gdb-check:$5 = 6
// gdb-command:print managed->val
// gdb-check:$6 = 7
// gdb-command:print closure_local
// gdb-check:$7 = 8
// gdb-command:continue

// gdb-command:finish
// gdb-command:print variable
// gdb-check:$8 = 1
// gdb-command:print constant
// gdb-check:$9 = 2
// gdb-command:print a_struct
// gdb-check:$10 = {a = -3, b = 4.5, c = 5}
// gdb-command:print *struct_ref
// gdb-check:$11 = {a = -3, b = 4.5, c = 5}
// gdb-command:print *owned
// gdb-check:$12 = 6
// gdb-command:print managed->val
// gdb-check:$13 = 7
// gdb-command:print closure_local
// gdb-check:$14 = 8
// gdb-command:continue


// === LLDB TESTS ==================================================================================

// lldb-command:run

// lldb-command:print variable
// lldb-check:[...]$0 = 1
// lldb-command:print constant
// lldb-check:[...]$1 = 2
// lldb-command:print a_struct
// lldb-check:[...]$2 = Struct { a: -3, b: 4.5, c: 5 }
// lldb-command:print *struct_ref
// lldb-check:[...]$3 = Struct { a: -3, b: 4.5, c: 5 }
// lldb-command:print *owned
// lldb-check:[...]$4 = 6
// lldb-command:print managed->val
// lldb-check:[...]$5 = 7
// lldb-command:print closure_local
// lldb-check:[...]$6 = 8
// lldb-command:continue

// lldb-command:print variable
// lldb-check:[...]$7 = 1
// lldb-command:print constant
// lldb-check:[...]$8 = 2
// lldb-command:print a_struct
// lldb-check:[...]$9 = Struct { a: -3, b: 4.5, c: 5 }
// lldb-command:print *struct_ref
// lldb-check:[...]$10 = Struct { a: -3, b: 4.5, c: 5 }
// lldb-command:print *owned
// lldb-check:[...]$11 = 6
// lldb-command:print managed->val
// lldb-check:[...]$12 = 7
// lldb-command:print closure_local
// lldb-check:[...]$13 = 8
// lldb-command:continue

#![feature(managed_boxes)]
#![allow(unused_variable)]

use std::gc::GC;

struct Struct {
    a: int,
    b: f64,
    c: uint
}

fn main() {
    let mut variable = 1;
    let constant = 2;

    let a_struct = Struct {
        a: -3,
        b: 4.5,
        c: 5
    };

    let struct_ref = &a_struct;
    let owned = box 6;
    let managed = box(GC) 7;

    let closure = || {
        let closure_local = 8;

        let nested_closure = || {
            zzz(); // #break
            variable = constant + a_struct.a + struct_ref.a + *owned + *managed + closure_local;
        };

        zzz(); // #break

        nested_closure();
    };

    closure();
}

fn zzz() {()}
