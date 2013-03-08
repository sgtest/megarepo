// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Testing shifts for various combinations of integers
// Issue #1570

pub fn main() {
    test_misc();
    test_expr();
    test_const();
}

fn test_misc() {
    fail_unless!(1 << 1i8 << 1u8 << 1i16 << 1 as char << 1u64 == 32);
}

fn test_expr() {
    let v10 = 10 as uint;
    let v4 = 4 as u8;
    let v2 = 2 as u8;
    fail_unless!((v10 >> v2 == v2 as uint));
    fail_unless!((v10 << v4 == 160 as uint));

    let v10 = 10 as u8;
    let v4 = 4 as uint;
    let v2 = 2 as uint;
    fail_unless!((v10 >> v2 == v2 as u8));
    fail_unless!((v10 << v4 == 160 as u8));

    let v10 = 10 as int;
    let v4 = 4 as i8;
    let v2 = 2 as i8;
    fail_unless!((v10 >> v2 == v2 as int));
    fail_unless!((v10 << v4 == 160 as int));

    let v10 = 10 as i8;
    let v4 = 4 as int;
    let v2 = 2 as int;
    fail_unless!((v10 >> v2 == v2 as i8));
    fail_unless!((v10 << v4 == 160 as i8));

    let v10 = 10 as uint;
    let v4 = 4 as int;
    let v2 = 2 as int;
    fail_unless!((v10 >> v2 == v2 as uint));
    fail_unless!((v10 << v4 == 160 as uint));
}

fn test_const() {
    const r1_1: uint = 10u >> 2u8;
    const r2_1: uint = 10u << 4u8;
    fail_unless!(r1_1 == 2 as uint);
    fail_unless!(r2_1 == 160 as uint);

    const r1_2: u8 = 10u8 >> 2u;
    const r2_2: u8 = 10u8 << 4u;
    fail_unless!(r1_2 == 2 as u8);
    fail_unless!(r2_2 == 160 as u8);

    const r1_3: int = 10 >> 2i8;
    const r2_3: int = 10 << 4i8;
    fail_unless!(r1_3 == 2 as int);
    fail_unless!(r2_3 == 160 as int);

    const r1_4: i8 = 10i8 >> 2;
    const r2_4: i8 = 10i8 << 4;
    fail_unless!(r1_4 == 2 as i8);
    fail_unless!(r2_4 == 160 as i8);

    const r1_5: uint = 10u >> 2i8;
    const r2_5: uint = 10u << 4i8;
    fail_unless!(r1_5 == 2 as uint);
    fail_unless!(r2_5 == 160 as uint);
}
