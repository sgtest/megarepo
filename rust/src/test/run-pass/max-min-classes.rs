// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

trait Product {
    fn product() -> int;
}

struct Foo {
    x: int,
    y: int,
}

impl Foo {
    fn sum() -> int {
        self.x + self.y
    }
}

impl Foo : Product {
    fn product() -> int {
        self.x * self.y
    }
}

fn Foo(x: int, y: int) -> Foo {
    Foo { x: x, y: y }
}

fn main() {
    let foo = Foo(3, 20);
    io::println(fmt!("%d %d", foo.sum(), foo.product()));
}

