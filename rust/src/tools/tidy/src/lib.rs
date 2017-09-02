// Copyright 2017 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Library used by tidy and other tools
//!
//! This library contains the tidy lints and exposes it
//! to be used by tools.

#![deny(warnings)]

use std::fs;

use std::path::Path;

macro_rules! t {
    ($e:expr, $p:expr) => (match $e {
        Ok(e) => e,
        Err(e) => panic!("{} failed on {} with {}", stringify!($e), ($p).display(), e),
    });

    ($e:expr) => (match $e {
        Ok(e) => e,
        Err(e) => panic!("{} failed with {}", stringify!($e), e),
    })
}

macro_rules! tidy_error {
    ($bad:expr, $fmt:expr, $($arg:tt)*) => ({
        use std::io::Write;
        *$bad = true;
        write!(::std::io::stderr(), "tidy error: ").expect("could not write to stderr");
        writeln!(::std::io::stderr(), $fmt, $($arg)*).expect("could not write to stderr");
    });
}

pub mod bins;
pub mod style;
pub mod errors;
pub mod features;
pub mod cargo;
pub mod pal;
pub mod deps;
pub mod unstable_book;

fn filter_dirs(path: &Path) -> bool {
    let skip = [
        "src/jemalloc",
        "src/llvm",
        "src/libbacktrace",
        "src/libcompiler_builtins",
        "src/compiler-rt",
        "src/rustllvm",
        "src/liblibc",
        "src/vendor",
        "src/rt/hoedown",
        "src/tools/cargo",
        "src/tools/rls",
        "src/tools/clippy",
        "src/tools/rust-installer",
    ];
    skip.iter().any(|p| path.ends_with(p))
}

fn walk_many(paths: &[&Path], skip: &mut FnMut(&Path) -> bool, f: &mut FnMut(&Path)) {
    for path in paths {
        walk(path, skip, f);
    }
}

fn walk(path: &Path, skip: &mut FnMut(&Path) -> bool, f: &mut FnMut(&Path)) {
    for entry in t!(fs::read_dir(path), path) {
        let entry = t!(entry);
        let kind = t!(entry.file_type());
        let path = entry.path();
        if kind.is_dir() {
            if !skip(&path) {
                walk(&path, skip, f);
            }
        } else {
            f(&path);
        }
    }
}
