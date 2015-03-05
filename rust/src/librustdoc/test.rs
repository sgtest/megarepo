// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::cell::RefCell;
use std::collections::{HashSet, HashMap};
use std::dynamic_lib::DynamicLibrary;
use std::env;
use std::ffi::OsString;
use std::fs::TempDir;
use std::old_io;
use std::io;
use std::path::PathBuf;
use std::process::Command;
use std::str;
use std::sync::mpsc::channel;
use std::thread;
use std::thunk::Thunk;

use testing;
use rustc_lint;
use rustc::session::{self, config};
use rustc::session::config::get_unstable_features_setting;
use rustc::session::search_paths::{SearchPaths, PathKind};
use rustc_driver::{driver, Compilation};
use syntax::codemap::CodeMap;
use syntax::diagnostic;

use core;
use clean;
use clean::Clean;
use fold::DocFolder;
use html::markdown;
use passes;
use visit_ast::RustdocVisitor;

pub fn run(input: &str,
           cfgs: Vec<String>,
           libs: SearchPaths,
           externs: core::Externs,
           mut test_args: Vec<String>,
           crate_name: Option<String>)
           -> int {
    let input_path = PathBuf::new(input);
    let input = config::Input::File(input_path.clone());

    let sessopts = config::Options {
        maybe_sysroot: Some(env::current_exe().unwrap().parent().unwrap()
                                              .parent().unwrap().to_path_buf()),
        search_paths: libs.clone(),
        crate_types: vec!(config::CrateTypeDylib),
        externs: externs.clone(),
        unstable_features: get_unstable_features_setting(),
        ..config::basic_options().clone()
    };

    let codemap = CodeMap::new();
    let diagnostic_handler = diagnostic::default_handler(diagnostic::Auto, None, true);
    let span_diagnostic_handler =
    diagnostic::mk_span_handler(diagnostic_handler, codemap);

    let sess = session::build_session_(sessopts,
                                      Some(input_path.clone()),
                                      span_diagnostic_handler);
    rustc_lint::register_builtins(&mut sess.lint_store.borrow_mut(), Some(&sess));

    let mut cfg = config::build_configuration(&sess);
    cfg.extend(config::parse_cfgspecs(cfgs).into_iter());
    let krate = driver::phase_1_parse_input(&sess, cfg, &input);
    let krate = driver::phase_2_configure_and_expand(&sess, krate,
                                                     "rustdoc-test", None)
        .expect("phase_2_configure_and_expand aborted in rustdoc!");

    let ctx = core::DocContext {
        krate: &krate,
        maybe_typed: core::NotTyped(sess),
        input: input,
        external_paths: RefCell::new(Some(HashMap::new())),
        external_traits: RefCell::new(None),
        external_typarams: RefCell::new(None),
        inlined: RefCell::new(None),
        populated_crate_impls: RefCell::new(HashSet::new()),
    };

    let mut v = RustdocVisitor::new(&ctx, None);
    v.visit(ctx.krate);
    let mut krate = v.clean(&ctx);
    match crate_name {
        Some(name) => krate.name = name,
        None => {}
    }
    let (krate, _) = passes::collapse_docs(krate);
    let (krate, _) = passes::unindent_comments(krate);

    let mut collector = Collector::new(krate.name.to_string(),
                                       libs,
                                       externs,
                                       false);
    collector.fold_crate(krate);

    test_args.insert(0, "rustdoctest".to_string());

    testing::test_main(&test_args,
                       collector.tests.into_iter().collect());
    0
}

fn runtest(test: &str, cratename: &str, libs: SearchPaths,
           externs: core::Externs,
           should_fail: bool, no_run: bool, as_test_harness: bool) {
    // the test harness wants its own `main` & top level functions, so
    // never wrap the test in `fn main() { ... }`
    let test = maketest(test, Some(cratename), true, as_test_harness);
    let input = config::Input::Str(test.to_string());

    let sessopts = config::Options {
        maybe_sysroot: Some(env::current_exe().unwrap().parent().unwrap()
                                              .parent().unwrap().to_path_buf()),
        search_paths: libs,
        crate_types: vec!(config::CrateTypeExecutable),
        output_types: vec!(config::OutputTypeExe),
        externs: externs,
        cg: config::CodegenOptions {
            prefer_dynamic: true,
            .. config::basic_codegen_options()
        },
        test: as_test_harness,
        unstable_features: get_unstable_features_setting(),
        ..config::basic_options().clone()
    };

    // Shuffle around a few input and output handles here. We're going to pass
    // an explicit handle into rustc to collect output messages, but we also
    // want to catch the error message that rustc prints when it fails.
    //
    // We take our task-local stderr (likely set by the test runner), and move
    // it into another task. This helper task then acts as a sink for both the
    // stderr of this task and stderr of rustc itself, copying all the info onto
    // the stderr channel we originally started with.
    //
    // The basic idea is to not use a default_handler() for rustc, and then also
    // not print things by default to the actual stderr.
    let (tx, rx) = channel();
    let w1 = old_io::ChanWriter::new(tx);
    let w2 = w1.clone();
    let old = old_io::stdio::set_stderr(box w1);
    thread::spawn(move || {
        let mut p = old_io::ChanReader::new(rx);
        let mut err = match old {
            Some(old) => {
                // Chop off the `Send` bound.
                let old: Box<Writer> = old;
                old
            }
            None => box old_io::stderr() as Box<Writer>,
        };
        old_io::util::copy(&mut p, &mut err).unwrap();
    });
    let emitter = diagnostic::EmitterWriter::new(box w2, None);

    // Compile the code
    let codemap = CodeMap::new();
    let diagnostic_handler = diagnostic::mk_handler(true, box emitter);
    let span_diagnostic_handler =
        diagnostic::mk_span_handler(diagnostic_handler, codemap);

    let sess = session::build_session_(sessopts,
                                       None,
                                       span_diagnostic_handler);
    rustc_lint::register_builtins(&mut sess.lint_store.borrow_mut(), Some(&sess));

    let outdir = TempDir::new("rustdoctest").ok().expect("rustdoc needs a tempdir");
    let out = Some(outdir.path().to_path_buf());
    let cfg = config::build_configuration(&sess);
    let libdir = sess.target_filesearch(PathKind::All).get_lib_path();
    let mut control = driver::CompileController::basic();
    if no_run {
        control.after_analysis.stop = Compilation::Stop;
    }
    driver::compile_input(sess, cfg, &input, &out, &None, None, control);

    if no_run { return }

    // Run the code!
    //
    // We're careful to prepend the *target* dylib search path to the child's
    // environment to ensure that the target loads the right libraries at
    // runtime. It would be a sad day if the *host* libraries were loaded as a
    // mistake.
    let mut cmd = Command::new(&outdir.path().join("rust-out"));
    let var = DynamicLibrary::envvar();
    let newpath = {
        let path = env::var_os(var).unwrap_or(OsString::new());
        let mut path = env::split_paths(&path).collect::<Vec<_>>();
        path.insert(0, libdir.clone());
        env::join_paths(path.iter()).unwrap()
    };
    cmd.env(var, &newpath);

    match cmd.output() {
        Err(e) => panic!("couldn't run the test: {}{}", e,
                        if e.kind() == io::ErrorKind::PermissionDenied {
                            " - maybe your tempdir is mounted with noexec?"
                        } else { "" }),
        Ok(out) => {
            if should_fail && out.status.success() {
                panic!("test executable succeeded when it should have failed");
            } else if !should_fail && !out.status.success() {
                panic!("test executable failed:\n{:?}",
                      str::from_utf8(&out.stdout));
            }
        }
    }
}

pub fn maketest(s: &str, cratename: Option<&str>, lints: bool, dont_insert_main: bool) -> String {
    let mut prog = String::new();
    if lints {
        prog.push_str(r"
#![allow(unused_variables, unused_assignments, unused_mut, unused_attributes, dead_code)]
");
    }

    // Don't inject `extern crate std` because it's already injected by the
    // compiler.
    if !s.contains("extern crate") && cratename != Some("std") {
        match cratename {
            Some(cratename) => {
                if s.contains(cratename) {
                    prog.push_str(&format!("extern crate {};\n",
                                           cratename));
                }
            }
            None => {}
        }
    }
    if dont_insert_main || s.contains("fn main") {
        prog.push_str(s);
    } else {
        prog.push_str("fn main() {\n    ");
        prog.push_str(&s.replace("\n", "\n    "));
        prog.push_str("\n}");
    }

    return prog
}

pub struct Collector {
    pub tests: Vec<testing::TestDescAndFn>,
    names: Vec<String>,
    libs: SearchPaths,
    externs: core::Externs,
    cnt: uint,
    use_headers: bool,
    current_header: Option<String>,
    cratename: String,
}

impl Collector {
    pub fn new(cratename: String, libs: SearchPaths, externs: core::Externs,
               use_headers: bool) -> Collector {
        Collector {
            tests: Vec::new(),
            names: Vec::new(),
            libs: libs,
            externs: externs,
            cnt: 0,
            use_headers: use_headers,
            current_header: None,
            cratename: cratename,
        }
    }

    pub fn add_test(&mut self, test: String,
                    should_fail: bool, no_run: bool, should_ignore: bool, as_test_harness: bool) {
        let name = if self.use_headers {
            let s = self.current_header.as_ref().map(|s| &**s).unwrap_or("");
            format!("{}_{}", s, self.cnt)
        } else {
            format!("{}_{}", self.names.connect("::"), self.cnt)
        };
        self.cnt += 1;
        let libs = self.libs.clone();
        let externs = self.externs.clone();
        let cratename = self.cratename.to_string();
        debug!("Creating test {}: {}", name, test);
        self.tests.push(testing::TestDescAndFn {
            desc: testing::TestDesc {
                name: testing::DynTestName(name),
                ignore: should_ignore,
                should_fail: testing::ShouldFail::No, // compiler failures are test failures
            },
            testfn: testing::DynTestFn(Thunk::new(move|| {
                runtest(&test,
                        &cratename,
                        libs,
                        externs,
                        should_fail,
                        no_run,
                        as_test_harness);
            }))
        });
    }

    pub fn register_header(&mut self, name: &str, level: u32) {
        if self.use_headers && level == 1 {
            // we use these headings as test names, so it's good if
            // they're valid identifiers.
            let name = name.chars().enumerate().map(|(i, c)| {
                    if (i == 0 && c.is_xid_start()) ||
                        (i != 0 && c.is_xid_continue()) {
                        c
                    } else {
                        '_'
                    }
                }).collect::<String>();

            // new header => reset count.
            self.cnt = 0;
            self.current_header = Some(name);
        }
    }
}

impl DocFolder for Collector {
    fn fold_item(&mut self, item: clean::Item) -> Option<clean::Item> {
        let pushed = match item.name {
            Some(ref name) if name.len() == 0 => false,
            Some(ref name) => { self.names.push(name.to_string()); true }
            None => false
        };
        match item.doc_value() {
            Some(doc) => {
                self.cnt = 0;
                markdown::find_testable_code(doc, &mut *self);
            }
            None => {}
        }
        let ret = self.fold_item_recur(item);
        if pushed {
            self.names.pop();
        }
        return ret;
    }
}
