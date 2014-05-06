// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// ignore-android
// ignore-win32

#![feature(phase)]

#[phase(syntax, link)]
extern crate log;

use std::io::{Process, ProcessConfig};
use std::os;
use std::str;

fn main() {
    let args = os::args();
    if args.len() > 1 && args[1].as_slice() == "child" {
        debug!("foo");
        debug!("bar");
        return
    }

    let env = [("RUST_LOG".to_owned(), "debug".to_owned())];
    let config = ProcessConfig {
        program: args[0].as_slice(),
        args: &["child".to_owned()],
        env: Some(env.as_slice()),
        ..ProcessConfig::new()
    };
    let p = Process::configure(config).unwrap().wait_with_output();
    assert!(p.status.success());
    let mut lines = str::from_utf8(p.error.as_slice()).unwrap().lines();
    assert!(lines.next().unwrap().contains("foo"));
    assert!(lines.next().unwrap().contains("bar"));
}

