// Copyright 2018 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// compile-flags: -O
// min-llvm-version 6.0

#![crate_type = "lib"]

// verify that LLVM recognizes a loop involving 0..=n and will const-fold it.

//------------------------------------------------------------------------------
// Example from original issue #45222

fn foo2(n: u64) -> u64 {
    let mut count = 0;
    for _ in 0..n {
        for j in (0..=n).rev() {
            count += j;
        }
    }
    count
}

// CHECK-LABEL: @check_foo2
#[no_mangle]
pub fn check_foo2() -> u64 {
    // CHECK: ret i64 500005000000000
    foo2(100000)
}

//------------------------------------------------------------------------------
// Simplified example of #45222

fn triangle_inc(n: u64) -> u64 {
    let mut count = 0;
    for j in 0 ..= n {
        count += j;
    }
    count
}

// CHECK-LABEL: @check_triangle_inc
#[no_mangle]
pub fn check_triangle_inc() -> u64 {
    // CHECK: ret i64 5000050000
    triangle_inc(100000)
}

//------------------------------------------------------------------------------
// Demo in #48012

fn foo3r(n: u64) -> u64 {
    let mut count = 0;
    (0..n).for_each(|_| {
        (0 ..= n).rev().for_each(|j| {
            count += j;
        })
    });
    count
}

// CHECK-LABEL: @check_foo3r
#[no_mangle]
pub fn check_foo3r() -> u64 {
    // CHECK: ret i64 500005000000000
    foo3r(100000)
}
