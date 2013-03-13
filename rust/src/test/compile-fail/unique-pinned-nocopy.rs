// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

struct r {
  b:bool,
}

impl Drop for r {
    fn finalize(&self) {}
}

fn main() {
    let i = ~r { b: true };
    let _j = copy i; //~ ERROR copying a value of non-copyable type
    debug!(i);
}
