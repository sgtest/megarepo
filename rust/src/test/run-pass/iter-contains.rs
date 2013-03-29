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
    assert!([].contains(&22u) == false);
    assert!([1u, 3u].contains(&22u) == false);
    assert!([22u, 1u, 3u].contains(&22u) == true);
    assert!([1u, 22u, 3u].contains(&22u) == true);
    assert!([1u, 3u, 22u].contains(&22u) == true);
    assert!(iter::contains(&None::<uint>, &22u) == false);
    assert!(iter::contains(&Some(1u), &22u) == false);
    assert!(iter::contains(&Some(22u), &22u) == true);
}
