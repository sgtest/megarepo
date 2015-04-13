// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Need a fix for LLDB first...
// ignore-lldb
// ignore-tidy-linelength

// compile-flags:-g
// gdb-command:run

// gdb-command:print/d vi8x16
// gdb-check:$1 = {__0 = 0, __1 = 1, __2 = 2, __3 = 3, __4 = 4, __5 = 5, __6 = 6, __7 = 7, __8 = 8, __9 = 9, __10 = 10, __11 = 11, __12 = 12, __13 = 13, __14 = 14, __15 = 15}
// gdb-command:print/d vi16x8
// gdb-check:$2 = {__0 = 16, __1 = 17, __2 = 18, __3 = 19, __4 = 20, __5 = 21, __6 = 22, __7 = 23}
// gdb-command:print/d vi32x4
// gdb-check:$3 = {__0 = 24, __1 = 25, __2 = 26, __3 = 27}
// gdb-command:print/d vi64x2
// gdb-check:$4 = {__0 = 28, __1 = 29}

// gdb-command:print/d vu8x16
// gdb-check:$5 = {__0 = 30, __1 = 31, __2 = 32, __3 = 33, __4 = 34, __5 = 35, __6 = 36, __7 = 37, __8 = 38, __9 = 39, __10 = 40, __11 = 41, __12 = 42, __13 = 43, __14 = 44, __15 = 45}
// gdb-command:print/d vu16x8
// gdb-check:$6 = {__0 = 46, __1 = 47, __2 = 48, __3 = 49, __4 = 50, __5 = 51, __6 = 52, __7 = 53}
// gdb-command:print/d vu32x4
// gdb-check:$7 = {__0 = 54, __1 = 55, __2 = 56, __3 = 57}
// gdb-command:print/d vu64x2
// gdb-check:$8 = {__0 = 58, __1 = 59}

// gdb-command:print vf32x4
// gdb-check:$9 = {__0 = 60.5, __1 = 61.5, __2 = 62.5, __3 = 63.5}
// gdb-command:print vf64x2
// gdb-check:$10 = {__0 = 64.5, __1 = 65.5}

// gdb-command:continue

#![allow(unused_variables)]
#![omit_gdb_pretty_printer_section]
#![feature(core)]

use std::simd::{i8x16, i16x8,i32x4,i64x2,u8x16,u16x8,u32x4,u64x2,f32x4,f64x2};

fn main() {

    let vi8x16 = i8x16(0, 1, 2, 3, 4, 5, 6, 7,
                      8, 9, 10, 11, 12, 13, 14, 15);

    let vi16x8 = i16x8(16, 17, 18, 19, 20, 21, 22, 23);
    let vi32x4 = i32x4(24, 25, 26, 27);
    let vi64x2 = i64x2(28, 29);

    let vu8x16 = u8x16(30, 31, 32, 33, 34, 35, 36, 37,
                      38, 39, 40, 41, 42, 43, 44, 45);
    let vu16x8 = u16x8(46, 47, 48, 49, 50, 51, 52, 53);
    let vu32x4 = u32x4(54, 55, 56, 57);
    let vu64x2 = u64x2(58, 59);

    let vf32x4 = f32x4(60.5f32, 61.5f32, 62.5f32, 63.5f32);
    let vf64x2 = f64x2(64.5f64, 65.5f64);

    zzz(); // #break
}

#[inline(never)]
fn zzz() { () }
