// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// xfail-fast

extern mod extra;

use extra::tempfile;
use std::rt::io;
use std::rt::io::Writer;
use std::rt::io::file::FileInfo;
use std::os;

pub fn main() {
    let dir = tempfile::TempDir::new_in(&Path::new("."), "").unwrap();
    let path = dir.path().join("file");

    {
        match path.open_writer(io::CreateOrTruncate) {
            None => unreachable!(),
            Some(f) => {
                let mut f = f;
                for _ in range(0u, 1000) {
                    f.write([0]);
                }
            }
        }
    }

    assert!(path.exists());
    assert_eq!(path.get_size(), Some(1000));
}
