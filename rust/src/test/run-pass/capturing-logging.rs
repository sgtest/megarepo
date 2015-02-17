// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// exec-env:RUST_LOG=info

#![allow(unknown_features)]
#![feature(box_syntax)]

#[macro_use]
extern crate log;

use log::{set_logger, Logger, LogRecord};
use std::sync::mpsc::channel;
use std::fmt;
use std::old_io::{ChanReader, ChanWriter};
use std::thread::Thread;

struct MyWriter(ChanWriter);

impl Logger for MyWriter {
    fn log(&mut self, record: &LogRecord) {
        let MyWriter(ref mut inner) = *self;
        write!(inner, "{}", record.args);
    }
}

fn main() {
    let (tx, rx) = channel();
    let (mut r, w) = (ChanReader::new(rx), ChanWriter::new(tx));
    let _t = Thread::spawn(move|| {
        set_logger(box MyWriter(w) as Box<Logger+Send>);
        debug!("debug");
        info!("info");
    });
    let s = r.read_to_string().unwrap();
    assert!(s.contains("info"));
    assert!(!s.contains("debug"));
}
