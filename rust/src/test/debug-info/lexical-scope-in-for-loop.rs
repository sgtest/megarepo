// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// ignore-win32
// ignore-android: FIXME(#10381)

// compile-flags:-g
// debugger:rbreak zzz
// debugger:run

// FIRST ITERATION
// debugger:finish
// debugger:print x
// check:$1 = 1
// debugger:continue

// debugger:finish
// debugger:print x
// check:$2 = -1
// debugger:continue

// SECOND ITERATION
// debugger:finish
// debugger:print x
// check:$3 = 2
// debugger:continue

// debugger:finish
// debugger:print x
// check:$4 = -2
// debugger:continue

// THIRD ITERATION
// debugger:finish
// debugger:print x
// check:$5 = 3
// debugger:continue

// debugger:finish
// debugger:print x
// check:$6 = -3
// debugger:continue

// AFTER LOOP
// debugger:finish
// debugger:print x
// check:$7 = 1000000
// debugger:continue

fn main() {

    let range = [1, 2, 3];

    let x = 1000000; // wan meeeljen doollaars!

    for &x in range.iter() {
        zzz();
        sentinel();

        let x = -1 * x;

        zzz();
        sentinel();
    }

    zzz();
    sentinel();
}

fn zzz() {()}
fn sentinel() {()}
