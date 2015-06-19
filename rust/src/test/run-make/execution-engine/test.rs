// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(rustc_private)]

extern crate rustc;
extern crate rustc_driver;
extern crate rustc_lint;
extern crate rustc_resolve;
extern crate syntax;

use std::ffi::{CStr, CString};
use std::mem::transmute;
use std::path::PathBuf;
use std::thread::Builder;

use rustc::ast_map;
use rustc::llvm;
use rustc::metadata::cstore::RequireDynamic;
use rustc::middle::ty;
use rustc::session::config::{self, basic_options, build_configuration, Input, Options};
use rustc::session::build_session;
use rustc_driver::driver;
use rustc_resolve::MakeGlobMap;

use syntax::diagnostics::registry::Registry;

fn main() {
    let program = r#"
    #[no_mangle]
    pub static TEST_STATIC: i32 = 42;
    "#;

    let program2 = r#"
    #[no_mangle]
    pub fn test_add(a: i32, b: i32) -> i32 { a + b }
    "#;

    let mut path = match std::env::args().nth(2) {
        Some(path) => PathBuf::from(&path),
        None => panic!("missing rustc path")
    };

    // Remove two segments from rustc path to get sysroot.
    path.pop();
    path.pop();

    let mut ee = ExecutionEngine::new(program, path);

    let test_static = match ee.get_global("TEST_STATIC") {
        Some(g) => g as *const i32,
        None => panic!("failed to get global")
    };

    assert_eq!(unsafe { *test_static }, 42);

    ee.add_module(program2);

    let test_add: fn(i32, i32) -> i32;

    test_add = match ee.get_function("test_add") {
        Some(f) => unsafe { transmute(f) },
        None => panic!("failed to get function")
    };

    assert_eq!(test_add(1, 2), 3);
}

struct ExecutionEngine {
    ee: llvm::ExecutionEngineRef,
    modules: Vec<llvm::ModuleRef>,
    sysroot: PathBuf,
}

impl ExecutionEngine {
    pub fn new(program: &str, sysroot: PathBuf) -> ExecutionEngine {
        let (llmod, deps) = compile_program(program, sysroot.clone())
            .expect("failed to compile program");

        let ee = unsafe { llvm::LLVMBuildExecutionEngine(llmod) };

        if ee.is_null() {
            panic!("Failed to create ExecutionEngine: {}", llvm_error());
        }

        let ee = ExecutionEngine{
            ee: ee,
            modules: vec![llmod],
            sysroot: sysroot,
        };

        ee.load_deps(&deps);
        ee
    }

    pub fn add_module(&mut self, program: &str) {
        let (llmod, deps) = compile_program(program, self.sysroot.clone())
            .expect("failed to compile program in add_module");

        unsafe { llvm::LLVMExecutionEngineAddModule(self.ee, llmod); }

        self.modules.push(llmod);
        self.load_deps(&deps);
    }

    /// Returns a raw pointer to the named function.
    pub fn get_function(&mut self, name: &str) -> Option<*const ()> {
        let s = CString::new(name.as_bytes()).unwrap();

        for &m in &self.modules {
            let fv = unsafe { llvm::LLVMGetNamedFunction(m, s.as_ptr()) };

            if !fv.is_null() {
                let fp = unsafe { llvm::LLVMGetPointerToGlobal(self.ee, fv) };

                assert!(!fp.is_null());
                return Some(fp);
            }
        }
        None
    }

    /// Returns a raw pointer to the named global item.
    pub fn get_global(&mut self, name: &str) -> Option<*const ()> {
        let s = CString::new(name.as_bytes()).unwrap();

        for &m in &self.modules {
            let gv = unsafe { llvm::LLVMGetNamedGlobal(m, s.as_ptr()) };

            if !gv.is_null() {
                let gp = unsafe { llvm::LLVMGetPointerToGlobal(self.ee, gv) };

                assert!(!gp.is_null());
                return Some(gp);
            }
        }
        None
    }

    /// Loads all dependencies of compiled code.
    /// Expects a series of paths to dynamic library files.
    fn load_deps(&self, deps: &[PathBuf]) {
        for path in deps {
            let s = match path.as_os_str().to_str() {
                Some(s) => s,
                None => panic!(
                    "Could not convert crate path to UTF-8 string: {:?}", path)
            };
            let cs = CString::new(s).unwrap();

            let res = unsafe { llvm::LLVMRustLoadDynamicLibrary(cs.as_ptr()) };

            if res == 0 {
                panic!("Failed to load crate {:?}: {}",
                    path.display(), llvm_error());
            }
        }
    }
}

impl Drop for ExecutionEngine {
    fn drop(&mut self) {
        unsafe { llvm::LLVMDisposeExecutionEngine(self.ee) };
    }
}

/// Returns last error from LLVM wrapper code.
fn llvm_error() -> String {
    String::from_utf8_lossy(
        unsafe { CStr::from_ptr(llvm::LLVMRustGetLastError()).to_bytes() })
        .into_owned()
}

fn build_exec_options(sysroot: PathBuf) -> Options {
    let mut opts = basic_options();

    // librustc derives sysroot from the executable name.
    // Since we are not rustc, we must specify it.
    opts.maybe_sysroot = Some(sysroot);

    // Prefer faster build time
    opts.optimize = config::No;

    // Don't require a `main` function
    opts.crate_types = vec![config::CrateTypeDylib];

    opts
}

/// Compiles input up to phase 4, translation to LLVM.
///
/// Returns the LLVM `ModuleRef` and a series of paths to dynamic libraries
/// for crates used in the given input.
fn compile_program(input: &str, sysroot: PathBuf)
                   -> Option<(llvm::ModuleRef, Vec<PathBuf>)> {
    let input = Input::Str(input.to_string());
    let thread = Builder::new().name("compile_program".to_string());

    let handle = thread.spawn(move || {
        let opts = build_exec_options(sysroot);
        let sess = build_session(opts, None, Registry::new(&rustc::DIAGNOSTICS));
        rustc_lint::register_builtins(&mut sess.lint_store.borrow_mut(), Some(&sess));

        let cfg = build_configuration(&sess);

        let id = "input".to_string();

        let krate = driver::phase_1_parse_input(&sess, cfg, &input);

        let krate = driver::phase_2_configure_and_expand(&sess, krate, &id, None)
            .expect("phase_2 returned `None`");

        let mut forest = ast_map::Forest::new(krate);
        let arenas = ty::CtxtArenas::new();
        let ast_map = driver::assign_node_ids_and_map(&sess, &mut forest);

        driver::phase_3_run_analysis_passes(
            sess, ast_map, &arenas, id, MakeGlobMap::No, |tcx, analysis| {

            let trans = driver::phase_4_translate_to_llvm(tcx, analysis);

            let crates = tcx.sess.cstore.get_used_crates(RequireDynamic);

            // Collect crates used in the session.
            // Reverse order finds dependencies first.
            let deps = crates.into_iter().rev()
                .filter_map(|(_, p)| p).collect();

            assert_eq!(trans.modules.len(), 1);
            let llmod = trans.modules[0].llmod;

            // Workaround because raw pointers do not impl Send
            let modp = llmod as usize;

            (modp, deps)
        }).1
    }).unwrap();

    match handle.join() {
        Ok((llmod, deps)) => Some((llmod as llvm::ModuleRef, deps)),
        Err(_) => None
    }
}
