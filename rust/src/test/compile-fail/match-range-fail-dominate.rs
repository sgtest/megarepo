// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//error-pattern: unreachable
//error-pattern: unreachable
//error-pattern: unreachable
//error-pattern: unreachable
//error-pattern: unreachable

fn main() {
    match 5us {
      1us ... 10us => { }
      5us ... 6us => { }
      _ => {}
    };

    match 5us {
      3us ... 6us => { }
      4us ... 6us => { }
      _ => {}
    };

    match 5us {
      4us ... 6us => { }
      4us ... 6us => { }
      _ => {}
    };

    match 'c' {
      'A' ... 'z' => {}
      'a' ... 'z' => {}
      _ => {}
    };

    match 1.0f64 {
      0.01f64 ... 6.5f64 => {}
      0.02f64 => {}
      _ => {}
    };
}
