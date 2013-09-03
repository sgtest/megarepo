// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::hashmap::HashMap;

trait Graph<Node, Edge> {
    fn f(&self, Edge);

}

impl<E> Graph<int, E> for HashMap<int, int> {
    fn f(&self, _e: E) {
        fail!();
    }
}

fn main() {
    let g : ~HashMap<int, int> = ~HashMap::new();
    let _g2 : ~Graph<int,int> = g as ~Graph<int,int>;
}
