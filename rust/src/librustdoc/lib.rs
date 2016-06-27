// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![crate_name = "rustdoc"]
#![unstable(feature = "rustdoc", issue = "27812")]
#![crate_type = "dylib"]
#![crate_type = "rlib"]
#![doc(html_logo_url = "https://www.rust-lang.org/logos/rust-logo-128x128-blk-v2.png",
       html_favicon_url = "https://doc.rust-lang.org/favicon.ico",
       html_root_url = "https://doc.rust-lang.org/nightly/",
       html_playground_url = "https://play.rust-lang.org/")]
#![cfg_attr(not(stage0), deny(warnings))]

#![feature(box_patterns)]
#![feature(box_syntax)]
#![feature(libc)]
#![feature(rustc_private)]
#![feature(set_stdio)]
#![feature(slice_patterns)]
#![feature(staged_api)]
#![feature(test)]
#![feature(unicode)]
#![feature(question_mark)]

extern crate arena;
extern crate getopts;
extern crate libc;
extern crate rustc;
extern crate rustc_const_eval;
extern crate rustc_trans;
extern crate rustc_driver;
extern crate rustc_resolve;
extern crate rustc_lint;
extern crate rustc_back;
extern crate rustc_metadata;
extern crate serialize;
#[macro_use] extern crate syntax;
extern crate syntax_pos;
extern crate test as testing;
extern crate rustc_unicode;
#[macro_use] extern crate log;
extern crate rustc_errors as errors;

extern crate serialize as rustc_serialize; // used by deriving

use std::collections::HashMap;
use std::default::Default;
use std::env;
use std::path::PathBuf;
use std::process;
use std::sync::mpsc::channel;

use externalfiles::ExternalHtml;
use rustc::session::search_paths::SearchPaths;
use rustc::session::config::{ErrorOutputType, RustcOptGroup, nightly_options};

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
mod flock;

use clean::Attributes;

type Pass = (&'static str,                                      // name
             fn(clean::Crate) -> plugins::PluginResult,         // fn
             &'static str);                                     // description

const PASSES: &'static [Pass] = &[
    ("strip-hidden", passes::strip_hidden,
     "strips all doc(hidden) items from the output"),
    ("unindent-comments", passes::unindent_comments,
     "removes excess indentation on comments in order for markdown to like it"),
    ("collapse-docs", passes::collapse_docs,
     "concatenates all document attributes into one document attribute"),
    ("strip-private", passes::strip_private,
     "strips all private items from a crate which cannot be seen externally, \
      implies strip-priv-imports"),
    ("strip-priv-imports", passes::strip_priv_imports,
     "strips all private import statements (`use`, `extern crate`) from a crate"),
];

const DEFAULT_PASSES: &'static [&'static str] = &[
    "strip-hidden",
    "strip-private",
    "collapse-docs",
    "unindent-comments",
];

struct Output {
    krate: clean::Crate,
    renderinfo: html::render::RenderInfo,
    passes: Vec<String>,
}

pub fn main() {
    const STACK_SIZE: usize = 32000000; // 32MB
    let res = std::thread::Builder::new().stack_size(STACK_SIZE).spawn(move || {
        let s = env::args().collect::<Vec<_>>();
        main_args(&s)
    }).unwrap().join().unwrap_or(101);
    process::exit(res as i32);
}

fn stable(g: getopts::OptGroup) -> RustcOptGroup { RustcOptGroup::stable(g) }
fn unstable(g: getopts::OptGroup) -> RustcOptGroup { RustcOptGroup::unstable(g) }

pub fn opts() -> Vec<RustcOptGroup> {
    use getopts::*;
    vec!(
        stable(optflag("h", "help", "show this help message")),
        stable(optflag("V", "version", "print rustdoc's version")),
        stable(optflag("v", "verbose", "use verbose output")),
        stable(optopt("r", "input-format", "the input type of the specified file",
                      "[rust]")),
        stable(optopt("w", "output-format", "the output type to write",
                      "[html]")),
        stable(optopt("o", "output", "where to place the output", "PATH")),
        stable(optopt("", "crate-name", "specify the name of this crate", "NAME")),
        stable(optmulti("L", "library-path", "directory to add to crate search path",
                        "DIR")),
        stable(optmulti("", "cfg", "pass a --cfg to rustc", "")),
        stable(optmulti("", "extern", "pass an --extern to rustc", "NAME=PATH")),
        stable(optmulti("", "plugin-path", "directory to load plugins from", "DIR")),
        stable(optmulti("", "passes",
                        "list of passes to also run, you might want \
                         to pass it multiple times; a value of `list` \
                         will print available passes",
                        "PASSES")),
        stable(optmulti("", "plugins", "space separated list of plugins to also load",
                        "PLUGINS")),
        stable(optflag("", "no-defaults", "don't run the default passes")),
        stable(optflag("", "test", "run code examples as tests")),
        stable(optmulti("", "test-args", "arguments to pass to the test runner",
                        "ARGS")),
        stable(optopt("", "target", "target triple to document", "TRIPLE")),
        stable(optmulti("", "markdown-css",
                        "CSS files to include via <link> in a rendered Markdown file",
                        "FILES")),
        stable(optmulti("", "html-in-header",
                        "files to include inline in the <head> section of a rendered Markdown file \
                         or generated documentation",
                        "FILES")),
        stable(optmulti("", "html-before-content",
                        "files to include inline between <body> and the content of a rendered \
                         Markdown file or generated documentation",
                        "FILES")),
        stable(optmulti("", "html-after-content",
                        "files to include inline between the content and </body> of a rendered \
                         Markdown file or generated documentation",
                        "FILES")),
        stable(optopt("", "markdown-playground-url",
                      "URL to send code snippets to", "URL")),
        stable(optflag("", "markdown-no-toc", "don't include table of contents")),
        unstable(optopt("e", "extend-css",
                        "to redefine some css rules with a given file to generate doc with your \
                         own theme", "PATH")),
        unstable(optmulti("Z", "",
                          "internal and debugging options (only on nightly build)", "FLAG")),
    )
}

pub fn usage(argv0: &str) {
    println!("{}",
             getopts::usage(&format!("{} [options] <input>", argv0),
                            &opts().into_iter()
                                   .map(|x| x.opt_group)
                                   .collect::<Vec<getopts::OptGroup>>()));
}

pub fn main_args(args: &[String]) -> isize {
    let all_groups: Vec<getopts::OptGroup> = opts()
                                             .into_iter()
                                             .map(|x| x.opt_group)
                                             .collect();
    let matches = match getopts::getopts(&args[1..], &all_groups) {
        Ok(m) => m,
        Err(err) => {
            println!("{}", err);
            return 1;
        }
    };
    // Check for unstable options.
    nightly_options::check_nightly_options(&matches, &opts());

    if matches.opt_present("h") || matches.opt_present("help") {
        usage(&args[0]);
        return 0;
    } else if matches.opt_present("version") {
        rustc_driver::version("rustdoc", &matches);
        return 0;
    }

    if matches.opt_strs("passes") == ["list"] {
        println!("Available passes for running rustdoc:");
        for &(name, _, description) in PASSES {
            println!("{:>20} - {}", name, description);
        }
        println!("\nDefault passes for rustdoc:");
        for &name in DEFAULT_PASSES {
            println!("{:>20}", name);
        }
        return 0;
    }

    if matches.free.is_empty() {
        println!("expected an input file to act on");
        return 1;
    } if matches.free.len() > 1 {
        println!("only one input file may be specified");
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
            println!("{}", err);
            return 1;
        }
    };

    let test_args = matches.opt_strs("test-args");
    let test_args: Vec<String> = test_args.iter()
                                          .flat_map(|s| s.split_whitespace())
                                          .map(|s| s.to_string())
                                          .collect();

    let should_test = matches.opt_present("test");
    let markdown_input = input.ends_with(".md") || input.ends_with(".markdown");

    let output = matches.opt_str("o").map(|s| PathBuf::from(&s));
    let css_file_extension = matches.opt_str("e").map(|s| PathBuf::from(&s));
    let cfgs = matches.opt_strs("cfg");

    if let Some(ref p) = css_file_extension {
        if !p.is_file() {
            println!("{}", "--extend-css option must take a css file as input");
            return 1;
        }
    }

    let external_html = match ExternalHtml::load(
            &matches.opt_strs("html-in-header"),
            &matches.opt_strs("html-before-content"),
            &matches.opt_strs("html-after-content")) {
        Some(eh) => eh,
        None => return 3
    };
    let crate_name = matches.opt_str("crate-name");

    match (should_test, markdown_input) {
        (true, true) => {
            return markdown::test(input, cfgs, libs, externs, test_args)
        }
        (true, false) => {
            return test::run(input, cfgs, libs, externs, test_args, crate_name)
        }
        (false, true) => return markdown::render(input,
                                                 output.unwrap_or(PathBuf::from("doc")),
                                                 &matches, &external_html,
                                                 !matches.opt_present("markdown-no-toc")),
        (false, false) => {}
    }
    let out = match acquire_input(input, externs, &matches) {
        Ok(out) => out,
        Err(s) => {
            println!("input error: {}", s);
            return 1;
        }
    };
    let Output { krate, passes, renderinfo } = out;
    info!("going to format");
    match matches.opt_str("w").as_ref().map(|s| &**s) {
        Some("html") | None => {
            html::render::run(krate, &external_html,
                              output.unwrap_or(PathBuf::from("doc")),
                              passes.into_iter().collect(),
                              css_file_extension,
                              renderinfo)
                .expect("failed to generate documentation")
        }
        Some(s) => {
            println!("unknown output format: {}", s);
            return 1;
        }
    }

    return 0;
}

/// Looks inside the command line arguments to extract the relevant input format
/// and files and then generates the necessary rustdoc output for formatting.
fn acquire_input(input: &str,
                 externs: core::Externs,
                 matches: &getopts::Matches) -> Result<Output, String> {
    match matches.opt_str("r").as_ref().map(|s| &**s) {
        Some("rust") => Ok(rust_input(input, externs, matches)),
        Some(s) => Err(format!("unknown input format: {}", s)),
        None => {
            Ok(rust_input(input, externs, matches))
        }
    }
}

/// Extracts `--extern CRATE=PATH` arguments from `matches` and
/// returns a `HashMap` mapping crate names to their paths or else an
/// error message.
fn parse_externs(matches: &getopts::Matches) -> Result<core::Externs, String> {
    let mut externs = HashMap::new();
    for arg in &matches.opt_strs("extern") {
        let mut parts = arg.splitn(2, '=');
        let name = parts.next().ok_or("--extern value must not be empty".to_string())?;
        let location = parts.next()
                                 .ok_or("--extern value must be of the format `foo=bar`"
                                    .to_string())?;
        let name = name.to_string();
        externs.entry(name).or_insert(vec![]).push(location.to_string());
    }
    Ok(externs)
}

/// Interprets the input file as a rust source file, passing it through the
/// compiler all the way through the analysis passes. The rustdoc output is then
/// generated from the cleaned AST of the crate.
///
/// This form of input will run all of the plug/cleaning passes
fn rust_input(cratefile: &str, externs: core::Externs, matches: &getopts::Matches) -> Output {
    let mut default_passes = !matches.opt_present("no-defaults");
    let mut passes = matches.opt_strs("passes");
    let mut plugins = matches.opt_strs("plugins");

    // First, parse the crate and extract all relevant information.
    let mut paths = SearchPaths::new();
    for s in &matches.opt_strs("L") {
        paths.add_path(s, ErrorOutputType::default());
    }
    let cfgs = matches.opt_strs("cfg");
    let triple = matches.opt_str("target");

    let cr = PathBuf::from(cratefile);
    info!("starting to run rustc");

    let (tx, rx) = channel();
    rustc_driver::monitor(move || {
        use rustc::session::config::Input;

        tx.send(core::run_core(paths, cfgs, externs, Input::File(cr),
                               triple)).unwrap();
    });
    let (mut krate, renderinfo) = rx.recv().unwrap();
    info!("finished with rustc");

    if let Some(name) = matches.opt_str("crate-name") {
        krate.name = name
    }

    // Process all of the crate attributes, extracting plugin metadata along
    // with the passes which we are supposed to run.
    for attr in krate.module.as_ref().unwrap().attrs.list("doc") {
        match *attr {
            clean::Word(ref w) if "no_default_passes" == *w => {
                default_passes = false;
            },
            clean::NameValue(ref name, ref value) => {
                let sink = match &name[..] {
                    "passes" => &mut passes,
                    "plugins" => &mut plugins,
                    _ => continue,
                };
                for p in value.split_whitespace() {
                    sink.push(p.to_string());
                }
            }
            _ => (),
        }
    }

    if default_passes {
        for name in DEFAULT_PASSES.iter().rev() {
            passes.insert(0, name.to_string());
        }
    }

    // Load all plugins/passes into a PluginManager
    let path = matches.opt_str("plugin-path")
                      .unwrap_or("/tmp/rustdoc/plugins".to_string());
    let mut pm = plugins::PluginManager::new(PathBuf::from(path));
    for pass in &passes {
        let plugin = match PASSES.iter()
                                 .position(|&(p, _, _)| {
                                     p == *pass
                                 }) {
            Some(i) => PASSES[i].1,
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
    Output { krate: krate, renderinfo: renderinfo, passes: passes }
}
