// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Test that the CompilerCalls interface to the compiler works.

// ignore-android

#![feature(rustc_private)]
#![feature(core)]

extern crate getopts;
extern crate rustc;
extern crate rustc_driver;
extern crate syntax;

use rustc::session::Session;
use rustc::session::config::{self, Input};
use rustc_driver::{driver, CompilerCalls, Compilation};
use syntax::diagnostics;

use std::path::PathBuf;

struct TestCalls {
    count: u32
}

impl<'a> CompilerCalls<'a> for TestCalls {
    fn early_callback(&mut self,
                      _: &getopts::Matches,
                      _: &diagnostics::registry::Registry)
                      -> Compilation {
        self.count *= 2;
        Compilation::Continue
    }

    fn late_callback(&mut self,
                     _: &getopts::Matches,
                     _: &Session,
                     _: &Input,
                     _: &Option<PathBuf>,
                     _: &Option<PathBuf>)
                     -> Compilation {
        self.count *= 3;
        Compilation::Stop
    }

    fn some_input(&mut self, input: Input, input_path: Option<PathBuf>)
                  -> (Input, Option<PathBuf>) {
        self.count *= 5;
        (input, input_path)
    }

    fn no_input(&mut self,
                _: &getopts::Matches,
                _: &config::Options,
                _: &Option<PathBuf>,
                _: &Option<PathBuf>,
                _: &diagnostics::registry::Registry)
                -> Option<(Input, Option<PathBuf>)> {
        panic!("This shouldn't happen");
    }

    fn build_controller(&mut self, _: &Session) -> driver::CompileController<'a> {
        panic!("This shouldn't be called");
    }
}


fn main() {
    let mut tc = TestCalls { count: 1 };
    // we should never get use this filename, but lets make sure they are valid args.
    let args = vec!["compiler-calls".to_string(), "foo.rs".to_string()];
    rustc_driver::run_compiler(args.as_slice(), &mut tc);
    assert!(tc.count == 30);
}

