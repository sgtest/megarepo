// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// ignore-pretty : (#23623) problems when  ending with // comments

// error-pattern:thread 'main' panicked at 'shift operation overflowed'
// compile-flags: -C debug-assertions

#![warn(exceeding_bitshifts)]

#![feature(rustc_attrs)]
#[rustc_no_mir] // FIXME #29769 MIR overflow checking is TBD.
fn main() {
    let _x = 1 << -1;
}
