// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Be sure that when a SIGPIPE would have been received that the entire process
// doesn't die in a ball of fire, but rather it's gracefully handled.

use std::os;
use std::old_io::PipeStream;
use std::old_io::Command;

fn test() {
    let os::Pipe { reader, writer } = unsafe { os::pipe().unwrap() };
    let reader = PipeStream::open(reader);
    let mut writer = PipeStream::open(writer);
    drop(reader);

    let _ = writer.write(&[1]);
}

fn main() {
    let args = os::args();
    let args = args;
    if args.len() > 1 && args[1] == "test" {
        return test();
    }

    let mut p = Command::new(&args[0])
                        .arg("test").spawn().unwrap();
    assert!(p.wait().unwrap().success());
}
