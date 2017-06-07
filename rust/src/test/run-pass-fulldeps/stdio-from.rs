// Copyright 2017 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// ignore-cross-compile

#![feature(rustc_private)]

extern crate rustc_back;

use std::env;
use std::fs::File;
use std::io;
use std::io::{Read, Write};
use std::process::{Command, Stdio};

use rustc_back::tempdir::TempDir;

fn main() {
    if env::args().len() > 1 {
        child().unwrap()
    } else {
        parent().unwrap()
    }
}

fn parent() -> io::Result<()> {
    let td = TempDir::new("foo").unwrap();
    let input = td.path().join("input");
    let output = td.path().join("output");

    File::create(&input)?.write_all(b"foo\n")?;

    // Set up this chain:
    //     $ me <file | me | me >file
    // ... to duplicate each line 8 times total.

    let mut child1 = Command::new(env::current_exe()?)
        .arg("first")
        .stdin(File::open(&input)?) // tests File::into()
        .stdout(Stdio::piped())
        .spawn()?;

    let mut child3 = Command::new(env::current_exe()?)
        .arg("third")
        .stdin(Stdio::piped())
        .stdout(File::create(&output)?) // tests File::into()
        .spawn()?;

    // Started out of order so we can test both `ChildStdin` and `ChildStdout`.
    let mut child2 = Command::new(env::current_exe()?)
        .arg("second")
        .stdin(child1.stdout.take().unwrap()) // tests ChildStdout::into()
        .stdout(child3.stdin.take().unwrap()) // tests ChildStdin::into()
        .spawn()?;

    assert!(child1.wait()?.success());
    assert!(child2.wait()?.success());
    assert!(child3.wait()?.success());

    let mut data = String::new();
    File::open(&output)?.read_to_string(&mut data)?;
    for line in data.lines() {
        assert_eq!(line, "foo");
    }
    assert_eq!(data.lines().count(), 8);
    Ok(())
}

fn child() -> io::Result<()> {
    // double everything
    let mut input = vec![];
    io::stdin().read_to_end(&mut input)?;
    io::stdout().write_all(&input)?;
    io::stdout().write_all(&input)?;
    Ok(())
}
