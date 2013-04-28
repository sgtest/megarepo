// Copyright 2012-2013 The Rust Project Developers. See the
// COPYRIGHT file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[deriving(Eq)]
pub enum mode {
    mode_compile_fail,
    mode_run_fail,
    mode_run_pass,
    mode_pretty,
    mode_debug_info,
}

pub struct config {
    // The library paths required for running the compiler
    compile_lib_path: ~str,

    // The library paths required for running compiled programs
    run_lib_path: ~str,

    // The rustc executable
    rustc_path: Path,

    // The directory containing the tests to run
    src_base: Path,

    // The directory where programs should be built
    build_base: Path,

    // Directory for auxiliary libraries
    aux_base: Path,

    // The name of the stage being built (stage1, etc)
    stage_id: ~str,

    // The test mode, compile-fail, run-fail, run-pass
    mode: mode,

    // Run ignored tests
    run_ignored: bool,

    // Only run tests that match this filter
    filter: Option<~str>,

    // Write out a parseable log of tests that were run
    logfile: Option<Path>,

    // A command line to prefix program execution with,
    // for running under valgrind
    runtool: Option<~str>,

    // Flags to pass to the compiler
    rustcflags: Option<~str>,

    // Run tests using the JIT
    jit: bool,

    // Run tests using the new runtime
    newrt: bool,

    // Explain what's going on
    verbose: bool

}
