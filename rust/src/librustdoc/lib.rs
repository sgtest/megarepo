// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![doc(html_logo_url = "https://www.rust-lang.org/logos/rust-logo-128x128-blk-v2.png",
       html_favicon_url = "https://doc.rust-lang.org/favicon.ico",
       html_root_url = "https://doc.rust-lang.org/nightly/",
       html_playground_url = "https://play.rust-lang.org/")]
#![deny(warnings)]

#![feature(ascii_ctype)]
#![feature(rustc_private)]
#![feature(box_patterns)]
#![feature(box_syntax)]
#![feature(fs_read_write)]
#![feature(libc)]
#![feature(set_stdio)]
#![feature(slice_patterns)]
#![feature(test)]
#![feature(unicode)]
#![feature(vec_remove_item)]

extern crate arena;
extern crate getopts;
extern crate env_logger;
extern crate html_diff;
extern crate libc;
extern crate rustc;
extern crate rustc_data_structures;
extern crate rustc_const_math;
extern crate rustc_trans_utils;
extern crate rustc_driver;
extern crate rustc_resolve;
extern crate rustc_lint;
extern crate rustc_back;
extern crate rustc_metadata;
extern crate rustc_typeck;
extern crate serialize;
#[macro_use] extern crate syntax;
extern crate syntax_pos;
extern crate test as testing;
extern crate std_unicode;
#[macro_use] extern crate log;
extern crate rustc_errors as errors;
extern crate pulldown_cmark;
extern crate tempdir;

extern crate serialize as rustc_serialize; // used by deriving

use std::collections::{BTreeMap, BTreeSet};
use std::default::Default;
use std::env;
use std::fmt::Display;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::mpsc::channel;

use rustc_driver::rustc_trans;

use externalfiles::ExternalHtml;
use rustc::session::search_paths::SearchPaths;
use rustc::session::config::{ErrorOutputType, RustcOptGroup, nightly_options,
                             Externs};

#[macro_use]
pub mod externalfiles;

pub mod clean;
pub mod core;
pub mod doctree;
pub mod fold;
pub mod html {
    pub mod highlight;
    pub mod escape;
    pub mod item_type;
    pub mod format;
    pub mod layout;
    pub mod markdown;
    pub mod render;
    pub mod toc;
}
pub mod markdown;
pub mod passes;
pub mod plugins;
pub mod visit_ast;
pub mod visit_lib;
pub mod test;

use clean::AttributesExt;

use html::markdown::RenderType;

struct Output {
    krate: clean::Crate,
    renderinfo: html::render::RenderInfo,
    passes: Vec<String>,
}

pub fn main() {
    const STACK_SIZE: usize = 32_000_000; // 32MB
    env_logger::init().unwrap();
    let res = std::thread::Builder::new().stack_size(STACK_SIZE).spawn(move || {
        get_args().map(|args| main_args(&args)).unwrap_or(1)
    }).unwrap().join().unwrap_or(101);
    process::exit(res as i32);
}

fn get_args() -> Option<Vec<String>> {
    env::args_os().enumerate()
        .map(|(i, arg)| arg.into_string().map_err(|arg| {
             print_error(format!("Argument {} is not valid Unicode: {:?}", i, arg));
        }).ok())
        .collect()
}

fn stable<F>(name: &'static str, f: F) -> RustcOptGroup
    where F: Fn(&mut getopts::Options) -> &mut getopts::Options + 'static
{
    RustcOptGroup::stable(name, f)
}

fn unstable<F>(name: &'static str, f: F) -> RustcOptGroup
    where F: Fn(&mut getopts::Options) -> &mut getopts::Options + 'static
{
    RustcOptGroup::unstable(name, f)
}

pub fn opts() -> Vec<RustcOptGroup> {
    vec![
        stable("h", |o| o.optflag("h", "help", "show this help message")),
        stable("V", |o| o.optflag("V", "version", "print rustdoc's version")),
        stable("v", |o| o.optflag("v", "verbose", "use verbose output")),
        stable("r", |o| {
            o.optopt("r", "input-format", "the input type of the specified file",
                     "[rust]")
        }),
        stable("w", |o| {
            o.optopt("w", "output-format", "the output type to write", "[html]")
        }),
        stable("o", |o| o.optopt("o", "output", "where to place the output", "PATH")),
        stable("crate-name", |o| {
            o.optopt("", "crate-name", "specify the name of this crate", "NAME")
        }),
        stable("L", |o| {
            o.optmulti("L", "library-path", "directory to add to crate search path",
                       "DIR")
        }),
        stable("cfg", |o| o.optmulti("", "cfg", "pass a --cfg to rustc", "")),
        stable("extern", |o| {
            o.optmulti("", "extern", "pass an --extern to rustc", "NAME=PATH")
        }),
        stable("plugin-path", |o| {
            o.optmulti("", "plugin-path", "directory to load plugins from", "DIR")
        }),
        stable("passes", |o| {
            o.optmulti("", "passes",
                       "list of passes to also run, you might want \
                        to pass it multiple times; a value of `list` \
                        will print available passes",
                       "PASSES")
        }),
        stable("plugins", |o| {
            o.optmulti("", "plugins", "space separated list of plugins to also load",
                       "PLUGINS")
        }),
        stable("no-default", |o| {
            o.optflag("", "no-defaults", "don't run the default passes")
        }),
        stable("document-private-items", |o| {
            o.optflag("", "document-private-items", "document private items")
        }),
        stable("test", |o| o.optflag("", "test", "run code examples as tests")),
        stable("test-args", |o| {
            o.optmulti("", "test-args", "arguments to pass to the test runner",
                       "ARGS")
        }),
        stable("target", |o| o.optopt("", "target", "target triple to document", "TRIPLE")),
        stable("markdown-css", |o| {
            o.optmulti("", "markdown-css",
                       "CSS files to include via <link> in a rendered Markdown file",
                       "FILES")
        }),
        stable("html-in-header", |o|  {
            o.optmulti("", "html-in-header",
                       "files to include inline in the <head> section of a rendered Markdown file \
                        or generated documentation",
                       "FILES")
        }),
        stable("html-before-content", |o| {
            o.optmulti("", "html-before-content",
                       "files to include inline between <body> and the content of a rendered \
                        Markdown file or generated documentation",
                       "FILES")
        }),
        stable("html-after-content", |o| {
            o.optmulti("", "html-after-content",
                       "files to include inline between the content and </body> of a rendered \
                        Markdown file or generated documentation",
                       "FILES")
        }),
        unstable("markdown-before-content", |o| {
            o.optmulti("", "markdown-before-content",
                       "files to include inline between <body> and the content of a rendered \
                        Markdown file or generated documentation",
                       "FILES")
        }),
        unstable("markdown-after-content", |o| {
            o.optmulti("", "markdown-after-content",
                       "files to include inline between the content and </body> of a rendered \
                        Markdown file or generated documentation",
                       "FILES")
        }),
        stable("markdown-playground-url", |o| {
            o.optopt("", "markdown-playground-url",
                     "URL to send code snippets to", "URL")
        }),
        stable("markdown-no-toc", |o| {
            o.optflag("", "markdown-no-toc", "don't include table of contents")
        }),
        stable("e", |o| {
            o.optopt("e", "extend-css",
                     "To add some CSS rules with a given file to generate doc with your \
                      own theme. However, your theme might break if the rustdoc's generated HTML \
                      changes, so be careful!", "PATH")
        }),
        unstable("Z", |o| {
            o.optmulti("Z", "",
                       "internal and debugging options (only on nightly build)", "FLAG")
        }),
        stable("sysroot", |o| {
            o.optopt("", "sysroot", "Override the system root", "PATH")
        }),
        unstable("playground-url", |o| {
            o.optopt("", "playground-url",
                     "URL to send code snippets to, may be reset by --markdown-playground-url \
                      or `#![doc(html_playground_url=...)]`",
                     "URL")
        }),
        unstable("disable-commonmark", |o| {
            o.optflag("", "disable-commonmark", "to disable commonmark doc rendering/testing")
        }),
        unstable("display-warnings", |o| {
            o.optflag("", "display-warnings", "to print code warnings when testing doc")
        }),
        unstable("crate-version", |o| {
            o.optopt("", "crate-version", "crate version to print into documentation", "VERSION")
        }),
        unstable("linker", |o| {
            o.optopt("", "linker", "linker used for building executable test code", "PATH")
        }),
        unstable("sort-modules-by-appearance", |o| {
            o.optflag("", "sort-modules-by-appearance", "sort modules by where they appear in the \
                                                         program, rather than alphabetically")
        }),
        unstable("deny-render-differences", |o| {
            o.optflag("", "deny-render-differences", "abort doc runs when markdown rendering \
                                                      differences are found")
        }),
    ]
}

pub fn usage(argv0: &str) {
    let mut options = getopts::Options::new();
    for option in opts() {
        (option.apply)(&mut options);
    }
    println!("{}", options.usage(&format!("{} [options] <input>", argv0)));
}

pub fn main_args(args: &[String]) -> isize {
    let mut options = getopts::Options::new();
    for option in opts() {
        (option.apply)(&mut options);
    }
    let matches = match options.parse(&args[1..]) {
        Ok(m) => m,
        Err(err) => {
            print_error(err);
            return 1;
        }
    };
    // Check for unstable options.
    nightly_options::check_nightly_options(&matches, &opts());

    // check for deprecated options
    check_deprecated_options(&matches);

    if matches.opt_present("h") || matches.opt_present("help") {
        usage("rustdoc");
        return 0;
    } else if matches.opt_present("version") {
        rustc_driver::version("rustdoc", &matches);
        return 0;
    }

    if matches.opt_strs("passes") == ["list"] {
        println!("Available passes for running rustdoc:");
        for &(name, _, description) in passes::PASSES {
            println!("{:>20} - {}", name, description);
        }
        println!("\nDefault passes for rustdoc:");
        for &name in passes::DEFAULT_PASSES {
            println!("{:>20}", name);
        }
        return 0;
    }

    if matches.free.is_empty() {
        print_error("missing file operand");
        return 1;
    }
    if matches.free.len() > 1 {
        print_error("too many file operands");
        return 1;
    }
    let input = &matches.free[0];

    let mut libs = SearchPaths::new();
    for s in &matches.opt_strs("L") {
        libs.add_path(s, ErrorOutputType::default());
    }
    let externs = match parse_externs(&matches) {
        Ok(ex) => ex,
        Err(err) => {
            print_error(err);
            return 1;
        }
    };

    let test_args = matches.opt_strs("test-args");
    let test_args: Vec<String> = test_args.iter()
                                          .flat_map(|s| s.split_whitespace())
                                          .map(|s| s.to_string())
                                          .collect();

    let should_test = matches.opt_present("test");
    let markdown_input = Path::new(input).extension()
        .map_or(false, |e| e == "md" || e == "markdown");

    let output = matches.opt_str("o").map(|s| PathBuf::from(&s));
    let css_file_extension = matches.opt_str("e").map(|s| PathBuf::from(&s));
    let cfgs = matches.opt_strs("cfg");

    let render_type = if matches.opt_present("disable-commonmark") {
        RenderType::Hoedown
    } else {
        RenderType::Pulldown
    };

    if let Some(ref p) = css_file_extension {
        if !p.is_file() {
            writeln!(
                &mut io::stderr(),
                "rustdoc: option --extend-css argument must be a file."
            ).unwrap();
            return 1;
        }
    }

    let external_html = match ExternalHtml::load(
            &matches.opt_strs("html-in-header"),
            &matches.opt_strs("html-before-content"),
            &matches.opt_strs("html-after-content"),
            &matches.opt_strs("markdown-before-content"),
            &matches.opt_strs("markdown-after-content"),
            render_type) {
        Some(eh) => eh,
        None => return 3,
    };
    let crate_name = matches.opt_str("crate-name");
    let playground_url = matches.opt_str("playground-url");
    let maybe_sysroot = matches.opt_str("sysroot").map(PathBuf::from);
    let display_warnings = matches.opt_present("display-warnings");
    let linker = matches.opt_str("linker").map(PathBuf::from);
    let sort_modules_alphabetically = !matches.opt_present("sort-modules-by-appearance");

    match (should_test, markdown_input) {
        (true, true) => {
            return markdown::test(input, cfgs, libs, externs, test_args, maybe_sysroot,
                                  render_type, display_warnings, linker)
        }
        (true, false) => {
            return test::run(Path::new(input), cfgs, libs, externs, test_args, crate_name,
                             maybe_sysroot, render_type, display_warnings, linker)
        }
        (false, true) => return markdown::render(Path::new(input),
                                                 output.unwrap_or(PathBuf::from("doc")),
                                                 &matches, &external_html,
                                                 !matches.opt_present("markdown-no-toc"),
                                                 render_type),
        (false, false) => {}
    }

    let output_format = matches.opt_str("w");
    let deny_render_differences = matches.opt_present("deny-render-differences");
    let res = acquire_input(PathBuf::from(input), externs, &matches, move |out| {
        let Output { krate, passes, renderinfo } = out;
        info!("going to format");
        match output_format.as_ref().map(|s| &**s) {
            Some("html") | None => {
                html::render::run(krate, &external_html, playground_url,
                                  output.unwrap_or(PathBuf::from("doc")),
                                  passes.into_iter().collect(),
                                  css_file_extension,
                                  renderinfo,
                                  render_type,
                                  sort_modules_alphabetically,
                                  deny_render_differences)
                    .expect("failed to generate documentation");
                0
            }
            Some(s) => {
                print_error(format!("unknown output format: {}", s));
                1
            }
        }
    });
    res.unwrap_or_else(|s| {
        print_error(format!("input error: {}", s));
        1
    })
}

/// Prints an uniformized error message on the standard error output
fn print_error<T>(error_message: T) where T: Display {
    writeln!(
        &mut io::stderr(),
        "rustdoc: {}\nTry 'rustdoc --help' for more information.",
        error_message
    ).unwrap();
}

/// Looks inside the command line arguments to extract the relevant input format
/// and files and then generates the necessary rustdoc output for formatting.
fn acquire_input<R, F>(input: PathBuf,
                       externs: Externs,
                       matches: &getopts::Matches,
                       f: F)
                       -> Result<R, String>
where R: 'static + Send, F: 'static + Send + FnOnce(Output) -> R {
    match matches.opt_str("r").as_ref().map(|s| &**s) {
        Some("rust") => Ok(rust_input(input, externs, matches, f)),
        Some(s) => Err(format!("unknown input format: {}", s)),
        None => Ok(rust_input(input, externs, matches, f))
    }
}

/// Extracts `--extern CRATE=PATH` arguments from `matches` and
/// returns a map mapping crate names to their paths or else an
/// error message.
fn parse_externs(matches: &getopts::Matches) -> Result<Externs, String> {
    let mut externs = BTreeMap::new();
    for arg in &matches.opt_strs("extern") {
        let mut parts = arg.splitn(2, '=');
        let name = parts.next().ok_or("--extern value must not be empty".to_string())?;
        let location = parts.next()
                                 .ok_or("--extern value must be of the format `foo=bar`"
                                    .to_string())?;
        let name = name.to_string();
        externs.entry(name).or_insert_with(BTreeSet::new).insert(location.to_string());
    }
    Ok(Externs::new(externs))
}

/// Interprets the input file as a rust source file, passing it through the
/// compiler all the way through the analysis passes. The rustdoc output is then
/// generated from the cleaned AST of the crate.
///
/// This form of input will run all of the plug/cleaning passes
fn rust_input<R, F>(cratefile: PathBuf, externs: Externs, matches: &getopts::Matches, f: F) -> R
where R: 'static + Send, F: 'static + Send + FnOnce(Output) -> R {
    let mut default_passes = !matches.opt_present("no-defaults");
    let mut passes = matches.opt_strs("passes");
    let mut plugins = matches.opt_strs("plugins");

    // We hardcode in the passes here, as this is a new flag and we
    // are generally deprecating passes.
    if matches.opt_present("document-private-items") {
        default_passes = false;

        passes = vec![
            String::from("collapse-docs"),
            String::from("unindent-comments"),
        ];
    }

    // First, parse the crate and extract all relevant information.
    let mut paths = SearchPaths::new();
    for s in &matches.opt_strs("L") {
        paths.add_path(s, ErrorOutputType::default());
    }
    let cfgs = matches.opt_strs("cfg");
    let triple = matches.opt_str("target");
    let maybe_sysroot = matches.opt_str("sysroot").map(PathBuf::from);
    let crate_name = matches.opt_str("crate-name");
    let crate_version = matches.opt_str("crate-version");
    let plugin_path = matches.opt_str("plugin-path");

    info!("starting to run rustc");
    let display_warnings = matches.opt_present("display-warnings");

    let force_unstable_if_unmarked = matches.opt_strs("Z").iter().any(|x| {
        *x == "force-unstable-if-unmarked"
    });

    let (tx, rx) = channel();
    rustc_driver::monitor(move || {
        use rustc::session::config::Input;

        let (mut krate, renderinfo) =
            core::run_core(paths, cfgs, externs, Input::File(cratefile), triple, maybe_sysroot,
                           display_warnings, force_unstable_if_unmarked);

        info!("finished with rustc");

        if let Some(name) = crate_name {
            krate.name = name
        }

        krate.version = crate_version;

        // Process all of the crate attributes, extracting plugin metadata along
        // with the passes which we are supposed to run.
        for attr in krate.module.as_ref().unwrap().attrs.lists("doc") {
            let name = attr.name().map(|s| s.as_str());
            let name = name.as_ref().map(|s| &s[..]);
            if attr.is_word() {
                if name == Some("no_default_passes") {
                    default_passes = false;
                }
            } else if let Some(value) = attr.value_str() {
                let sink = match name {
                    Some("passes") => &mut passes,
                    Some("plugins") => &mut plugins,
                    _ => continue,
                };
                for p in value.as_str().split_whitespace() {
                    sink.push(p.to_string());
                }
            }
        }

        if default_passes {
            for name in passes::DEFAULT_PASSES.iter().rev() {
                passes.insert(0, name.to_string());
            }
        }

        // Load all plugins/passes into a PluginManager
        let path = plugin_path.unwrap_or("/tmp/rustdoc/plugins".to_string());
        let mut pm = plugins::PluginManager::new(PathBuf::from(path));
        for pass in &passes {
            let plugin = match passes::PASSES.iter()
                                             .position(|&(p, ..)| {
                                                 p == *pass
                                             }) {
                Some(i) => passes::PASSES[i].1,
                None => {
                    error!("unknown pass {}, skipping", *pass);
                    continue
                },
            };
            pm.add_plugin(plugin);
        }
        info!("loading plugins...");
        for pname in plugins {
            pm.load_plugin(pname);
        }

        // Run everything!
        info!("Executing passes/plugins");
        let krate = pm.run_plugins(krate);

        tx.send(f(Output { krate: krate, renderinfo: renderinfo, passes: passes })).unwrap();
    });
    rx.recv().unwrap()
}

/// Prints deprecation warnings for deprecated options
fn check_deprecated_options(matches: &getopts::Matches) {
    let deprecated_flags = [
       "input-format",
       "output-format",
       "plugin-path",
       "plugins",
       "no-defaults",
       "passes",
    ];

    for flag in deprecated_flags.into_iter() {
        if matches.opt_present(flag) {
            eprintln!("WARNING: the '{}' flag is considered deprecated", flag);
            eprintln!("WARNING: please see https://github.com/rust-lang/rust/issues/44136");
        }
    }

    if matches.opt_present("no-defaults") {
        eprintln!("WARNING: (you may want to use --document-private-items)");
    }
}
