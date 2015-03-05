// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! An abstraction of the terminal. Eventually, provide color and
//! verbosity support. For now, just a wrapper around stdout/stderr.

use std::env;
use std::old_io::stdio;

pub struct Term {
    err: Box<Writer + 'static>
}

impl Term {
    pub fn new() -> Term {
        Term {
            err: Box::new(stdio::stderr())
        }
    }

    pub fn err(&mut self, msg: &str) {
        // swallow any errors
        let _ = self.err.write_line(msg);
        env::set_exit_status(101);
    }
}
