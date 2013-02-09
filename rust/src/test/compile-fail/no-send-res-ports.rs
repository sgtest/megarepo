// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

struct Port<T>(@T);

fn main() {
    struct foo {
      _x: Port<()>,
    }

    impl foo : Drop {
        fn finalize(&self) {}
    }

    fn foo(x: Port<()>) -> foo {
        foo {
            _x: x
        }
    }

    let x = ~mut Some(foo(Port(@())));

    do task::spawn {
        let mut y = None;
        *x <-> y; //~ ERROR value has non-owned type
        log(error, y);
    }
}
