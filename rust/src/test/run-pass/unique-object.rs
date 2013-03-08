// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

trait Foo {
    fn f(&self) -> int;
}

struct Bar {
    x: int
}

impl Foo for Bar {
    fn f(&self) -> int {
        self.x
    }
}

pub fn main() {
    let x = ~Bar { x: 10 };
    let y = x as ~Foo;
    fail_unless!(y.f() == 10);
}

