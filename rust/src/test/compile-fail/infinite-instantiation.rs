// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// error-pattern: overly deep expansion
// issue 2258

trait to_opt {
    fn to_option() -> Option<Self>;
}

impl to_opt for uint {
    fn to_option() -> Option<uint> {
        Some(self)
    }
}

impl<T:Copy> to_opt for Option<T> {
    fn to_option() -> Option<Option<T>> {
        Some(self)
    }
}

fn function<T:to_opt>(counter: uint, t: T) {
    if counter > 0u {
        function(counter - 1u, t.to_option());
    }
}

fn main() {
    function(22u, 22u);
}
