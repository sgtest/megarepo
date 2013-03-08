// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

fn is_even(&&x: uint) -> bool { (x % 2u) == 0u }

pub fn main() {
    fail_unless!([1u, 3u].min() == 1u);
    fail_unless!([3u, 1u].min() == 1u);
    fail_unless!(iter::min(&Some(1u)) == 1u);

    fail_unless!([1u, 3u].max() == 3u);
    fail_unless!([3u, 1u].max() == 3u);
    fail_unless!(iter::max(&Some(3u)) == 3u);
}
