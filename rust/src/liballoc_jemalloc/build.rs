// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

extern crate build_helper;
extern crate gcc;

use std::env;
use std::path::PathBuf;
use std::process::Command;
use build_helper::run;

fn main() {
    println!("cargo:rustc-cfg=cargobuild");

    let target = env::var("TARGET").unwrap();
    let host = env::var("HOST").unwrap();
    let build_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let src_dir = env::current_dir().unwrap();

    if let Some(jemalloc) = env::var_os("JEMALLOC_OVERRIDE") {
        let jemalloc = PathBuf::from(jemalloc);
        println!("cargo:rustc-link-search=native={}",
                 jemalloc.parent().unwrap().display());
        let stem = jemalloc.file_stem().unwrap().to_str().unwrap();
        let name = jemalloc.file_name().unwrap().to_str().unwrap();
        let kind = if name.ends_with(".a") {"static"} else {"dylib"};
        println!("cargo:rustc-link-lib={}={}", kind, &stem[3..]);
        return
    }

    let compiler = gcc::Config::new().get_compiler();
    let ar = build_helper::cc2ar(compiler.path(), &target);
    let cflags = compiler.args().iter().map(|s| s.to_str().unwrap())
                         .collect::<Vec<_>>().join(" ");

    let mut cmd = Command::new("sh");
    cmd.arg(src_dir.join("../jemalloc/configure").to_str().unwrap()
                   .replace("C:\\", "/c/")
                   .replace("\\", "/"))
       .current_dir(&build_dir)
       .env("CC", compiler.path())
       .env("EXTRA_CFLAGS", cflags)
       .env("AR", &ar)
       .env("RANLIB", format!("{} s", ar.display()));

    if target.contains("windows-gnu") {
        // A bit of history here, this used to be --enable-lazy-lock added in
        // #14006 which was filed with jemalloc in jemalloc/jemalloc#83 which
        // was also reported to MinGW:
        //
        //  http://sourceforge.net/p/mingw-w64/bugs/395/
        //
        // When updating jemalloc to 4.0, however, it was found that binaries
        // would exit with the status code STATUS_RESOURCE_NOT_OWNED indicating
        // that a thread was unlocking a mutex it never locked. Disabling this
        // "lazy lock" option seems to fix the issue, but it was enabled by
        // default for MinGW targets in 13473c7 for jemalloc.
        //
        // As a result of all that, force disabling lazy lock on Windows, and
        // after reading some code it at least *appears* that the initialization
        // of mutexes is otherwise ok in jemalloc, so shouldn't cause problems
        // hopefully...
        //
        // tl;dr: make windows behave like other platforms by disabling lazy
        //        locking, but requires passing an option due to a historical
        //        default with jemalloc.
        cmd.arg("--disable-lazy-lock");
    } else if target.contains("ios") || target.contains("android") {
        cmd.arg("--disable-tls");
    }

    if cfg!(feature = "debug-jemalloc") {
        cmd.arg("--enable-debug");
    }

    // Turn off broken quarantine (see jemalloc/jemalloc#161)
    cmd.arg("--disable-fill");
    cmd.arg("--with-jemalloc-prefix=je_");
    cmd.arg(format!("--host={}", build_helper::gnu_target(&target)));
    cmd.arg(format!("--build={}", build_helper::gnu_target(&host)));

    run(&mut cmd);
    run(Command::new("make")
                .current_dir(&build_dir)
                .arg("build_lib_static")
                .arg("-j").arg(env::var("NUM_JOBS").unwrap()));

    if target.contains("windows") {
        println!("cargo:rustc-link-lib=static=jemalloc");
    } else {
        println!("cargo:rustc-link-lib=static=jemalloc_pic");
    }
    println!("cargo:rustc-link-search=native={}/lib", build_dir.display());
    if target.contains("android") {
        println!("cargo:rustc-link-lib=gcc");
    } else if !target.contains("windows") {
        println!("cargo:rustc-link-lib=pthread");
    }
}
