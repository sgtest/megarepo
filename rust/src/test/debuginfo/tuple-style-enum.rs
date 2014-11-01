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
// min-lldb-version: 310

// compile-flags:-g

// === GDB TESTS ===================================================================================

// gdb-command:set print union on
// gdb-command:run

// gdb-command:print case1
// gdb-check:$1 = {{RUST$ENUM$DISR = Case1, 0, 31868, 31868, 31868, 31868}, {RUST$ENUM$DISR = Case1, 0, 2088533116, 2088533116}, {RUST$ENUM$DISR = Case1, 0, 8970181431921507452}}

// gdb-command:print case2
// gdb-check:$2 = {{RUST$ENUM$DISR = Case2, 0, 4369, 4369, 4369, 4369}, {RUST$ENUM$DISR = Case2, 0, 286331153, 286331153}, {RUST$ENUM$DISR = Case2, 0, 1229782938247303441}}

// gdb-command:print case3
// gdb-check:$3 = {{RUST$ENUM$DISR = Case3, 0, 22873, 22873, 22873, 22873}, {RUST$ENUM$DISR = Case3, 0, 1499027801, 1499027801}, {RUST$ENUM$DISR = Case3, 0, 6438275382588823897}}

// gdb-command:print univariant
// gdb-check:$4 = {{-1}}


// === LLDB TESTS ==================================================================================

// lldb-command:run

// lldb-command:print case1
// lldb-check:[...]$0 = Case1(0, 31868, 31868, 31868, 31868)

// lldb-command:print case2
// lldb-check:[...]$1 = Case2(0, 286331153, 286331153)

// lldb-command:print case3
// lldb-check:[...]$2 = Case3(0, 6438275382588823897)

// lldb-command:print univariant
// lldb-check:[...]$3 = TheOnlyCase(-1)

#![allow(unused_variables)]

// The first element is to ensure proper alignment, irrespective of the machines word size. Since
// the size of the discriminant value is machine dependent, this has be taken into account when
// datatype layout should be predictable as in this case.
enum Regular {
    Case1(u64, u16, u16, u16, u16),
    Case2(u64, u32, u32),
    Case3(u64, u64)
}

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
    let case1 = Case1(0, 31868, 31868, 31868, 31868);

    // 0b0001000100010001000100010001000100010001000100010001000100010001 = 1229782938247303441
    // 0b00010001000100010001000100010001 = 286331153
    // 0b0001000100010001 = 4369
    // 0b00010001 = 17
    let case2 = Case2(0, 286331153, 286331153);

    // 0b0101100101011001010110010101100101011001010110010101100101011001 = 6438275382588823897
    // 0b01011001010110010101100101011001 = 1499027801
    // 0b0101100101011001 = 22873
    // 0b01011001 = 89
    let case3 = Case3(0, 6438275382588823897);

    let univariant = TheOnlyCase(-1);

    zzz(); // #break
}

fn zzz() {()}
