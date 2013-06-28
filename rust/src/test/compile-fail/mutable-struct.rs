// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[mutable]
struct Foo { a: int }

fn bar<T: Freeze>(_: T) {}

fn main() {
    let x = Foo { a: 5 };
    bar(x); //~ ERROR instantiating a type parameter with an incompatible type `Foo`, which does not fulfill `Freeze`
}
