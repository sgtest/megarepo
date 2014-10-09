// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// ignore-tidy-linelength
// ignore-android: FIXME(#10381)

// compile-flags:-g

// === GDB TESTS ===================================================================================

// gdb-command:rbreak zzz
// gdb-command:run
// gdb-command:finish

// gdb-command:print *the_a
// gdb-check:$1 = {{RUST$ENUM$DISR = TheA, x = 0, y = 8970181431921507452}, {RUST$ENUM$DISR = TheA, 0, 2088533116, 2088533116}}

// gdb-command:print *the_b
// gdb-check:$2 = {{RUST$ENUM$DISR = TheB, x = 0, y = 1229782938247303441}, {RUST$ENUM$DISR = TheB, 0, 286331153, 286331153}}

// gdb-command:print *univariant
// gdb-check:$3 = {{123234}}


// === LLDB TESTS ==================================================================================

// lldb-command:run

// lldb-command:print *the_a
// lldb-check:[...]$0 = TheA { x: 0, y: 8970181431921507452 }

// lldb-command:print *the_b
// lldb-check:[...]$1 = TheB(0, 286331153, 286331153)

// lldb-command:print *univariant
// lldb-check:[...]$2 = TheOnlyCase(123234)

#![allow(unused_variable)]
#![feature(struct_variant)]

// The first element is to ensure proper alignment, irrespective of the machines word size. Since
// the size of the discriminant value is machine dependent, this has be taken into account when
// datatype layout should be predictable as in this case.
enum ABC {
    TheA { x: i64, y: i64 },
    TheB (i64, i32, i32),
}

// This is a special case since it does not have the implicit discriminant field.
enum Univariant {
    TheOnlyCase(i64)
}

fn main() {

    // In order to avoid endianess trouble all of the following test values consist of a single
    // repeated byte. This way each interpretation of the union should look the same, no matter if
    // this is a big or little endian machine.

    // 0b0111110001111100011111000111110001111100011111000111110001111100 = 8970181431921507452
    // 0b01111100011111000111110001111100 = 2088533116
    // 0b0111110001111100 = 31868
    // 0b01111100 = 124
    let the_a = box TheA { x: 0, y: 8970181431921507452 };

    // 0b0001000100010001000100010001000100010001000100010001000100010001 = 1229782938247303441
    // 0b00010001000100010001000100010001 = 286331153
    // 0b0001000100010001 = 4369
    // 0b00010001 = 17
    let the_b = box TheB (0, 286331153, 286331153);

    let univariant = box TheOnlyCase(123234);

    zzz(); // #break
}

fn zzz() {()}
