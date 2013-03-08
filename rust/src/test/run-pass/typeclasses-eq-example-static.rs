// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Example from lkuper's intern talk, August 2012 -- now with static
// methods!

trait Equal {
    static fn isEq(a: Self, b: Self) -> bool;
}

enum Color { cyan, magenta, yellow, black }

impl Equal for Color {
    static fn isEq(a: Color, b: Color) -> bool {
        match (a, b) {
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
    static fn isEq(a: ColorTree, b: ColorTree) -> bool {
        match (a, b) {
          (leaf(x), leaf(y)) => { Equal::isEq(x, y) }
          (branch(l1, r1), branch(l2, r2)) => { 
            Equal::isEq(*l1, *l2) && Equal::isEq(*r1, *r2)
          }
          _ => { false }
        }
    }
}

pub fn main() {
    fail_unless!(Equal::isEq(cyan, cyan));
    fail_unless!(Equal::isEq(magenta, magenta));
    fail_unless!(!Equal::isEq(cyan, yellow));
    fail_unless!(!Equal::isEq(magenta, cyan));

    fail_unless!(Equal::isEq(leaf(cyan), leaf(cyan)));
    fail_unless!(!Equal::isEq(leaf(cyan), leaf(yellow)));

    fail_unless!(Equal::isEq(branch(@leaf(magenta), @leaf(cyan)),
                branch(@leaf(magenta), @leaf(cyan))));

    fail_unless!(!Equal::isEq(branch(@leaf(magenta), @leaf(cyan)),
                 branch(@leaf(magenta), @leaf(magenta))));

    log(error, "Assertions all succeeded!");
}
