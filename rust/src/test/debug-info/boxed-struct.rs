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
// debugger:rbreak zzz
// debugger:run
// debugger:finish

// debugger:print *unique
// check:$1 = {x = 99, y = 999, z = 9999, w = 99999}

// debugger:print managed->val
// check:$2 = {x = 88, y = 888, z = 8888, w = 88888}

// debugger:print *unique_dtor
// check:$3 = {x = 77, y = 777, z = 7777, w = 77777}

// debugger:print managed_dtor->val
// check:$4 = {x = 33, y = 333, z = 3333, w = 33333}

#[feature(managed_boxes)];
#[allow(unused_variable)];

struct StructWithSomePadding {
    x: i16,
    y: i32,
    z: i32,
    w: i64
}

struct StructWithDestructor {
    x: i16,
    y: i32,
    z: i32,
    w: i64
}

impl Drop for StructWithDestructor {
    fn drop(&mut self) {}
}

fn main() {

    let unique = ~StructWithSomePadding { x: 99, y: 999, z: 9999, w: 99999 };
    let managed = @StructWithSomePadding { x: 88, y: 888, z: 8888, w: 88888 };

    let unique_dtor = ~StructWithDestructor { x: 77, y: 777, z: 7777, w: 77777 };
    let managed_dtor = @StructWithDestructor { x: 33, y: 333, z: 3333, w: 33333 };

    zzz();
}

fn zzz() {()}
