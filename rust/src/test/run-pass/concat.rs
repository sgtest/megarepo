// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

pub fn main() {
    assert_eq!(format!(concat!("foo", "bar", "{}"), "baz"), "foobarbaz".to_strbuf());
    assert_eq!(format!(concat!()), "".to_strbuf());

    assert_eq!(
        concat!(1, 2i, 3u, 4f32, 4.0, 'a', true, ()),
        "12344.0atrue"
    );
}
