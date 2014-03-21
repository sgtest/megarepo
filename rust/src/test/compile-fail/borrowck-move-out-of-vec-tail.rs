// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Test that we do not permit moves from &[] matched by a vec pattern.

#[deriving(Clone)]
struct Foo {
    string: ~str
}

pub fn main() {
    let x = vec!(
        Foo { string: ~"foo" },
        Foo { string: ~"bar" },
        Foo { string: ~"baz" }
    );
    let x: &[Foo] = x.as_slice();
    match x {
        [_, ..tail] => {
            match tail {
                [Foo { string: a }, Foo { string: b }] => {
                    //~^ ERROR cannot move out of dereference of `&`-pointer
                    //~^^ ERROR cannot move out of dereference of `&`-pointer
                }
                _ => {
                    unreachable!();
                }
            }
            let z = tail[0].clone();
            println!("{:?}", z);
        }
        _ => {
            unreachable!();
        }
    }
}
