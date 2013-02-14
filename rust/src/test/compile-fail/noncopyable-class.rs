// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Test that a class with a non-copyable field can't be
// copied
struct bar {
  x: int,
}

impl Drop for bar {
    fn finalize(&self) {}
}

fn bar(x:int) -> bar {
    bar {
        x: x
    }
}

struct foo {
  i: int,
  j: bar,
}

fn foo(i:int) -> foo {
    foo {
        i: i,
        j: bar(5)
    }
}

fn main() {
    let x = move foo(10);
    let _y = copy x; //~ ERROR copying a value of non-copyable type
    log(error, x);
}
