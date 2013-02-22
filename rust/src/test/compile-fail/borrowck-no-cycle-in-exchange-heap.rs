// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

struct node_ {
    mut a: ~cycle
}

enum cycle {
    node(node_),
    empty
}
fn main() {
    let x = ~node(node_ {mut a: ~empty});
    // Create a cycle!
    match *x { //~ NOTE loan of immutable local variable granted here
      node(ref y) => {
        y.a = x; //~ ERROR moving out of immutable local variable prohibited due to outstanding loan
      }
      empty => {}
    };
}
