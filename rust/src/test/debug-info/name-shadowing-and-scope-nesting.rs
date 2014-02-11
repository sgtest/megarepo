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
// debugger:print x
// check:$1 = false
// debugger:print y
// check:$2 = true
// debugger:continue

// debugger:finish
// debugger:print x
// check:$3 = 10
// debugger:print y
// check:$4 = true
// debugger:continue

// debugger:finish
// debugger:print x
// check:$5 = 10.5
// debugger:print y
// check:$6 = 20
// debugger:continue

// debugger:finish
// debugger:print x
// check:$7 = true
// debugger:print y
// check:$8 = 2220
// debugger:continue

// debugger:finish
// debugger:print x
// check:$9 = 203203.5
// debugger:print y
// check:$10 = 2220
// debugger:continue

// debugger:finish
// debugger:print x
// check:$11 = 10.5
// debugger:print y
// check:$12 = 20
// debugger:continue

fn main() {
    let x = false;
    let y = true;

    zzz();
    sentinel();

    let x = 10;

    zzz();
    sentinel();

    let x = 10.5;
    let y = 20;

    zzz();
    sentinel();

    {
        let x = true;
        let y = 2220;

        zzz();
        sentinel();

        let x = 203203.5;

        zzz();
        sentinel();
    }

    zzz();
    sentinel();
}

fn zzz() {()}
fn sentinel() {()}
