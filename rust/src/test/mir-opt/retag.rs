// Copyright 2017 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// ignore-tidy-linelength
// compile-flags: -Z mir-emit-retag -Z mir-opt-level=0 -Z span_free_formats

#![allow(unused)]

struct Test(i32);

impl Test {
    // Make sure we run the pass on a method, not just on bare functions.
    fn foo<'x>(&self, x: &'x mut i32) -> &'x mut i32 { x }
    fn foo_shr<'x>(&self, x: &'x i32) -> &'x i32 { x }
}

fn main() {
    let mut x = 0;
    {
        let v = Test(0).foo(&mut x); // just making sure we do not panic when there is a tuple struct ctor
        let w = { v }; // assignment
        let _w = w; // reborrow
    }

    // Also test closures
    let c: fn(&i32) -> &i32 = |x: &i32| -> &i32 { let _y = x; x };
    let _w = c(&x);

    // need to call `foo_shr` or it doesn't even get generated
    Test(0).foo_shr(&0);
}

// END RUST SOURCE
// START rustc.{{impl}}-foo.EraseRegions.after.mir
//     bb0: {
//         Retag([fn entry] _1);
//         Retag([fn entry] _2);
//         ...
//         _0 = &mut (*_3);
//         ...
//         return;
//     }
// END rustc.{{impl}}-foo.EraseRegions.after.mir
// START rustc.{{impl}}-foo_shr.EraseRegions.after.mir
//     bb0: {
//         Retag([fn entry] _1);
//         Retag([fn entry] _2);
//         ...
//         _0 = _2;
//         Retag(_0);
//         ...
//         return;
//     }
// END rustc.{{impl}}-foo_shr.EraseRegions.after.mir
// START rustc.main.EraseRegions.after.mir
// fn main() -> () {
//     ...
//     bb0: {
//         ...
//         _3 = const Test::foo(move _4, move _6) -> bb1;
//     }
//
//     bb1: {
//         Retag(_3);
//         ...
//         _9 = move _3;
//         Retag(_9);
//         _8 = &mut (*_9);
//         StorageDead(_9);
//         StorageLive(_10);
//         _10 = move _8;
//         Retag(_10);
//         ...
//         _13 = move _14(move _15) -> bb2;
//     }
//
//     bb2: {
//         Retag(_13);
//         ...
//     }
//     ...
// }
// END rustc.main.EraseRegions.after.mir
// START rustc.main-{{closure}}.EraseRegions.after.mir
// fn main::{{closure}}(_1: &[closure@NodeId(117)], _2: &i32) -> &i32 {
//     ...
//     bb0: {
//         Retag([fn entry] _1);
//         Retag([fn entry] _2);
//         StorageLive(_3);
//         _3 = _2;
//         Retag(_3);
//         _0 = _2;
//         Retag(_0);
//         StorageDead(_3);
//         return;
//     }
// }
// END rustc.main-{{closure}}.EraseRegions.after.mir
