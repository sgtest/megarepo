// Copyright 2017 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use syntax_pos::symbol::Symbol;
use back::write::create_target_machine;
use llvm;
use rustc::session::Session;
use rustc::session::config::PrintRequest;
use libc::c_int;
use std::ffi::CString;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;

static POISONED: AtomicBool = AtomicBool::new(false);
static INIT: Once = Once::new();

pub(crate) fn init(sess: &Session) {
    unsafe {
        // Before we touch LLVM, make sure that multithreading is enabled.
        INIT.call_once(|| {
            if llvm::LLVMStartMultithreaded() != 1 {
                // use an extra bool to make sure that all future usage of LLVM
                // cannot proceed despite the Once not running more than once.
                POISONED.store(true, Ordering::SeqCst);
            }

            configure_llvm(sess);
        });

        if POISONED.load(Ordering::SeqCst) {
            bug!("couldn't enable multi-threaded LLVM");
        }
    }
}

fn require_inited() {
    INIT.call_once(|| bug!("llvm is not initialized"));
    if POISONED.load(Ordering::SeqCst) {
        bug!("couldn't enable multi-threaded LLVM");
    }
}

unsafe fn configure_llvm(sess: &Session) {
    let mut llvm_c_strs = Vec::new();
    let mut llvm_args = Vec::new();

    {
        let mut add = |arg: &str| {
            let s = CString::new(arg).unwrap();
            llvm_args.push(s.as_ptr());
            llvm_c_strs.push(s);
        };
        add("rustc"); // fake program name
        if sess.time_llvm_passes() { add("-time-passes"); }
        if sess.print_llvm_passes() { add("-debug-pass=Structure"); }

        for arg in &sess.opts.cg.llvm_args {
            add(&(*arg));
        }
    }

    llvm::LLVMInitializePasses();

    llvm::initialize_available_targets();

    llvm::LLVMRustSetLLVMOptions(llvm_args.len() as c_int,
                                 llvm_args.as_ptr());
}

// WARNING: the features must be known to LLVM or the feature
// detection code will walk past the end of the feature array,
// leading to crashes.

const ARM_WHITELIST: &'static [&'static str] = &["neon", "v7", "vfp2", "vfp3", "vfp4"];

const AARCH64_WHITELIST: &'static [&'static str] = &["neon", "v7"];

const X86_WHITELIST: &'static [&'static str] = &["avx", "avx2", "bmi", "bmi2", "sse",
                                                 "sse2", "sse3", "sse4.1", "sse4.2",
                                                 "ssse3", "tbm", "lzcnt", "popcnt",
                                                 "sse4a", "rdrnd", "rdseed", "fma",
                                                 "xsave", "xsaveopt", "xsavec",
                                                 "xsaves", "aes", "pclmulqdq",
                                                 "avx512bw", "avx512cd",
                                                 "avx512dq", "avx512er",
                                                 "avx512f", "avx512ifma",
                                                 "avx512pf", "avx512vbmi",
                                                 "avx512vl", "avx512vpopcntdq",
                                                 "mmx", "fxsr"];

const HEXAGON_WHITELIST: &'static [&'static str] = &["hvx", "hvx-double"];

const POWERPC_WHITELIST: &'static [&'static str] = &["altivec",
                                                     "power8-altivec", "power9-altivec",
                                                     "power8-vector", "power9-vector",
                                                     "vsx"];

const MIPS_WHITELIST: &'static [&'static str] = &["msa"];

pub fn to_llvm_feature(s: &str) -> &str {
    match s {
        "pclmulqdq" => "pclmul",
        s => s,
    }
}

pub fn target_features(sess: &Session) -> Vec<Symbol> {
    let target_machine = create_target_machine(sess);
    target_feature_whitelist(sess)
        .iter()
        .filter(|feature| {
            let llvm_feature = to_llvm_feature(feature);
            let cstr = CString::new(llvm_feature).unwrap();
            unsafe { llvm::LLVMRustHasFeature(target_machine, cstr.as_ptr()) }
        })
        .map(|feature| Symbol::intern(feature)).collect()
}

pub fn target_feature_whitelist(sess: &Session) -> &'static [&'static str] {
    match &*sess.target.target.arch {
        "arm" => ARM_WHITELIST,
        "aarch64" => AARCH64_WHITELIST,
        "x86" | "x86_64" => X86_WHITELIST,
        "hexagon" => HEXAGON_WHITELIST,
        "mips" | "mips64" => MIPS_WHITELIST,
        "powerpc" | "powerpc64" => POWERPC_WHITELIST,
        _ => &[],
    }
}

pub fn print_version() {
    // Can be called without initializing LLVM
    unsafe {
        println!("LLVM version: {}.{}",
                 llvm::LLVMRustVersionMajor(), llvm::LLVMRustVersionMinor());
    }
}

pub fn print_passes() {
    // Can be called without initializing LLVM
    unsafe { llvm::LLVMRustPrintPasses(); }
}

pub(crate) fn print(req: PrintRequest, sess: &Session) {
    require_inited();
    let tm = create_target_machine(sess);
    unsafe {
        match req {
            PrintRequest::TargetCPUs => llvm::LLVMRustPrintTargetCPUs(tm),
            PrintRequest::TargetFeatures => llvm::LLVMRustPrintTargetFeatures(tm),
            _ => bug!("rustc_trans can't handle print request: {:?}", req),
        }
    }
}
