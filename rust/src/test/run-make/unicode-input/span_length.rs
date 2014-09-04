// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::{char, os};
use std::io::{File, Command};
use std::rand::{task_rng, Rng};

// creates a file with `fn main() { <random ident> }` and checks the
// compiler emits a span of the appropriate length (for the
// "unresolved name" message); currently just using the number of code
// points, but should be the number of graphemes (FIXME #7043)

fn random_char() -> char {
    let mut rng = task_rng();
    // a subset of the XID_start Unicode table (ensuring that the
    // compiler doesn't fail with an "unrecognised token" error)
    let (lo, hi): (u32, u32) = match rng.gen_range(1u32, 4u32 + 1) {
        1 => (0x41, 0x5a),
        2 => (0xf8, 0x1ba),
        3 => (0x1401, 0x166c),
        _ => (0x10400, 0x1044f)
    };

    char::from_u32(rng.gen_range(lo, hi + 1)).unwrap()
}

fn main() {
    let args = os::args();
    let rustc = args.get(1).as_slice();
    let tmpdir = Path::new(args.get(2).as_slice());
    let main_file = tmpdir.join("span_main.rs");

    for _ in range(0u, 100) {
        let n = task_rng().gen_range(3u, 20);

        {
            let _ = write!(&mut File::create(&main_file).unwrap(),
                           "#![feature(non_ascii_idents)] fn main() {{ {} }}",
                           // random string of length n
                           range(0, n).map(|_| random_char()).collect::<String>());
        }

        // rustc is passed to us with --out-dir and -L etc., so we
        // can't exec it directly
        let result = Command::new("sh")
                             .arg("-c")
                             .arg(format!("{} {}",
                                          rustc,
                                          main_file.as_str()
                                                   .unwrap()).as_slice())
                             .output().unwrap();

        let err = String::from_utf8_lossy(result.error.as_slice());

        // the span should end the line (e.g no extra ~'s)
        let expected_span = format!("^{}\n", "~".repeat(n - 1));
        assert!(err.as_slice().contains(expected_span.as_slice()));
    }
}
