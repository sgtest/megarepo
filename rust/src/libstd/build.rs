// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

extern crate gcc;
extern crate build_helper;

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use build_helper::run;

fn main() {
    println!("cargo:rustc-cfg=cargobuild");

    let target = env::var("TARGET").unwrap();
    let host = env::var("HOST").unwrap();
    if !target.contains("apple") && !target.contains("msvc") && !target.contains("emscripten"){
        build_libbacktrace(&host, &target);
    }

    if target.contains("linux") {
        if target.contains("musl") && (target.contains("x86_64") || target.contains("i686")) {
            println!("cargo:rustc-link-lib=static=unwind");
        } else if target.contains("android") {
            println!("cargo:rustc-link-lib=dl");
            println!("cargo:rustc-link-lib=log");
            println!("cargo:rustc-link-lib=gcc");
        } else {
            println!("cargo:rustc-link-lib=dl");
            println!("cargo:rustc-link-lib=rt");
            println!("cargo:rustc-link-lib=pthread");
            println!("cargo:rustc-link-lib=gcc_s");
        }
    } else if target.contains("freebsd") {
        println!("cargo:rustc-link-lib=execinfo");
        println!("cargo:rustc-link-lib=pthread");
        println!("cargo:rustc-link-lib=gcc_s");
    } else if target.contains("dragonfly") || target.contains("bitrig") ||
              target.contains("netbsd") || target.contains("openbsd") {
        println!("cargo:rustc-link-lib=pthread");

        if target.contains("rumprun") {
            println!("cargo:rustc-link-lib=unwind");
        } else if target.contains("netbsd") {
            println!("cargo:rustc-link-lib=gcc_s");
        } else if target.contains("openbsd") {
            println!("cargo:rustc-link-lib=gcc");
        } else if target.contains("bitrig") {
            println!("cargo:rustc-link-lib=c++abi");
        } else if target.contains("dragonfly") {
            println!("cargo:rustc-link-lib=gcc_pic");
        }
    } else if target.contains("apple-darwin") {
        println!("cargo:rustc-link-lib=System");
    } else if target.contains("apple-ios") {
        println!("cargo:rustc-link-lib=System");
        println!("cargo:rustc-link-lib=objc");
        println!("cargo:rustc-link-lib=framework=Security");
        println!("cargo:rustc-link-lib=framework=Foundation");
    } else if target.contains("windows") {
        if target.contains("windows-gnu") {
            println!("cargo:rustc-link-lib=gcc_eh");
        }
        println!("cargo:rustc-link-lib=advapi32");
        println!("cargo:rustc-link-lib=ws2_32");
        println!("cargo:rustc-link-lib=userenv");
        println!("cargo:rustc-link-lib=shell32");
    }
}

fn build_libbacktrace(host: &str, target: &str) {
    let src_dir = env::current_dir().unwrap().join("../libbacktrace");
    let build_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());

    println!("cargo:rustc-link-lib=static=backtrace");
    println!("cargo:rustc-link-search=native={}/.libs", build_dir.display());

    if fs::metadata(&build_dir.join(".libs/libbacktrace.a")).is_ok() {
        return
    }

    let compiler = gcc::Config::new().get_compiler();
    let ar = build_helper::cc2ar(compiler.path(), target);
    let cflags = compiler.args().iter().map(|s| s.to_str().unwrap())
                         .collect::<Vec<_>>().join(" ");
    run(Command::new("sh")
                .current_dir(&build_dir)
                .arg(src_dir.join("configure").to_str().unwrap()
                            .replace("C:\\", "/c/")
                            .replace("\\", "/"))
                .arg("--with-pic")
                .arg("--disable-multilib")
                .arg("--disable-shared")
                .arg("--disable-host-shared")
                .arg(format!("--host={}", build_helper::gnu_target(target)))
                .arg(format!("--build={}", build_helper::gnu_target(host)))
                .env("CC", compiler.path())
                .env("AR", &ar)
                .env("RANLIB", format!("{} s", ar.display()))
                .env("CFLAGS", cflags));
    run(Command::new("make")
                .current_dir(&build_dir)
                .arg(format!("INCDIR={}", src_dir.display()))
                .arg("-j").arg(env::var("NUM_JOBS").unwrap()));
}
