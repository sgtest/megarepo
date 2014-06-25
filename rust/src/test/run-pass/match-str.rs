// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Issue #53

pub fn main() {
    match "test" { "not-test" => fail!(), "test" => (), _ => fail!() }

    enum t { tag1(String), tag2, }


    match tag1("test".to_string()) {
      tag2 => fail!(),
      tag1(ref s) if "test" != s.as_slice() => fail!(),
      tag1(ref s) if "test" == s.as_slice() => (),
      _ => fail!()
    }

    let x = match "a" { "a" => 1i, "b" => 2i, _ => fail!() };
    assert_eq!(x, 1);

    match "a" { "a" => { } "b" => { }, _ => fail!() }

}
