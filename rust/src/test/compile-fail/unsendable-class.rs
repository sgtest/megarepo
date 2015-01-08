// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::sync::mpsc::channel;

// Test that a class with an unsendable field can't be
// sent

use std::rc::Rc;

struct foo {
  i: isize,
  j: Rc<String>,
}

fn foo(i:isize, j: Rc<String>) -> foo {
    foo {
        i: i,
        j: j
    }
}

fn main() {
  let cat = "kitty".to_string();
  let (tx, _) = channel();
  //~^ ERROR `core::marker::Send` is not implemented
  //~^^ ERROR `core::marker::Send` is not implemented
  tx.send(foo(42, Rc::new(cat)));
}
