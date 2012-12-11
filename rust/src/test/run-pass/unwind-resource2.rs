// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// xfail-win32
extern mod std;

struct complainer {
  c: @int,
}

impl complainer : Drop {
    fn finalize(&self) {}
}

fn complainer(c: @int) -> complainer {
    complainer {
        c: c
    }
}

fn f() {
    let c = move complainer(@0);
    fail;
}

fn main() {
    task::spawn_unlinked(f);
}
