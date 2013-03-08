// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

pub fn main() {
    fail_unless!([].count(&22u) == 0u);
    fail_unless!([1u, 3u].count(&22u) == 0u);
    fail_unless!([22u, 1u, 3u].count(&22u) == 1u);
    fail_unless!([22u, 1u, 22u].count(&22u) == 2u);
    fail_unless!(iter::count(&None::<uint>, &22u) == 0u);
    fail_unless!(iter::count(&Some(1u), &22u) == 0u);
    fail_unless!(iter::count(&Some(22u), &22u) == 1u);
}
