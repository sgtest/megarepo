// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

struct cat {
  name: ~str,
}

impl Drop for cat {
    #[cat_dropper]
    /**
       Actually, cats don't always land on their feet when you drop them.
    */
    fn drop(&mut self) {
        error!("%s landed on hir feet", self.name);
    }
}

#[cat_maker]
/**
Maybe it should technically be a kitten_maker.
*/
fn cat(name: ~str) -> cat {
    cat {
        name: name
    }
}

pub fn main() {
  let _kitty = cat(~"Spotty");
}
