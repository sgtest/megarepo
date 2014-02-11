// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// ignore-fast compile-flags doesn't work with fast-check
// compile-flags: --cfg foo --cfg bar(baz) --cfg qux="foo"

pub fn main() {
    // check
    if ! cfg!(foo) { fail!() }
    if   cfg!(not(foo)) { fail!() }

    if ! cfg!(bar(baz)) { fail!() }
    if   cfg!(not(bar(baz))) { fail!() }

    if ! cfg!(qux="foo") { fail!() }
    if   cfg!(not(qux="foo")) { fail!() }

    if ! cfg!(foo, bar(baz), qux="foo") { fail!() }
    if   cfg!(not(foo, bar(baz), qux="foo")) { fail!() }

    if cfg!(not_a_cfg) { fail!() }
    if cfg!(not_a_cfg, foo, bar(baz), qux="foo") { fail!() }

    if ! cfg!(not(not_a_cfg)) { fail!() }
    if ! cfg!(not(not_a_cfg), foo, bar(baz), qux="foo") { fail!() }

    if cfg!(trailing_comma, ) { fail!() }
}
