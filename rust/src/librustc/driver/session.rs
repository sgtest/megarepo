// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


use back::target_strs;
use back;
use driver::driver::host_triple;
use metadata::filesearch;
use metadata;
use middle::lint;

use syntax::attr::AttrMetaMethods;
use syntax::ast::NodeId;
use syntax::ast::{IntTy, UintTy};
use syntax::codemap::Span;
use syntax::diagnostic;
use syntax::parse::ParseSess;
use syntax::{abi, ast, codemap};
use syntax;

use std::cell::{Cell, RefCell};
use std::hashmap::{HashMap,HashSet};

pub struct Config {
    os: abi::Os,
    arch: abi::Architecture,
    target_strs: target_strs::t,
    int_type: IntTy,
    uint_type: UintTy,
}

macro_rules! debugging_opts(
    ([ $opt:ident ] $cnt:expr ) => (
        pub static $opt: u64 = 1 << $cnt;
    );
    ([ $opt:ident, $($rest:ident),* ] $cnt:expr ) => (
        pub static $opt: u64 = 1 << $cnt;
        debugging_opts!([ $($rest),* ] $cnt + 1)
    )
)

debugging_opts!(
    [
        VERBOSE,
        TIME_PASSES,
        COUNT_LLVM_INSNS,
        TIME_LLVM_PASSES,
        TRANS_STATS,
        ASM_COMMENTS,
        NO_VERIFY,
        BORROWCK_STATS,
        NO_LANDING_PADS,
        DEBUG_LLVM,
        SHOW_SPAN,
        COUNT_TYPE_SIZES,
        META_STATS,
        NO_OPT,
        GC,
        PRINT_LINK_ARGS,
        PRINT_LLVM_PASSES,
        LTO,
        AST_JSON,
        AST_JSON_NOEXPAND
    ]
    0
)

pub fn debugging_opts_map() -> ~[(&'static str, &'static str, u64)] {
    ~[("verbose", "in general, enable more debug printouts", VERBOSE),
     ("time-passes", "measure time of each rustc pass", TIME_PASSES),
     ("count-llvm-insns", "count where LLVM \
                           instrs originate", COUNT_LLVM_INSNS),
     ("time-llvm-passes", "measure time of each LLVM pass",
      TIME_LLVM_PASSES),
     ("trans-stats", "gather trans statistics", TRANS_STATS),
     ("asm-comments", "generate comments into the assembly (may change behavior)",
      ASM_COMMENTS),
     ("no-verify", "skip LLVM verification", NO_VERIFY),
     ("borrowck-stats", "gather borrowck statistics",  BORROWCK_STATS),
     ("no-landing-pads", "omit landing pads for unwinding",
      NO_LANDING_PADS),
     ("debug-llvm", "enable debug output from LLVM", DEBUG_LLVM),
     ("show-span", "show spans for compiler debugging", SHOW_SPAN),
     ("count-type-sizes", "count the sizes of aggregate types",
      COUNT_TYPE_SIZES),
     ("meta-stats", "gather metadata statistics", META_STATS),
     ("no-opt", "do not optimize, even if -O is passed", NO_OPT),
     ("print-link-args", "Print the arguments passed to the linker",
      PRINT_LINK_ARGS),
     ("gc", "Garbage collect shared data (experimental)", GC),
     ("print-llvm-passes",
      "Prints the llvm optimization passes being run",
      PRINT_LLVM_PASSES),
     ("lto", "Perform LLVM link-time optimizations", LTO),
     ("ast-json", "Print the AST as JSON and halt", AST_JSON),
     ("ast-json-noexpand", "Print the pre-expansion AST as JSON and halt", AST_JSON_NOEXPAND),
    ]
}

#[deriving(Clone, Eq)]
pub enum OptLevel {
    No, // -O0
    Less, // -O1
    Default, // -O2
    Aggressive // -O3
}

#[deriving(Clone)]
pub struct Options {
    // The crate config requested for the session, which may be combined
    // with additional crate configurations during the compile process
    crate_types: ~[CrateType],

    gc: bool,
    optimize: OptLevel,
    debuginfo: bool,
    lint_opts: ~[(lint::Lint, lint::level)],
    output_types: ~[back::link::OutputType],
    // This was mutable for rustpkg, which updates search paths based on the
    // parsed code. It remains mutable in case its replacements wants to use
    // this.
    addl_lib_search_paths: @RefCell<HashSet<Path>>,
    maybe_sysroot: Option<@Path>,
    target_triple: ~str,
    // User-specified cfg meta items. The compiler itself will add additional
    // items to the crate config, and during parsing the entire crate config
    // will be added to the crate AST node.  This should not be used for
    // anything except building the full crate config prior to parsing.
    cfg: ast::CrateConfig,
    test: bool,
    parse_only: bool,
    no_trans: bool,
    no_analysis: bool,
    debugging_opts: u64,
    /// Whether to write dependency files. It's (enabled, optional filename).
    write_dependency_info: (bool, Option<Path>),
    /// Crate id-related things to maybe print. It's (crate_id, crate_name, crate_file_name).
    print_metas: (bool, bool, bool),
    cg: CodegenOptions,
}

// The type of entry function, so
// users can have their own entry
// functions that don't start a
// scheduler
#[deriving(Eq)]
pub enum EntryFnType {
    EntryMain,
    EntryStart,
    EntryNone,
}

#[deriving(Eq, Clone, TotalOrd, TotalEq)]
pub enum CrateType {
    CrateTypeExecutable,
    CrateTypeDylib,
    CrateTypeRlib,
    CrateTypeStaticlib,
}

pub struct Session_ {
    targ_cfg: @Config,
    opts: @Options,
    cstore: @metadata::cstore::CStore,
    parse_sess: @ParseSess,
    codemap: @codemap::CodeMap,
    // For a library crate, this is always none
    entry_fn: RefCell<Option<(NodeId, codemap::Span)>>,
    entry_type: Cell<Option<EntryFnType>>,
    span_diagnostic: @diagnostic::SpanHandler,
    macro_registrar_fn: RefCell<Option<ast::DefId>>,
    filesearch: @filesearch::FileSearch,
    building_library: Cell<bool>,
    // The name of the root source file of the crate, in the local file system. The path is always
    // expected to be absolute. `None` means that there is no source file.
    local_crate_source_file: Option<Path>,
    working_dir: Path,
    lints: RefCell<HashMap<ast::NodeId,
                           ~[(lint::Lint, codemap::Span, ~str)]>>,
    node_id: Cell<ast::NodeId>,
    crate_types: @RefCell<~[CrateType]>,
}

pub type Session = @Session_;

impl Session_ {
    pub fn span_fatal(&self, sp: Span, msg: &str) -> ! {
        self.span_diagnostic.span_fatal(sp, msg)
    }
    pub fn fatal(&self, msg: &str) -> ! {
        self.span_diagnostic.handler().fatal(msg)
    }
    pub fn span_err(&self, sp: Span, msg: &str) {
        self.span_diagnostic.span_err(sp, msg)
    }
    pub fn err(&self, msg: &str) {
        self.span_diagnostic.handler().err(msg)
    }
    pub fn err_count(&self) -> uint {
        self.span_diagnostic.handler().err_count()
    }
    pub fn has_errors(&self) -> bool {
        self.span_diagnostic.handler().has_errors()
    }
    pub fn abort_if_errors(&self) {
        self.span_diagnostic.handler().abort_if_errors()
    }
    pub fn span_warn(&self, sp: Span, msg: &str) {
        self.span_diagnostic.span_warn(sp, msg)
    }
    pub fn warn(&self, msg: &str) {
        self.span_diagnostic.handler().warn(msg)
    }
    pub fn span_note(&self, sp: Span, msg: &str) {
        self.span_diagnostic.span_note(sp, msg)
    }
    pub fn span_end_note(&self, sp: Span, msg: &str) {
        self.span_diagnostic.span_end_note(sp, msg)
    }
    pub fn note(&self, msg: &str) {
        self.span_diagnostic.handler().note(msg)
    }
    pub fn span_bug(&self, sp: Span, msg: &str) -> ! {
        self.span_diagnostic.span_bug(sp, msg)
    }
    pub fn bug(&self, msg: &str) -> ! {
        self.span_diagnostic.handler().bug(msg)
    }
    pub fn span_unimpl(&self, sp: Span, msg: &str) -> ! {
        self.span_diagnostic.span_unimpl(sp, msg)
    }
    pub fn unimpl(&self, msg: &str) -> ! {
        self.span_diagnostic.handler().unimpl(msg)
    }
    pub fn add_lint(&self,
                    lint: lint::Lint,
                    id: ast::NodeId,
                    sp: Span,
                    msg: ~str) {
        let mut lints = self.lints.borrow_mut();
        match lints.get().find_mut(&id) {
            Some(arr) => { arr.push((lint, sp, msg)); return; }
            None => {}
        }
        lints.get().insert(id, ~[(lint, sp, msg)]);
    }
    pub fn next_node_id(&self) -> ast::NodeId {
        self.reserve_node_ids(1)
    }
    pub fn reserve_node_ids(&self, count: ast::NodeId) -> ast::NodeId {
        let v = self.node_id.get();

        match v.checked_add(&count) {
            Some(next) => { self.node_id.set(next); }
            None => self.bug("Input too large, ran out of node ids!")
        }

        v
    }
    pub fn diagnostic(&self) -> @diagnostic::SpanHandler {
        self.span_diagnostic
    }
    pub fn debugging_opt(&self, opt: u64) -> bool {
        (self.opts.debugging_opts & opt) != 0
    }
    // This exists to help with refactoring to eliminate impossible
    // cases later on
    pub fn impossible_case(&self, sp: Span, msg: &str) -> ! {
        self.span_bug(sp, format!("impossible case reached: {}", msg));
    }
    pub fn verbose(&self) -> bool { self.debugging_opt(VERBOSE) }
    pub fn time_passes(&self) -> bool { self.debugging_opt(TIME_PASSES) }
    pub fn count_llvm_insns(&self) -> bool {
        self.debugging_opt(COUNT_LLVM_INSNS)
    }
    pub fn count_type_sizes(&self) -> bool {
        self.debugging_opt(COUNT_TYPE_SIZES)
    }
    pub fn time_llvm_passes(&self) -> bool {
        self.debugging_opt(TIME_LLVM_PASSES)
    }
    pub fn trans_stats(&self) -> bool { self.debugging_opt(TRANS_STATS) }
    pub fn meta_stats(&self) -> bool { self.debugging_opt(META_STATS) }
    pub fn asm_comments(&self) -> bool { self.debugging_opt(ASM_COMMENTS) }
    pub fn no_verify(&self) -> bool { self.debugging_opt(NO_VERIFY) }
    pub fn borrowck_stats(&self) -> bool { self.debugging_opt(BORROWCK_STATS) }
    pub fn print_llvm_passes(&self) -> bool {
        self.debugging_opt(PRINT_LLVM_PASSES)
    }
    pub fn lto(&self) -> bool {
        self.debugging_opt(LTO)
    }
    pub fn no_landing_pads(&self) -> bool {
        self.debugging_opt(NO_LANDING_PADS)
    }
    pub fn show_span(&self) -> bool {
        self.debugging_opt(SHOW_SPAN)
    }
}

/// Some reasonable defaults
pub fn basic_options() -> @Options {
    @Options {
        crate_types: ~[],
        gc: false,
        optimize: No,
        debuginfo: false,
        lint_opts: ~[],
        output_types: ~[],
        addl_lib_search_paths: @RefCell::new(HashSet::new()),
        maybe_sysroot: None,
        target_triple: host_triple(),
        cfg: ~[],
        test: false,
        parse_only: false,
        no_trans: false,
        no_analysis: false,
        debugging_opts: 0,
        write_dependency_info: (false, None),
        print_metas: (false, false, false),
        cg: basic_codegen_options(),
    }
}

/// Declare a macro that will define all CodegenOptions fields and parsers all
/// at once. The goal of this macro is to define an interface that can be
/// programmatically used by the option parser in order to initialize the struct
/// without hardcoding field names all over the place.
///
/// The goal is to invoke this macro once with the correct fields, and then this
/// macro generates all necessary code. The main gotcha of this macro is the
/// cgsetters module which is a bunch of generated code to parse an option into
/// its respective field in the struct. There are a few hand-written parsers for
/// parsing specific types of values in this module.
macro_rules! cgoptions(
    ($($opt:ident : $t:ty = ($init:expr, $parse:ident, $desc:expr)),* ,) =>
(
    #[deriving(Clone)]
    pub struct CodegenOptions { $($opt: $t),* }

    pub fn basic_codegen_options() -> CodegenOptions {
        CodegenOptions { $($opt: $init),* }
    }

    pub type CodegenSetter = fn(&mut CodegenOptions, v: Option<&str>) -> bool;
    pub static CG_OPTIONS: &'static [(&'static str, CodegenSetter,
                                      &'static str)] =
        &[ $( (stringify!($opt), cgsetters::$opt, $desc) ),* ];

    mod cgsetters {
        use super::CodegenOptions;

        $(
            pub fn $opt(cg: &mut CodegenOptions, v: Option<&str>) -> bool {
                $parse(&mut cg.$opt, v)
            }
        )*

        fn parse_bool(slot: &mut bool, v: Option<&str>) -> bool {
            match v {
                Some(..) => false,
                None => { *slot = true; true }
            }
        }

        fn parse_opt_string(slot: &mut Option<~str>, v: Option<&str>) -> bool {
            match v {
                Some(s) => { *slot = Some(s.to_owned()); true },
                None => false,
            }
        }

        fn parse_string(slot: &mut ~str, v: Option<&str>) -> bool {
            match v {
                Some(s) => { *slot = s.to_owned(); true },
                None => false,
            }
        }

        fn parse_list(slot: &mut ~[~str], v: Option<&str>) -> bool {
            match v {
                Some(s) => {
                    for s in s.words() {
                        slot.push(s.to_owned());
                    }
                    true
                },
                None => false,
            }
        }

    }
) )

cgoptions!(
    ar: Option<~str> = (None, parse_opt_string,
        "tool to assemble archives with"),
    linker: Option<~str> = (None, parse_opt_string,
        "system linker to link outputs with"),
    link_args: ~[~str] = (~[], parse_list,
        "extra arguments to pass to the linker (space separated)"),
    target_cpu: ~str = (~"generic", parse_string,
        "select target processor (llc -mcpu=help for details)"),
    target_feature: ~str = (~"", parse_string,
        "target specific attributes (llc -mattr=help for details)"),
    passes: ~[~str] = (~[], parse_list,
        "a list of extra LLVM passes to run (space separated)"),
    llvm_args: ~[~str] = (~[], parse_list,
        "a list of arguments to pass to llvm (space separated)"),
    save_temps: bool = (false, parse_bool,
        "save all temporary output files during compilation"),
    android_cross_path: Option<~str> = (None, parse_opt_string,
        "the path to the Android NDK"),
    no_rpath: bool = (false, parse_bool,
        "disables setting the rpath in libs/exes"),
    no_prepopulate_passes: bool = (false, parse_bool,
        "don't pre-populate the pass manager with a list of passes"),
    no_vectorize_loops: bool = (false, parse_bool,
        "don't run the loop vectorization optimization passes"),
    no_vectorize_slp: bool = (false, parse_bool,
        "don't run LLVM's SLP vectorization pass"),
    soft_float: bool = (false, parse_bool,
        "generate software floating point library calls"),
    gen_crate_map: bool = (false, parse_bool,
        "force generation of a toplevel crate map"),
    prefer_dynamic: bool = (false, parse_bool,
        "prefer dynamic linking to static linking"),
    no_integrated_as: bool = (false, parse_bool,
        "use an external assembler rather than LLVM's integrated one"),
)

// Seems out of place, but it uses session, so I'm putting it here
pub fn expect<T:Clone>(sess: Session, opt: Option<T>, msg: || -> ~str) -> T {
    diagnostic::expect(sess.diagnostic(), opt, msg)
}

pub fn building_library(options: &Options, krate: &ast::Crate) -> bool {
    if options.test { return false }
    for output in options.crate_types.iter() {
        match *output {
            CrateTypeExecutable => {}
            CrateTypeStaticlib | CrateTypeDylib | CrateTypeRlib => return true
        }
    }
    match syntax::attr::first_attr_value_str_by_name(krate.attrs, "crate_type") {
        Some(s) => {
            s.equiv(&("lib")) ||
            s.equiv(&("rlib")) ||
            s.equiv(&("dylib")) ||
            s.equiv(&("staticlib"))
        }
        _ => false
    }
}

pub fn default_lib_output() -> CrateType {
    CrateTypeRlib
}

pub fn collect_crate_types(session: &Session,
                           attrs: &[ast::Attribute]) -> ~[CrateType] {
    // If we're generating a test executable, then ignore all other output
    // styles at all other locations
    if session.opts.test {
        return ~[CrateTypeExecutable];
    }
    let mut base = session.opts.crate_types.clone();
    let mut iter = attrs.iter().filter_map(|a| {
        if a.name().equiv(&("crate_type")) {
            match a.value_str() {
                Some(ref n) if n.equiv(&("rlib")) => Some(CrateTypeRlib),
                Some(ref n) if n.equiv(&("dylib")) => Some(CrateTypeDylib),
                Some(ref n) if n.equiv(&("lib")) => {
                    Some(default_lib_output())
                }
                Some(ref n) if n.equiv(&("staticlib")) => {
                    Some(CrateTypeStaticlib)
                }
                Some(ref n) if n.equiv(&("bin")) => Some(CrateTypeExecutable),
                Some(_) => {
                    session.add_lint(lint::UnknownCrateType,
                                     ast::CRATE_NODE_ID,
                                     a.span,
                                     ~"invalid `crate_type` value");
                    None
                }
                _ => {
                    session.add_lint(lint::UnknownCrateType, ast::CRATE_NODE_ID,
                                    a.span, ~"`crate_type` requires a value");
                    None
                }
            }
        } else {
            None
        }
    });
    base.extend(&mut iter);
    if base.len() == 0 {
        base.push(CrateTypeExecutable);
    }
    base.sort();
    base.dedup();
    return base;
}

pub fn sess_os_to_meta_os(os: abi::Os) -> metadata::loader::Os {
    use metadata::loader;

    match os {
        abi::OsWin32 => loader::OsWin32,
        abi::OsLinux => loader::OsLinux,
        abi::OsAndroid => loader::OsAndroid,
        abi::OsMacos => loader::OsMacos,
        abi::OsFreebsd => loader::OsFreebsd
    }
}
