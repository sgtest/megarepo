// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// ignore-windows
// exec-env:RUST_LOG=debug
// compile-flags:-C debug-assertions=y

#![feature(rustc_private)]

#[macro_use]
extern crate log;

use std::process::Command;
use std::env;
use std::str;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 && args[1] == "child" {
        debug!("foo");
        debug!("bar");
        return
    }

    let p = Command::new(&args[0])
                    .arg("child")
                    .output().unwrap();
    assert!(p.status.success());
    let mut lines = str::from_utf8(&p.stderr).unwrap().lines();
    assert!(lines.next().unwrap().contains("foo"));
    assert!(lines.next().unwrap().contains("bar"));
}
