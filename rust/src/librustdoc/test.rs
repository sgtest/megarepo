// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::io::prelude::*;
use std::io;
use std::path::{Path, PathBuf};
use std::panic::{self, AssertUnwindSafe};
use std::process::Command;
use std::rc::Rc;
use std::str;
use std::sync::{Arc, Mutex};

use testing;
use rustc_lint;
use rustc::dep_graph::DepGraph;
use rustc::hir;
use rustc::hir::intravisit;
use rustc::session::{self, CompileIncomplete, config};
use rustc::session::config::{OutputType, OutputTypes, Externs};
use rustc::session::search_paths::{SearchPaths, PathKind};
use rustc_back::dynamic_lib::DynamicLibrary;
use rustc_back::tempdir::TempDir;
use rustc_driver::{self, driver, Compilation};
use rustc_driver::driver::phase_2_configure_and_expand;
use rustc_metadata::cstore::CStore;
use rustc_resolve::MakeGlobMap;
use rustc_trans;
use rustc_trans::back::link;
use syntax::ast;
use syntax::codemap::CodeMap;
use syntax::feature_gate::UnstableFeatures;
use syntax_pos::{BytePos, DUMMY_SP, Pos, Span};
use errors;
use errors::emitter::ColorConfig;

use clean::Attributes;
use html::markdown::{self, RenderType};

#[derive(Clone, Default)]
pub struct TestOptions {
    pub no_crate_inject: bool,
    pub attrs: Vec<String>,
}

pub fn run(input: &str,
           cfgs: Vec<String>,
           libs: SearchPaths,
           externs: Externs,
           mut test_args: Vec<String>,
           crate_name: Option<String>,
           maybe_sysroot: Option<PathBuf>,
           render_type: RenderType,
           display_warnings: bool)
           -> isize {
    let input_path = PathBuf::from(input);
    let input = config::Input::File(input_path.clone());

    let sessopts = config::Options {
        maybe_sysroot: maybe_sysroot.clone().or_else(
            || Some(env::current_exe().unwrap().parent().unwrap().parent().unwrap().to_path_buf())),
        search_paths: libs.clone(),
        crate_types: vec![config::CrateTypeDylib],
        externs: externs.clone(),
        unstable_features: UnstableFeatures::from_environment(),
        actually_rustdoc: true,
        ..config::basic_options().clone()
    };

    let codemap = Rc::new(CodeMap::new(sessopts.file_path_mapping()));
    let handler =
        errors::Handler::with_tty_emitter(ColorConfig::Auto, true, false, Some(codemap.clone()));

    let dep_graph = DepGraph::new(false);
    let _ignore = dep_graph.in_ignore();
    let cstore = Rc::new(CStore::new(&dep_graph, box rustc_trans::LlvmMetadataLoader));
    let mut sess = session::build_session_(
        sessopts, &dep_graph, Some(input_path.clone()), handler, codemap.clone(), cstore.clone(),
    );
    rustc_trans::init(&sess);
    rustc_lint::register_builtins(&mut sess.lint_store.borrow_mut(), Some(&sess));
    sess.parse_sess.config =
        config::build_configuration(&sess, config::parse_cfgspecs(cfgs.clone()));

    let krate = panictry!(driver::phase_1_parse_input(&sess, &input));
    let driver::ExpansionResult { defs, mut hir_forest, .. } = {
        phase_2_configure_and_expand(
            &sess, &cstore, krate, None, "rustdoc-test", None, MakeGlobMap::No, |_| Ok(())
        ).expect("phase_2_configure_and_expand aborted in rustdoc!")
    };

    let crate_name = crate_name.unwrap_or_else(|| {
        link::find_crate_name(None, &hir_forest.krate().attrs, &input)
    });
    let opts = scrape_test_config(hir_forest.krate());
    let mut collector = Collector::new(crate_name,
                                       cfgs,
                                       libs,
                                       externs,
                                       false,
                                       opts,
                                       maybe_sysroot,
                                       Some(codemap),
                                       None,
                                       render_type);

    {
        let dep_graph = DepGraph::new(false);
        let _ignore = dep_graph.in_ignore();
        let map = hir::map::map_crate(&mut hir_forest, defs);
        let krate = map.krate();
        let mut hir_collector = HirCollector {
            collector: &mut collector,
            map: &map
        };
        hir_collector.visit_testable("".to_string(), &krate.attrs, |this| {
            intravisit::walk_crate(this, krate);
        });
    }

    test_args.insert(0, "rustdoctest".to_string());

    testing::test_main(&test_args,
                       collector.tests.into_iter().collect(),
                       testing::Options::new().display_output(display_warnings));
    0
}

// Look for #![doc(test(no_crate_inject))], used by crates in the std facade
fn scrape_test_config(krate: &::rustc::hir::Crate) -> TestOptions {
    use syntax::print::pprust;

    let mut opts = TestOptions {
        no_crate_inject: false,
        attrs: Vec::new(),
    };

    let test_attrs: Vec<_> = krate.attrs.iter()
        .filter(|a| a.check_name("doc"))
        .flat_map(|a| a.meta_item_list().unwrap_or_else(Vec::new))
        .filter(|a| a.check_name("test"))
        .collect();
    let attrs = test_attrs.iter().flat_map(|a| a.meta_item_list().unwrap_or(&[]));

    for attr in attrs {
        if attr.check_name("no_crate_inject") {
            opts.no_crate_inject = true;
        }
        if attr.check_name("attr") {
            if let Some(l) = attr.meta_item_list() {
                for item in l {
                    opts.attrs.push(pprust::meta_list_item_to_string(item));
                }
            }
        }
    }

    opts
}

fn runtest(test: &str, cratename: &str, cfgs: Vec<String>, libs: SearchPaths,
           externs: Externs,
           should_panic: bool, no_run: bool, as_test_harness: bool,
           compile_fail: bool, mut error_codes: Vec<String>, opts: &TestOptions,
           maybe_sysroot: Option<PathBuf>) {
    // the test harness wants its own `main` & top level functions, so
    // never wrap the test in `fn main() { ... }`
    let test = maketest(test, Some(cratename), as_test_harness, opts);
    let input = config::Input::Str {
        name: driver::anon_src(),
        input: test.to_owned(),
    };
    let outputs = OutputTypes::new(&[(OutputType::Exe, None)]);

    let sessopts = config::Options {
        maybe_sysroot: maybe_sysroot.or_else(
            || Some(env::current_exe().unwrap().parent().unwrap().parent().unwrap().to_path_buf())),
        search_paths: libs,
        crate_types: vec![config::CrateTypeExecutable],
        output_types: outputs,
        externs: externs,
        cg: config::CodegenOptions {
            prefer_dynamic: true,
            .. config::basic_codegen_options()
        },
        test: as_test_harness,
        unstable_features: UnstableFeatures::from_environment(),
        ..config::basic_options().clone()
    };

    // Shuffle around a few input and output handles here. We're going to pass
    // an explicit handle into rustc to collect output messages, but we also
    // want to catch the error message that rustc prints when it fails.
    //
    // We take our thread-local stderr (likely set by the test runner) and replace
    // it with a sink that is also passed to rustc itself. When this function
    // returns the output of the sink is copied onto the output of our own thread.
    //
    // The basic idea is to not use a default Handler for rustc, and then also
    // not print things by default to the actual stderr.
    struct Sink(Arc<Mutex<Vec<u8>>>);
    impl Write for Sink {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            Write::write(&mut *self.0.lock().unwrap(), data)
        }
        fn flush(&mut self) -> io::Result<()> { Ok(()) }
    }
    struct Bomb(Arc<Mutex<Vec<u8>>>, Box<Write+Send>);
    impl Drop for Bomb {
        fn drop(&mut self) {
            let _ = self.1.write_all(&self.0.lock().unwrap());
        }
    }
    let data = Arc::new(Mutex::new(Vec::new()));
    let codemap = Rc::new(CodeMap::new(sessopts.file_path_mapping()));
    let emitter = errors::emitter::EmitterWriter::new(box Sink(data.clone()),
                                                      Some(codemap.clone()));
    let old = io::set_panic(Some(box Sink(data.clone())));
    let _bomb = Bomb(data.clone(), old.unwrap_or(box io::stdout()));

    // Compile the code
    let diagnostic_handler = errors::Handler::with_emitter(true, false, box emitter);

    let dep_graph = DepGraph::new(false);
    let cstore = Rc::new(CStore::new(&dep_graph, box rustc_trans::LlvmMetadataLoader));
    let mut sess = session::build_session_(
        sessopts, &dep_graph, None, diagnostic_handler, codemap, cstore.clone(),
    );
    rustc_trans::init(&sess);
    rustc_lint::register_builtins(&mut sess.lint_store.borrow_mut(), Some(&sess));

    let outdir = Mutex::new(TempDir::new("rustdoctest").ok().expect("rustdoc needs a tempdir"));
    let libdir = sess.target_filesearch(PathKind::All).get_lib_path();
    let mut control = driver::CompileController::basic();
    sess.parse_sess.config =
        config::build_configuration(&sess, config::parse_cfgspecs(cfgs.clone()));
    let out = Some(outdir.lock().unwrap().path().to_path_buf());

    if no_run {
        control.after_analysis.stop = Compilation::Stop;
    }

    let res = panic::catch_unwind(AssertUnwindSafe(|| {
        driver::compile_input(&sess, &cstore, &input, &out, &None, None, &control)
    }));

    let compile_result = match res {
        Ok(Ok(())) | Ok(Err(CompileIncomplete::Stopped)) => Ok(()),
        Err(_) | Ok(Err(CompileIncomplete::Errored(_))) => Err(())
    };

    match (compile_result, compile_fail) {
        (Ok(()), true) => {
            panic!("test compiled while it wasn't supposed to")
        }
        (Ok(()), false) => {}
        (Err(()), true) => {
            if error_codes.len() > 0 {
                let out = String::from_utf8(data.lock().unwrap().to_vec()).unwrap();
                error_codes.retain(|err| !out.contains(err));
            }
        }
        (Err(()), false) => {
            panic!("couldn't compile the test")
        }
    }

    if error_codes.len() > 0 {
        panic!("Some expected error codes were not found: {:?}", error_codes);
    }

    if no_run { return }

    // Run the code!
    //
    // We're careful to prepend the *target* dylib search path to the child's
    // environment to ensure that the target loads the right libraries at
    // runtime. It would be a sad day if the *host* libraries were loaded as a
    // mistake.
    let mut cmd = Command::new(&outdir.lock().unwrap().path().join("rust_out"));
    let var = DynamicLibrary::envvar();
    let newpath = {
        let path = env::var_os(var).unwrap_or(OsString::new());
        let mut path = env::split_paths(&path).collect::<Vec<_>>();
        path.insert(0, libdir.clone());
        env::join_paths(path).unwrap()
    };
    cmd.env(var, &newpath);

    match cmd.output() {
        Err(e) => panic!("couldn't run the test: {}{}", e,
                        if e.kind() == io::ErrorKind::PermissionDenied {
                            " - maybe your tempdir is mounted with noexec?"
                        } else { "" }),
        Ok(out) => {
            if should_panic && out.status.success() {
                panic!("test executable succeeded when it should have failed");
            } else if !should_panic && !out.status.success() {
                panic!("test executable failed:\n{}\n{}\n",
                       str::from_utf8(&out.stdout).unwrap_or(""),
                       str::from_utf8(&out.stderr).unwrap_or(""));
            }
        }
    }
}

pub fn maketest(s: &str, cratename: Option<&str>, dont_insert_main: bool,
                opts: &TestOptions) -> String {
    let (crate_attrs, everything_else) = partition_source(s);

    let mut prog = String::new();

    // First push any outer attributes from the example, assuming they
    // are intended to be crate attributes.
    prog.push_str(&crate_attrs);

    // Next, any attributes for other aspects such as lints.
    for attr in &opts.attrs {
        prog.push_str(&format!("#![{}]\n", attr));
    }

    // Don't inject `extern crate std` because it's already injected by the
    // compiler.
    if !s.contains("extern crate") && !opts.no_crate_inject && cratename != Some("std") {
        if let Some(cratename) = cratename {
            if s.contains(cratename) {
                prog.push_str(&format!("extern crate {};\n", cratename));
            }
        }
    }
    if dont_insert_main || s.contains("fn main") {
        prog.push_str(&everything_else);
    } else {
        prog.push_str("fn main() {\n");
        prog.push_str(&everything_else);
        prog = prog.trim().into();
        prog.push_str("\n}");
    }

    info!("final test program: {}", prog);

    prog
}

// FIXME(aburka): use a real parser to deal with multiline attributes
fn partition_source(s: &str) -> (String, String) {
    use std_unicode::str::UnicodeStr;

    let mut after_header = false;
    let mut before = String::new();
    let mut after = String::new();

    for line in s.lines() {
        let trimline = line.trim();
        let header = trimline.is_whitespace() ||
            trimline.starts_with("#![");
        if !header || after_header {
            after_header = true;
            after.push_str(line);
            after.push_str("\n");
        } else {
            before.push_str(line);
            before.push_str("\n");
        }
    }

    (before, after)
}

pub struct Collector {
    pub tests: Vec<testing::TestDescAndFn>,
    // to be removed when hoedown will be definitely gone
    pub old_tests: HashMap<String, Vec<String>>,
    names: Vec<String>,
    cfgs: Vec<String>,
    libs: SearchPaths,
    externs: Externs,
    cnt: usize,
    use_headers: bool,
    current_header: Option<String>,
    cratename: String,
    opts: TestOptions,
    maybe_sysroot: Option<PathBuf>,
    position: Span,
    codemap: Option<Rc<CodeMap>>,
    filename: Option<String>,
    // to be removed when hoedown will be removed as well
    pub render_type: RenderType,
}

impl Collector {
    pub fn new(cratename: String, cfgs: Vec<String>, libs: SearchPaths, externs: Externs,
               use_headers: bool, opts: TestOptions, maybe_sysroot: Option<PathBuf>,
               codemap: Option<Rc<CodeMap>>, filename: Option<String>,
               render_type: RenderType) -> Collector {
        Collector {
            tests: Vec::new(),
            old_tests: HashMap::new(),
            names: Vec::new(),
            cfgs: cfgs,
            libs: libs,
            externs: externs,
            cnt: 0,
            use_headers: use_headers,
            current_header: None,
            cratename: cratename,
            opts: opts,
            maybe_sysroot: maybe_sysroot,
            position: DUMMY_SP,
            codemap: codemap,
            filename: filename,
            render_type: render_type,
        }
    }

    fn generate_name(&self, line: usize, filename: &str) -> String {
        if self.use_headers {
            if let Some(ref header) = self.current_header {
                format!("{} - {} (line {})", filename, header, line)
            } else {
                format!("{} - (line {})", filename, line)
            }
        } else {
            format!("{} - {} (line {})", filename, self.names.join("::"), line)
        }
    }

    // to be removed once hoedown is gone
    fn generate_name_beginning(&self, filename: &str) -> String {
        if self.use_headers {
            if let Some(ref header) = self.current_header {
                format!("{} - {} (line", filename, header)
            } else {
                format!("{} - (line", filename)
            }
        } else {
            format!("{} - {} (line", filename, self.names.join("::"))
        }
    }

    pub fn add_old_test(&mut self, test: String, filename: String) {
        let name_beg = self.generate_name_beginning(&filename);
        let entry = self.old_tests.entry(name_beg)
                                  .or_insert(Vec::new());
        entry.push(test.trim().to_owned());
    }

    pub fn add_test(&mut self, test: String,
                    should_panic: bool, no_run: bool, should_ignore: bool,
                    as_test_harness: bool, compile_fail: bool, error_codes: Vec<String>,
                    line: usize, filename: String, allow_fail: bool) {
        let name = self.generate_name(line, &filename);
        // to be removed when hoedown is removed
        if self.render_type == RenderType::Pulldown {
            let name_beg = self.generate_name_beginning(&filename);
            let mut found = false;
            let test = test.trim().to_owned();
            if let Some(entry) = self.old_tests.get_mut(&name_beg) {
                found = entry.remove_item(&test).is_some();
            }
            if !found {
                let _ = writeln!(&mut io::stderr(),
                                 "WARNING: {} Code block is not currently run as a test, but will \
                                  in future versions of rustdoc. Please ensure this code block is \
                                  a runnable test, or use the `ignore` directive.",
                                 name);
                return
            }
        }
        let cfgs = self.cfgs.clone();
        let libs = self.libs.clone();
        let externs = self.externs.clone();
        let cratename = self.cratename.to_string();
        let opts = self.opts.clone();
        let maybe_sysroot = self.maybe_sysroot.clone();
        debug!("Creating test {}: {}", name, test);
        self.tests.push(testing::TestDescAndFn {
            desc: testing::TestDesc {
                name: testing::DynTestName(name),
                ignore: should_ignore,
                // compiler failures are test failures
                should_panic: testing::ShouldPanic::No,
                allow_fail: allow_fail,
            },
            testfn: testing::DynTestFn(box move |()| {
                let panic = io::set_panic(None);
                let print = io::set_print(None);
                match {
                    rustc_driver::in_rustc_thread(move || {
                        io::set_panic(panic);
                        io::set_print(print);
                        runtest(&test,
                                &cratename,
                                cfgs,
                                libs,
                                externs,
                                should_panic,
                                no_run,
                                as_test_harness,
                                compile_fail,
                                error_codes,
                                &opts,
                                maybe_sysroot)
                    })
                } {
                    Ok(()) => (),
                    Err(err) => panic::resume_unwind(err),
                }
            }),
        });
    }

    pub fn get_line(&self) -> usize {
        if let Some(ref codemap) = self.codemap {
            let line = self.position.lo.to_usize();
            let line = codemap.lookup_char_pos(BytePos(line as u32)).line;
            if line > 0 { line - 1 } else { line }
        } else {
            0
        }
    }

    pub fn set_position(&mut self, position: Span) {
        self.position = position;
    }

    pub fn get_filename(&self) -> String {
        if let Some(ref codemap) = self.codemap {
            let filename = codemap.span_to_filename(self.position);
            if let Ok(cur_dir) = env::current_dir() {
                if let Ok(path) = Path::new(&filename).strip_prefix(&cur_dir) {
                    if let Some(path) = path.to_str() {
                        return path.to_owned();
                    }
                }
            }
            filename
        } else if let Some(ref filename) = self.filename {
            filename.clone()
        } else {
            "<input>".to_owned()
        }
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

struct HirCollector<'a, 'hir: 'a> {
    collector: &'a mut Collector,
    map: &'a hir::map::Map<'hir>
}

impl<'a, 'hir> HirCollector<'a, 'hir> {
    fn visit_testable<F: FnOnce(&mut Self)>(&mut self,
                                            name: String,
                                            attrs: &[ast::Attribute],
                                            nested: F) {
        let has_name = !name.is_empty();
        if has_name {
            self.collector.names.push(name);
        }

        let mut attrs = Attributes::from_ast(attrs);
        attrs.collapse_doc_comments();
        attrs.unindent_doc_comments();
        if let Some(doc) = attrs.doc_value() {
            self.collector.cnt = 0;
            if self.collector.render_type == RenderType::Pulldown {
                markdown::old_find_testable_code(doc, self.collector,
                                                 attrs.span.unwrap_or(DUMMY_SP));
                markdown::find_testable_code(doc, self.collector,
                                             attrs.span.unwrap_or(DUMMY_SP));
            } else {
                markdown::old_find_testable_code(doc, self.collector,
                                                 attrs.span.unwrap_or(DUMMY_SP));
            }
        }

        nested(self);

        if has_name {
            self.collector.names.pop();
        }
    }
}

impl<'a, 'hir> intravisit::Visitor<'hir> for HirCollector<'a, 'hir> {
    fn nested_visit_map<'this>(&'this mut self) -> intravisit::NestedVisitorMap<'this, 'hir> {
        intravisit::NestedVisitorMap::All(&self.map)
    }

    fn visit_item(&mut self, item: &'hir hir::Item) {
        let name = if let hir::ItemImpl(.., ref ty, _) = item.node {
            self.map.node_to_pretty_string(ty.id)
        } else {
            item.name.to_string()
        };

        self.visit_testable(name, &item.attrs, |this| {
            intravisit::walk_item(this, item);
        });
    }

    fn visit_trait_item(&mut self, item: &'hir hir::TraitItem) {
        self.visit_testable(item.name.to_string(), &item.attrs, |this| {
            intravisit::walk_trait_item(this, item);
        });
    }

    fn visit_impl_item(&mut self, item: &'hir hir::ImplItem) {
        self.visit_testable(item.name.to_string(), &item.attrs, |this| {
            intravisit::walk_impl_item(this, item);
        });
    }

    fn visit_foreign_item(&mut self, item: &'hir hir::ForeignItem) {
        self.visit_testable(item.name.to_string(), &item.attrs, |this| {
            intravisit::walk_foreign_item(this, item);
        });
    }

    fn visit_variant(&mut self,
                     v: &'hir hir::Variant,
                     g: &'hir hir::Generics,
                     item_id: ast::NodeId) {
        self.visit_testable(v.node.name.to_string(), &v.node.attrs, |this| {
            intravisit::walk_variant(this, v, g, item_id);
        });
    }

    fn visit_struct_field(&mut self, f: &'hir hir::StructField) {
        self.visit_testable(f.name.to_string(), &f.attrs, |this| {
            intravisit::walk_struct_field(this, f);
        });
    }

    fn visit_macro_def(&mut self, macro_def: &'hir hir::MacroDef) {
        self.visit_testable(macro_def.name.to_string(), &macro_def.attrs, |_| ());
    }
}
