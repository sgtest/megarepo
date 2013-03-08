// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Example from lkuper's intern talk, August 2012.

trait Equal {
    fn isEq(a: Self) -> bool;
}

enum Color { cyan, magenta, yellow, black }

impl Equal for Color {
    fn isEq(a: Color) -> bool {
        match (self, a) {
          (cyan, cyan)       => { true  }
          (magenta, magenta) => { true  }
          (yellow, yellow)   => { true  }
          (black, black)     => { true  }
          _                  => { false }
        }
    }
}

enum ColorTree {
    leaf(Color),
    branch(@ColorTree, @ColorTree)
}

impl Equal for ColorTree {
    fn isEq(a: ColorTree) -> bool {
        match (self, a) {
          (leaf(x), leaf(y)) => { x.isEq(y) }
          (branch(l1, r1), branch(l2, r2)) => { 
            (*l1).isEq(*l2) && (*r1).isEq(*r2)
          }
          _ => { false }
        }
    }
}

pub fn main() {
    fail_unless!(cyan.isEq(cyan));
    fail_unless!(magenta.isEq(magenta));
    fail_unless!(!cyan.isEq(yellow));
    fail_unless!(!magenta.isEq(cyan));

    fail_unless!(leaf(cyan).isEq(leaf(cyan)));
    fail_unless!(!leaf(cyan).isEq(leaf(yellow)));

    fail_unless!(branch(@leaf(magenta), @leaf(cyan))
        .isEq(branch(@leaf(magenta), @leaf(cyan))));

    fail_unless!(!branch(@leaf(magenta), @leaf(cyan))
        .isEq(branch(@leaf(magenta), @leaf(magenta))));

    error!("Assertions all succeeded!");
}
