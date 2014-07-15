// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use super::link;
use driver::session;
use driver::config;
use llvm;
use llvm::archive_ro::ArchiveRO;
use llvm::{ModuleRef, TargetMachineRef, True, False};
use metadata::cstore;
use util::common::time;

use libc;
use flate;

pub fn run(sess: &session::Session, llmod: ModuleRef,
           tm: TargetMachineRef, reachable: &[String]) {
    if sess.opts.cg.prefer_dynamic {
        sess.err("cannot prefer dynamic linking when performing LTO");
        sess.note("only 'staticlib' and 'bin' outputs are supported with LTO");
        sess.abort_if_errors();
    }

    // Make sure we actually can run LTO
    for crate_type in sess.crate_types.borrow().iter() {
        match *crate_type {
            config::CrateTypeExecutable | config::CrateTypeStaticlib => {}
            _ => {
                sess.fatal("lto can only be run for executables and \
                            static library outputs");
            }
        }
    }

    // For each of our upstream dependencies, find the corresponding rlib and
    // load the bitcode from the archive. Then merge it into the current LLVM
    // module that we've got.
    let crates = sess.cstore.get_used_crates(cstore::RequireStatic);
    for (cnum, path) in crates.move_iter() {
        let name = sess.cstore.get_crate_data(cnum).name.clone();
        let path = match path {
            Some(p) => p,
            None => {
                sess.fatal(format!("could not find rlib for: `{}`",
                                   name).as_slice());
            }
        };

        let archive = ArchiveRO::open(&path).expect("wanted an rlib");
        debug!("reading {}", name);
        let bc = time(sess.time_passes(),
                      format!("read {}.bytecode.deflate", name).as_slice(),
                      (),
                      |_| {
                          archive.read(format!("{}.bytecode.deflate",
                                               name).as_slice())
                      });
        let bc = bc.expect("missing compressed bytecode in archive!");
        let bc = time(sess.time_passes(),
                      format!("inflate {}.bc", name).as_slice(),
                      (),
                      |_| {
                          match flate::inflate_bytes(bc) {
                              Some(bc) => bc,
                              None => {
                                  sess.fatal(format!("failed to decompress \
                                                      bc of `{}`",
                                                     name).as_slice())
                              }
                          }
                      });
        let ptr = bc.as_slice().as_ptr();
        debug!("linking {}", name);
        time(sess.time_passes(),
             format!("ll link {}", name).as_slice(),
             (),
             |()| unsafe {
            if !llvm::LLVMRustLinkInExternalBitcode(llmod,
                                                    ptr as *const libc::c_char,
                                                    bc.len() as libc::size_t) {
                link::llvm_err(sess,
                               format!("failed to load bc of `{}`",
                                       name.as_slice()));
            }
        });
    }

    // Internalize everything but the reachable symbols of the current module
    let cstrs: Vec<::std::c_str::CString> =
        reachable.iter().map(|s| s.as_slice().to_c_str()).collect();
    let arr: Vec<*const i8> = cstrs.iter().map(|c| c.as_ptr()).collect();
    let ptr = arr.as_ptr();
    unsafe {
        llvm::LLVMRustRunRestrictionPass(llmod,
                                         ptr as *const *const libc::c_char,
                                         arr.len() as libc::size_t);
    }

    if sess.no_landing_pads() {
        unsafe {
            llvm::LLVMRustMarkAllFunctionsNounwind(llmod);
        }
    }

    // Now we have one massive module inside of llmod. Time to run the
    // LTO-specific optimization passes that LLVM provides.
    //
    // This code is based off the code found in llvm's LTO code generator:
    //      tools/lto/LTOCodeGenerator.cpp
    debug!("running the pass manager");
    unsafe {
        let pm = llvm::LLVMCreatePassManager();
        llvm::LLVMRustAddAnalysisPasses(tm, pm, llmod);
        "verify".with_c_str(|s| llvm::LLVMRustAddPass(pm, s));

        let builder = llvm::LLVMPassManagerBuilderCreate();
        llvm::LLVMPassManagerBuilderPopulateLTOPassManager(builder, pm,
            /* Internalize = */ False,
            /* RunInliner = */ True);
        llvm::LLVMPassManagerBuilderDispose(builder);

        "verify".with_c_str(|s| llvm::LLVMRustAddPass(pm, s));

        time(sess.time_passes(), "LTO pases", (), |()|
             llvm::LLVMRunPassManager(pm, llmod));

        llvm::LLVMDisposePassManager(pm);
    }
    debug!("lto done");
}
