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
// gdb-command:rbreak zzz
// gdb-command:run
// gdb-command:finish

// gdb-command:print some
// gdb-check:$1 = {RUST$ENCODED$ENUM$0$None = {0x12345678}}

// gdb-command:print none
// gdb-check:$2 = {RUST$ENCODED$ENUM$0$None = {0x0}}

// gdb-command:print full
// gdb-check:$3 = {RUST$ENCODED$ENUM$1$Empty = {454545, 0x87654321, 9988}}

// gdb-command:print empty->discr
// gdb-check:$4 = (int *) 0x0

// gdb-command:print droid
// gdb-check:$5 = {RUST$ENCODED$ENUM$2$Void = {id = 675675, range = 10000001, internals = 0x43218765}}

// gdb-command:print void_droid->internals
// gdb-check:$6 = (int *) 0x0

// gdb-command:continue

#![feature(struct_variant)]

// If a struct has exactly two variants, one of them is empty, and the other one
// contains a non-nullable pointer, then this value is used as the discriminator.
// The test cases in this file make sure that something readable is generated for
// this kind of types.
// If the non-empty variant contains a single non-nullable pointer than the whole
// item is represented as just a pointer and not wrapped in a struct.
// Unfortunately (for these test cases) the content of the non-discriminant fields
// in the null-case is not defined. So we just read the discriminator field in
// this case (by casting the value to a memory-equivalent struct).

enum MoreFields<'a> {
    Full(u32, &'a int, i16),
    Empty
}

struct MoreFieldsRepr<'a> {
    a: u32,
    discr: &'a int,
    b: i16
}

enum NamedFields<'a> {
    Droid { id: i32, range: i64, internals: &'a int },
    Void
}

struct NamedFieldsRepr<'a> {
    id: i32,
    range: i64,
    internals: &'a int
}

fn main() {

    let some: Option<&u32> = Some(unsafe { std::mem::transmute(0x12345678u) });
    let none: Option<&u32> = None;

    let full = Full(454545, unsafe { std::mem::transmute(0x87654321u) }, 9988);

    let int_val = 0i;
    let empty: &MoreFieldsRepr = unsafe { std::mem::transmute(&Empty) };

    let droid = Droid {
        id: 675675,
        range: 10000001,
        internals: unsafe { std::mem::transmute(0x43218765u) }
    };

    let void_droid: &NamedFieldsRepr = unsafe { std::mem::transmute(&Void) };

    zzz();
}

fn zzz() {()}
