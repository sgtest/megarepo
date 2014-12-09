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
// min-lldb-version: 310

// compile-flags:-g

// === GDB TESTS ===================================================================================

// gdb-command:print 'c-style-enum::SINGLE_VARIANT'
// gdb-check:$1 = TheOnlyVariant

// gdb-command:print 'c-style-enum::AUTO_ONE'
// gdb-check:$2 = One

// gdb-command:print 'c-style-enum::AUTO_TWO'
// gdb-check:$3 = One

// gdb-command:print 'c-style-enum::AUTO_THREE'
// gdb-check:$4 = One

// gdb-command:print 'c-style-enum::MANUAL_ONE'
// gdb-check:$5 = OneHundred

// gdb-command:print 'c-style-enum::MANUAL_TWO'
// gdb-check:$6 = OneHundred

// gdb-command:print 'c-style-enum::MANUAL_THREE'
// gdb-check:$7 = OneHundred

// gdb-command:run

// gdb-command:print auto_one
// gdb-check:$8 = One

// gdb-command:print auto_two
// gdb-check:$9 = Two

// gdb-command:print auto_three
// gdb-check:$10 = Three

// gdb-command:print manual_one_hundred
// gdb-check:$11 = OneHundred

// gdb-command:print manual_one_thousand
// gdb-check:$12 = OneThousand

// gdb-command:print manual_one_million
// gdb-check:$13 = OneMillion

// gdb-command:print single_variant
// gdb-check:$14 = TheOnlyVariant

// gdb-command:print 'c-style-enum::AUTO_TWO'
// gdb-check:$15 = Two

// gdb-command:print 'c-style-enum::AUTO_THREE'
// gdb-check:$16 = Three

// gdb-command:print 'c-style-enum::MANUAL_TWO'
// gdb-check:$17 = OneThousand

// gdb-command:print 'c-style-enum::MANUAL_THREE'
// gdb-check:$18 = OneMillion


// === LLDB TESTS ==================================================================================

// lldb-command:run

// lldb-command:print auto_one
// lldb-check:[...]$0 = One

// lldb-command:print auto_two
// lldb-check:[...]$1 = Two

// lldb-command:print auto_three
// lldb-check:[...]$2 = Three

// lldb-command:print manual_one_hundred
// lldb-check:[...]$3 = OneHundred

// lldb-command:print manual_one_thousand
// lldb-check:[...]$4 = OneThousand

// lldb-command:print manual_one_million
// lldb-check:[...]$5 = OneMillion

// lldb-command:print single_variant
// lldb-check:[...]$6 = TheOnlyVariant

#![allow(unused_variables)]
#![allow(dead_code)]

use self::AutoDiscriminant::{One, Two, Three};
use self::ManualDiscriminant::{OneHundred, OneThousand, OneMillion};
use self::SingleVariant::TheOnlyVariant;

#[deriving(Copy)]
enum AutoDiscriminant {
    One,
    Two,
    Three
}

#[deriving(Copy)]
enum ManualDiscriminant {
    OneHundred = 100,
    OneThousand = 1000,
    OneMillion = 1000000
}

#[deriving(Copy)]
enum SingleVariant {
    TheOnlyVariant
}

static SINGLE_VARIANT: SingleVariant = TheOnlyVariant;

static mut AUTO_ONE: AutoDiscriminant = One;
static mut AUTO_TWO: AutoDiscriminant = One;
static mut AUTO_THREE: AutoDiscriminant = One;

static mut MANUAL_ONE: ManualDiscriminant = OneHundred;
static mut MANUAL_TWO: ManualDiscriminant = OneHundred;
static mut MANUAL_THREE: ManualDiscriminant = OneHundred;

fn main() {

    let auto_one = One;
    let auto_two = Two;
    let auto_three = Three;

    let manual_one_hundred = OneHundred;
    let manual_one_thousand = OneThousand;
    let manual_one_million = OneMillion;

    let single_variant = TheOnlyVariant;

    unsafe {
        AUTO_TWO = Two;
        AUTO_THREE = Three;

        MANUAL_TWO = OneThousand;
        MANUAL_THREE = OneMillion;
    };

    zzz(); // #break

    let a = SINGLE_VARIANT;
    let a = unsafe { AUTO_ONE };
    let a = unsafe { MANUAL_ONE };
}

fn zzz() { () }
