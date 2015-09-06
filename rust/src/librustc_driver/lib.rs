// Copyright 2014-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! The Rust compiler.
//!
//! # Note
//!
//! This API is completely unstable and subject to change.

// Do not remove on snapshot creation. Needed for bootstrap. (Issue #22364)
#![cfg_attr(stage0, feature(custom_attribute))]
#![crate_name = "rustc_driver"]
#![unstable(feature = "rustc_private", issue = "27812")]
#![staged_api]
#![crate_type = "dylib"]
#![crate_type = "rlib"]
#![doc(html_logo_url = "https://www.rust-lang.org/logos/rust-logo-128x128-blk-v2.png",
      html_favicon_url = "https://doc.rust-lang.org/favicon.ico",
      html_root_url = "https://doc.rust-lang.org/nightly/")]

#![feature(box_syntax)]
#![feature(libc)]
#![feature(quote)]
#![feature(rustc_diagnostic_macros)]
#![feature(rustc_private)]
#![feature(set_stdio)]
#![feature(staged_api)]
#![feature(vec_push_all)]

extern crate arena;
extern crate flate;
extern crate getopts;
extern crate graphviz;
extern crate libc;
extern crate rustc;
extern crate rustc_back;
extern crate rustc_borrowck;
extern crate rustc_front;
extern crate rustc_lint;
extern crate rustc_privacy;
extern crate rustc_mir;
extern crate rustc_resolve;
extern crate rustc_trans;
extern crate rustc_typeck;
extern crate serialize;
extern crate rustc_llvm as llvm;
#[macro_use] extern crate log;
#[macro_use] extern crate syntax;

pub use syntax::diagnostic;

use driver::CompileController;
use pretty::{PpMode, UserIdentifiedItem};

use rustc_resolve as resolve;
use rustc_trans::back::link;
use rustc_trans::save;
use rustc::session::{config, Session, build_session};
use rustc::session::config::{Input, PrintRequest};
use rustc::lint::Lint;
use rustc::lint;
use rustc::metadata;
use rustc::util::common::time;

use std::cmp::Ordering::Equal;
use std::env;
use std::io::{self, Read, Write};
use std::iter::repeat;
use std::path::PathBuf;
use std::process;
use std::str;
use std::sync::{Arc, Mutex};
use std::thread;

use rustc::session::early_error;

use syntax::ast;
use syntax::parse;
use syntax::diagnostic::Emitter;
use syntax::diagnostics;

#[cfg(test)]
pub mod test;

pub mod driver;
pub mod pretty;
pub mod target_features;


const BUG_REPORT_URL: &'static str =
    "https://github.com/rust-lang/rust/blob/master/CONTRIBUTING.md#bug-reports";


pub fn run(args: Vec<String>) -> isize {
    monitor(move || run_compiler(&args, &mut RustcDefaultCalls));
    0
}

// Parse args and run the compiler. This is the primary entry point for rustc.
// See comments on CompilerCalls below for details about the callbacks argument.
pub fn run_compiler<'a>(args: &[String],
                        callbacks: &mut CompilerCalls<'a>) {
    macro_rules! do_or_return {($expr: expr) => {
        match $expr {
            Compilation::Stop => return,
            Compilation::Continue => {}
        }
    }}

    let matches = match handle_options(args.to_vec()) {
        Some(matches) => matches,
        None => return
    };

    let sopts = config::build_session_options(&matches);

    let descriptions = diagnostics_registry();

    do_or_return!(callbacks.early_callback(&matches, &descriptions, sopts.color));

    let (odir, ofile) = make_output(&matches);
    let (input, input_file_path) = match make_input(&matches.free) {
        Some((input, input_file_path)) => callbacks.some_input(input, input_file_path),
        None => match callbacks.no_input(&matches, &sopts, &odir, &ofile, &descriptions) {
            Some((input, input_file_path)) => (input, input_file_path),
            None => return
        }
    };

    let mut sess = build_session(sopts, input_file_path, descriptions);
    rustc_lint::register_builtins(&mut sess.lint_store.borrow_mut(), Some(&sess));
    if sess.unstable_options() {
        sess.opts.show_span = matches.opt_str("show-span");
    }
    let mut cfg = config::build_configuration(&sess);
    target_features::add_configuration(&mut cfg, &sess);

    do_or_return!(callbacks.late_callback(&matches, &sess, &input, &odir, &ofile));

    // It is somewhat unfortunate that this is hardwired in - this is forced by
    // the fact that pretty_print_input requires the session by value.
    let pretty = callbacks.parse_pretty(&sess, &matches);
    match pretty {
        Some((ppm, opt_uii)) => {
            pretty::pretty_print_input(sess, cfg, &input, ppm, opt_uii, ofile);
            return;
        }
        None => {/* continue */ }
    }

    let plugins = sess.opts.debugging_opts.extra_plugins.clone();
    let control = callbacks.build_controller(&sess);
    driver::compile_input(sess, cfg, &input, &odir, &ofile, Some(plugins), control);
}

// Extract output directory and file from matches.
fn make_output(matches: &getopts::Matches) -> (Option<PathBuf>, Option<PathBuf>) {
    let odir = matches.opt_str("out-dir").map(|o| PathBuf::from(&o));
    let ofile = matches.opt_str("o").map(|o| PathBuf::from(&o));
    (odir, ofile)
}

// Extract input (string or file and optional path) from matches.
fn make_input(free_matches: &[String]) -> Option<(Input, Option<PathBuf>)> {
    if free_matches.len() == 1 {
        let ifile = &free_matches[0][..];
        if ifile == "-" {
            let mut src = String::new();
            io::stdin().read_to_string(&mut src).unwrap();
            Some((Input::Str(src), None))
        } else {
            Some((Input::File(PathBuf::from(ifile)), Some(PathBuf::from(ifile))))
        }
    } else {
        None
    }
}

// Whether to stop or continue compilation.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Compilation {
    Stop,
    Continue,
}

impl Compilation {
    pub fn and_then<F: FnOnce() -> Compilation>(self, next: F) -> Compilation {
        match self {
            Compilation::Stop => Compilation::Stop,
            Compilation::Continue => next()
        }
    }
}

// A trait for customising the compilation process. Offers a number of hooks for
// executing custom code or customising input.
pub trait CompilerCalls<'a> {
    // Hook for a callback early in the process of handling arguments. This will
    // be called straight after options have been parsed but before anything
    // else (e.g., selecting input and output).
    fn early_callback(&mut self,
                      _: &getopts::Matches,
                      _: &diagnostics::registry::Registry,
                      _: diagnostic::ColorConfig)
                      -> Compilation {
        Compilation::Continue
    }

    // Hook for a callback late in the process of handling arguments. This will
    // be called just before actual compilation starts (and before build_controller
    // is called), after all arguments etc. have been completely handled.
    fn late_callback(&mut self,
                     _: &getopts::Matches,
                     _: &Session,
                     _: &Input,
                     _: &Option<PathBuf>,
                     _: &Option<PathBuf>)
                     -> Compilation {
        Compilation::Continue
    }

    // Called after we extract the input from the arguments. Gives the implementer
    // an opportunity to change the inputs or to add some custom input handling.
    // The default behaviour is to simply pass through the inputs.
    fn some_input(&mut self, input: Input, input_path: Option<PathBuf>)
                  -> (Input, Option<PathBuf>) {
        (input, input_path)
    }

    // Called after we extract the input from the arguments if there is no valid
    // input. Gives the implementer an opportunity to supply alternate input (by
    // returning a Some value) or to add custom behaviour for this error such as
    // emitting error messages. Returning None will cause compilation to stop
    // at this point.
    fn no_input(&mut self,
                _: &getopts::Matches,
                _: &config::Options,
                _: &Option<PathBuf>,
                _: &Option<PathBuf>,
                _: &diagnostics::registry::Registry)
                -> Option<(Input, Option<PathBuf>)> {
        None
    }

    // Parse pretty printing information from the arguments. The implementer can
    // choose to ignore this (the default will return None) which will skip pretty
    // printing. If you do want to pretty print, it is recommended to use the
    // implementation of this method from RustcDefaultCalls.
    // FIXME, this is a terrible bit of API. Parsing of pretty printing stuff
    // should be done as part of the framework and the implementor should customise
    // handling of it. However, that is not possible atm because pretty printing
    // essentially goes off and takes another path through the compiler which
    // means the session is either moved or not depending on what parse_pretty
    // returns (we could fix this by cloning, but it's another hack). The proper
    // solution is to handle pretty printing as if it were a compiler extension,
    // extending CompileController to make this work (see for example the treatment
    // of save-analysis in RustcDefaultCalls::build_controller).
    fn parse_pretty(&mut self,
                    _sess: &Session,
                    _matches: &getopts::Matches)
                    -> Option<(PpMode, Option<UserIdentifiedItem>)> {
        None
    }

    // Create a CompilController struct for controlling the behaviour of compilation.
    fn build_controller(&mut self, &Session) -> CompileController<'a>;
}

// CompilerCalls instance for a regular rustc build.
#[derive(Copy, Clone)]
pub struct RustcDefaultCalls;

impl<'a> CompilerCalls<'a> for RustcDefaultCalls {
    fn early_callback(&mut self,
                      matches: &getopts::Matches,
                      descriptions: &diagnostics::registry::Registry,
                      color: diagnostic::ColorConfig)
                      -> Compilation {
        match matches.opt_str("explain") {
            Some(ref code) => {
                match descriptions.find_description(&code[..]) {
                    Some(ref description) => {
                        // Slice off the leading newline and print.
                        print!("{}", &description[1..]);
                    }
                    None => {
                        early_error(color, &format!("no extended information for {}", code));
                    }
                }
                return Compilation::Stop;
            },
            None => ()
        }

        return Compilation::Continue;
    }

    fn no_input(&mut self,
                matches: &getopts::Matches,
                sopts: &config::Options,
                odir: &Option<PathBuf>,
                ofile: &Option<PathBuf>,
                descriptions: &diagnostics::registry::Registry)
                -> Option<(Input, Option<PathBuf>)> {
        match matches.free.len() {
            0 => {
                if sopts.describe_lints {
                    let mut ls = lint::LintStore::new();
                    rustc_lint::register_builtins(&mut ls, None);
                    describe_lints(&ls, false);
                    return None;
                }
                let sess = build_session(sopts.clone(), None, descriptions.clone());
                rustc_lint::register_builtins(&mut sess.lint_store.borrow_mut(), Some(&sess));
                let should_stop = RustcDefaultCalls::print_crate_info(&sess, None, odir, ofile);
                if should_stop == Compilation::Stop {
                    return None;
                }
                early_error(sopts.color, "no input filename given");
            }
            1 => panic!("make_input should have provided valid inputs"),
            _ => early_error(sopts.color, "multiple input filenames provided")
        }

        None
    }

    fn parse_pretty(&mut self,
                    sess: &Session,
                    matches: &getopts::Matches)
                    -> Option<(PpMode, Option<UserIdentifiedItem>)> {
        let pretty = if sess.opts.debugging_opts.unstable_options {
            matches.opt_default("pretty", "normal").map(|a| {
                // stable pretty-print variants only
                pretty::parse_pretty(sess, &a, false)
            })
        } else {
            None
        };
        if pretty.is_none() && sess.unstable_options() {
            matches.opt_str("unpretty").map(|a| {
                // extended with unstable pretty-print variants
                pretty::parse_pretty(sess, &a, true)
            })
        } else {
            pretty
        }
    }

    fn late_callback(&mut self,
                     matches: &getopts::Matches,
                     sess: &Session,
                     input: &Input,
                     odir: &Option<PathBuf>,
                     ofile: &Option<PathBuf>)
                     -> Compilation {
        RustcDefaultCalls::print_crate_info(sess, Some(input), odir, ofile).and_then(
            || RustcDefaultCalls::list_metadata(sess, matches, input))
    }

    fn build_controller(&mut self, sess: &Session) -> CompileController<'a> {
        let mut control = CompileController::basic();

        if sess.opts.parse_only ||
           sess.opts.show_span.is_some() ||
           sess.opts.debugging_opts.ast_json_noexpand {
            control.after_parse.stop = Compilation::Stop;
        }

        if sess.opts.no_analysis || sess.opts.debugging_opts.ast_json {
            control.after_write_deps.stop = Compilation::Stop;
        }

        if sess.opts.no_trans {
            control.after_analysis.stop = Compilation::Stop;
        }

        if !sess.opts.output_types.iter().any(|&i| i == config::OutputTypeExe) {
            control.after_llvm.stop = Compilation::Stop;
        }

        if sess.opts.debugging_opts.save_analysis {
            control.after_analysis.callback = box |state| {
                time(state.session.time_passes(),
                     "save analysis",
                     || save::process_crate(state.tcx.unwrap(),
                                            state.krate.unwrap(),
                                            state.analysis.unwrap(),
                                            state.out_dir));
            };
            control.make_glob_map = resolve::MakeGlobMap::Yes;
        }

        control
    }
}

impl RustcDefaultCalls {
    pub fn list_metadata(sess: &Session,
                         matches: &getopts::Matches,
                         input: &Input)
                         -> Compilation {
        let r = matches.opt_strs("Z");
        if r.contains(&("ls".to_string())) {
            match input {
                &Input::File(ref ifile) => {
                    let path = &(*ifile);
                    let mut v = Vec::new();
                    metadata::loader::list_file_metadata(&sess.target.target,
                                                         path,
                                                         &mut v).unwrap();
                    println!("{}", String::from_utf8(v).unwrap());
                }
                &Input::Str(_) => {
                    early_error(sess.opts.color, "cannot list metadata for stdin");
                }
            }
            return Compilation::Stop;
        }

        return Compilation::Continue;
    }


    fn print_crate_info(sess: &Session,
                        input: Option<&Input>,
                        odir: &Option<PathBuf>,
                        ofile: &Option<PathBuf>)
                        -> Compilation {
        if sess.opts.prints.is_empty() {
            return Compilation::Continue;
        }

        let attrs = input.map(|input| parse_crate_attrs(sess, input));
        for req in &sess.opts.prints {
            match *req {
                PrintRequest::Sysroot => println!("{}", sess.sysroot().display()),
                PrintRequest::FileNames |
                PrintRequest::CrateName => {
                    let input = match input {
                        Some(input) => input,
                        None => early_error(sess.opts.color, "no input file provided"),
                    };
                    let attrs = attrs.as_ref().unwrap();
                    let t_outputs = driver::build_output_filenames(input,
                                                                   odir,
                                                                   ofile,
                                                                   attrs,
                                                                   sess);
                    let id = link::find_crate_name(Some(sess),
                                                   attrs,
                                                   input);
                    if *req == PrintRequest::CrateName {
                        println!("{}", id);
                        continue
                    }
                    let crate_types = driver::collect_crate_types(sess, attrs);
                    let metadata = driver::collect_crate_metadata(sess, attrs);
                    *sess.crate_metadata.borrow_mut() = metadata;
                    for &style in &crate_types {
                        let fname = link::filename_for_input(sess, style, &id,
                                                             &t_outputs);
                        println!("{}", fname.file_name().unwrap()
                                            .to_string_lossy());
                    }
                }
            }
        }
        return Compilation::Stop;
    }
}

/// Returns a version string such as "0.12.0-dev".
pub fn release_str() -> Option<&'static str> {
    option_env!("CFG_RELEASE")
}

/// Returns the full SHA1 hash of HEAD of the Git repo from which rustc was built.
pub fn commit_hash_str() -> Option<&'static str> {
    option_env!("CFG_VER_HASH")
}

/// Returns the "commit date" of HEAD of the Git repo from which rustc was built as a static string.
pub fn commit_date_str() -> Option<&'static str> {
    option_env!("CFG_VER_DATE")
}

/// Prints version information
pub fn version(binary: &str, matches: &getopts::Matches) {
    let verbose = matches.opt_present("verbose");

    println!("{} {}", binary, option_env!("CFG_VERSION").unwrap_or("unknown version"));
    if verbose {
        fn unw(x: Option<&str>) -> &str { x.unwrap_or("unknown") }
        println!("binary: {}", binary);
        println!("commit-hash: {}", unw(commit_hash_str()));
        println!("commit-date: {}", unw(commit_date_str()));
        println!("host: {}", config::host_triple());
        println!("release: {}", unw(release_str()));
    }
}

fn usage(verbose: bool, include_unstable_options: bool) {
    let groups = if verbose {
        config::rustc_optgroups()
    } else {
        config::rustc_short_optgroups()
    };
    let groups : Vec<_> = groups.into_iter()
        .filter(|x| include_unstable_options || x.is_stable())
        .map(|x|x.opt_group)
        .collect();
    let message = format!("Usage: rustc [OPTIONS] INPUT");
    let extra_help = if verbose {
        ""
    } else {
        "\n    --help -v           Print the full set of options rustc accepts"
    };
    println!("{}\n\
Additional help:
    -C help             Print codegen options
    -W help             Print 'lint' options and default settings
    -Z help             Print internal options for debugging rustc{}\n",
              getopts::usage(&message, &groups),
              extra_help);
}

fn describe_lints(lint_store: &lint::LintStore, loaded_plugins: bool) {
    println!("
Available lint options:
    -W <foo>           Warn about <foo>
    -A <foo>           Allow <foo>
    -D <foo>           Deny <foo>
    -F <foo>           Forbid <foo> (deny, and deny all overrides)

");

    fn sort_lints(lints: Vec<(&'static Lint, bool)>) -> Vec<&'static Lint> {
        let mut lints: Vec<_> = lints.into_iter().map(|(x, _)| x).collect();
        lints.sort_by(|x: &&Lint, y: &&Lint| {
            match x.default_level.cmp(&y.default_level) {
                // The sort doesn't case-fold but it's doubtful we care.
                Equal => x.name.cmp(y.name),
                r => r,
            }
        });
        lints
    }

    fn sort_lint_groups(lints: Vec<(&'static str, Vec<lint::LintId>, bool)>)
                     -> Vec<(&'static str, Vec<lint::LintId>)> {
        let mut lints: Vec<_> = lints.into_iter().map(|(x, y, _)| (x, y)).collect();
        lints.sort_by(|&(x, _): &(&'static str, Vec<lint::LintId>),
                       &(y, _): &(&'static str, Vec<lint::LintId>)| {
            x.cmp(y)
        });
        lints
    }

    let (plugin, builtin): (Vec<_>, _) = lint_store.get_lints()
        .iter().cloned().partition(|&(_, p)| p);
    let plugin = sort_lints(plugin);
    let builtin = sort_lints(builtin);

    let (plugin_groups, builtin_groups): (Vec<_>, _) = lint_store.get_lint_groups()
        .iter().cloned().partition(|&(_, _, p)| p);
    let plugin_groups = sort_lint_groups(plugin_groups);
    let builtin_groups = sort_lint_groups(builtin_groups);

    let max_name_len = plugin.iter().chain(&builtin)
        .map(|&s| s.name.chars().count())
        .max().unwrap_or(0);
    let padded = |x: &str| {
        let mut s = repeat(" ").take(max_name_len - x.chars().count())
                               .collect::<String>();
        s.push_str(x);
        s
    };

    println!("Lint checks provided by rustc:\n");
    println!("    {}  {:7.7}  {}", padded("name"), "default", "meaning");
    println!("    {}  {:7.7}  {}", padded("----"), "-------", "-------");

    let print_lints = |lints: Vec<&Lint>| {
        for lint in lints {
            let name = lint.name_lower().replace("_", "-");
            println!("    {}  {:7.7}  {}",
                     padded(&name[..]), lint.default_level.as_str(), lint.desc);
        }
        println!("\n");
    };

    print_lints(builtin);



    let max_name_len = plugin_groups.iter().chain(&builtin_groups)
        .map(|&(s, _)| s.chars().count())
        .max().unwrap_or(0);
    let padded = |x: &str| {
        let mut s = repeat(" ").take(max_name_len - x.chars().count())
                               .collect::<String>();
        s.push_str(x);
        s
    };

    println!("Lint groups provided by rustc:\n");
    println!("    {}  {}", padded("name"), "sub-lints");
    println!("    {}  {}", padded("----"), "---------");

    let print_lint_groups = |lints: Vec<(&'static str, Vec<lint::LintId>)>| {
        for (name, to) in lints {
            let name = name.to_lowercase().replace("_", "-");
            let desc = to.into_iter().map(|x| x.as_str().replace("_", "-"))
                         .collect::<Vec<String>>().join(", ");
            println!("    {}  {}",
                     padded(&name[..]), desc);
        }
        println!("\n");
    };

    print_lint_groups(builtin_groups);

    match (loaded_plugins, plugin.len(), plugin_groups.len()) {
        (false, 0, _) | (false, _, 0) => {
            println!("Compiler plugins can provide additional lints and lint groups. To see a \
                      listing of these, re-run `rustc -W help` with a crate filename.");
        }
        (false, _, _) => panic!("didn't load lint plugins but got them anyway!"),
        (true, 0, 0) => println!("This crate does not load any lint plugins or lint groups."),
        (true, l, g) => {
            if l > 0 {
                println!("Lint checks provided by plugins loaded by this crate:\n");
                print_lints(plugin);
            }
            if g > 0 {
                println!("Lint groups provided by plugins loaded by this crate:\n");
                print_lint_groups(plugin_groups);
            }
        }
    }
}

fn describe_debug_flags() {
    println!("\nAvailable debug options:\n");
    for &(name, _, opt_type_desc, desc) in config::DB_OPTIONS {
        let (width, extra) = match opt_type_desc {
            Some(..) => (21, "=val"),
            None => (25, "")
        };
        println!("    -Z {:>width$}{} -- {}", name.replace("_", "-"),
                 extra, desc, width=width);
    }
}

fn describe_codegen_flags() {
    println!("\nAvailable codegen options:\n");
    for &(name, _, opt_type_desc, desc) in config::CG_OPTIONS {
        let (width, extra) = match opt_type_desc {
            Some(..) => (21, "=val"),
            None => (25, "")
        };
        println!("    -C {:>width$}{} -- {}", name.replace("_", "-"),
                 extra, desc, width=width);
    }
}

/// Process command line options. Emits messages as appropriate. If compilation
/// should continue, returns a getopts::Matches object parsed from args, otherwise
/// returns None.
pub fn handle_options(mut args: Vec<String>) -> Option<getopts::Matches> {
    // Throw away the first argument, the name of the binary
    let _binary = args.remove(0);

    if args.is_empty() {
        // user did not write `-v` nor `-Z unstable-options`, so do not
        // include that extra information.
        usage(false, false);
        return None;
    }

    fn allows_unstable_options(matches: &getopts::Matches) -> bool {
        let r = matches.opt_strs("Z");
        r.iter().any(|x| *x == "unstable-options")
    }

    fn parse_all_options(args: &Vec<String>) -> getopts::Matches {
        let all_groups : Vec<getopts::OptGroup>
            = config::rustc_optgroups().into_iter().map(|x|x.opt_group).collect();
        match getopts::getopts(&args[..], &all_groups) {
            Ok(m) => {
                if !allows_unstable_options(&m) {
                    // If -Z unstable-options was not specified, verify that
                    // no unstable options were present.
                    for opt in config::rustc_optgroups().into_iter().filter(|x| !x.is_stable()) {
                        let opt_name = if !opt.opt_group.long_name.is_empty() {
                            &opt.opt_group.long_name
                        } else {
                            &opt.opt_group.short_name
                        };
                        if m.opt_present(opt_name) {
                            early_error(diagnostic::Auto, &format!("use of unstable option '{}' \
                                                                    requires -Z unstable-options",
                                                                   opt_name));
                        }
                    }
                }
                m
            }
            Err(f) => early_error(diagnostic::Auto, &f.to_string())
        }
    }

    // As a speed optimization, first try to parse the command-line using just
    // the stable options.
    let matches = match getopts::getopts(&args[..], &config::optgroups()) {
        Ok(ref m) if allows_unstable_options(m) => {
            // If -Z unstable-options was specified, redo parsing with the
            // unstable options to ensure that unstable options are defined
            // in the returned getopts::Matches.
            parse_all_options(&args)
        }
        Ok(m) => m,
        Err(_) => {
            // redo option parsing, including unstable options this time,
            // in anticipation that the mishandled option was one of the
            // unstable ones.
            parse_all_options(&args)
        }
    };

    if matches.opt_present("h") || matches.opt_present("help") {
        usage(matches.opt_present("verbose"), allows_unstable_options(&matches));
        return None;
    }

    // Don't handle -W help here, because we might first load plugins.

    let r = matches.opt_strs("Z");
    if r.iter().any(|x| *x == "help") {
        describe_debug_flags();
        return None;
    }

    let cg_flags = matches.opt_strs("C");
    if cg_flags.iter().any(|x| *x == "help") {
        describe_codegen_flags();
        return None;
    }

    if cg_flags.contains(&"passes=list".to_string()) {
        unsafe { ::llvm::LLVMRustPrintPasses(); }
        return None;
    }

    if matches.opt_present("version") {
        version("rustc", &matches);
        return None;
    }

    Some(matches)
}

fn parse_crate_attrs(sess: &Session, input: &Input) ->
                     Vec<ast::Attribute> {
    let result = match *input {
        Input::File(ref ifile) => {
            parse::parse_crate_attrs_from_file(ifile,
                                               Vec::new(),
                                               &sess.parse_sess)
        }
        Input::Str(ref src) => {
            parse::parse_crate_attrs_from_source_str(
                driver::anon_src().to_string(),
                src.to_string(),
                Vec::new(),
                &sess.parse_sess)
        }
    };
    result.into_iter().collect()
}

/// Run a procedure which will detect panics in the compiler and print nicer
/// error messages rather than just failing the test.
///
/// The diagnostic emitter yielded to the procedure should be used for reporting
/// errors of the compiler.
pub fn monitor<F:FnOnce()+Send+'static>(f: F) {
    const STACK_SIZE: usize = 8 * 1024 * 1024; // 8MB

    struct Sink(Arc<Mutex<Vec<u8>>>);
    impl Write for Sink {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            Write::write(&mut *self.0.lock().unwrap(), data)
        }
        fn flush(&mut self) -> io::Result<()> { Ok(()) }
    }

    let data = Arc::new(Mutex::new(Vec::new()));
    let err = Sink(data.clone());

    let mut cfg = thread::Builder::new().name("rustc".to_string());

    // FIXME: Hacks on hacks. If the env is trying to override the stack size
    // then *don't* set it explicitly.
    if env::var_os("RUST_MIN_STACK").is_none() {
        cfg = cfg.stack_size(STACK_SIZE);
    }

    match cfg.spawn(move || { io::set_panic(box err); f() }).unwrap().join() {
        Ok(()) => { /* fallthrough */ }
        Err(value) => {
            // Thread panicked without emitting a fatal diagnostic
            if !value.is::<diagnostic::FatalError>() {
                let mut emitter = diagnostic::EmitterWriter::stderr(diagnostic::Auto, None);

                // a .span_bug or .bug call has already printed what
                // it wants to print.
                if !value.is::<diagnostic::ExplicitBug>() {
                    emitter.emit(
                        None,
                        "unexpected panic",
                        None,
                        diagnostic::Bug);
                }

                let xs = [
                    "the compiler unexpectedly panicked. this is a bug.".to_string(),
                    format!("we would appreciate a bug report: {}",
                            BUG_REPORT_URL),
                ];
                for note in &xs {
                    emitter.emit(None, &note[..], None, diagnostic::Note)
                }
                if let None = env::var_os("RUST_BACKTRACE") {
                    emitter.emit(None, "run with `RUST_BACKTRACE=1` for a backtrace",
                                 None, diagnostic::Note);
                }

                println!("{}", str::from_utf8(&data.lock().unwrap()).unwrap());
            }

            // Panic so the process returns a failure code, but don't pollute the
            // output with some unnecessary panic messages, we've already
            // printed everything that we needed to.
            io::set_panic(box io::sink());
            panic!();
        }
    }
}

pub fn diagnostics_registry() -> diagnostics::registry::Registry {
    use syntax::diagnostics::registry::Registry;

    let mut all_errors = Vec::new();
    all_errors.push_all(&rustc::DIAGNOSTICS);
    all_errors.push_all(&rustc_typeck::DIAGNOSTICS);
    all_errors.push_all(&rustc_borrowck::DIAGNOSTICS);
    all_errors.push_all(&rustc_resolve::DIAGNOSTICS);

    Registry::new(&*all_errors)
}

pub fn main() {
    let result = run(env::args().collect());
    process::exit(result as i32);
}
