// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
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

#![crate_name = "rustc"]
#![unstable(feature = "rustc_private", issue = "27812")]
#![crate_type = "dylib"]
#![crate_type = "rlib"]
#![doc(html_logo_url = "https://www.rust-lang.org/logos/rust-logo-128x128-blk-v2.png",
       html_favicon_url = "https://doc.rust-lang.org/favicon.ico",
       html_root_url = "https://doc.rust-lang.org/nightly/")]
#![cfg_attr(not(stage0), deny(warnings))]

#![feature(associated_consts)]
#![feature(box_patterns)]
#![feature(box_syntax)]
#![feature(collections)]
#![feature(const_fn)]
#![feature(core_intrinsics)]
#![feature(enumset)]
#![feature(iter_arith)]
#![feature(libc)]
#![feature(nonzero)]
#![feature(quote)]
#![feature(rustc_diagnostic_macros)]
#![feature(rustc_private)]
#![feature(slice_patterns)]
#![feature(staged_api)]
#![feature(question_mark)]
#![cfg_attr(test, feature(test))]

extern crate arena;
extern crate core;
extern crate flate;
extern crate fmt_macros;
extern crate getopts;
extern crate graphviz;
extern crate libc;
extern crate rbml;
extern crate rustc_llvm as llvm;
extern crate rustc_back;
extern crate rustc_data_structures;
extern crate serialize;
extern crate collections;
extern crate rustc_const_math;
extern crate rustc_errors as errors;
#[macro_use] extern crate log;
#[macro_use] extern crate syntax;
#[macro_use] extern crate syntax_pos;
#[macro_use] #[no_link] extern crate rustc_bitflags;

extern crate serialize as rustc_serialize; // used by deriving

#[cfg(test)]
extern crate test;

#[macro_use]
mod macros;

// NB: This module needs to be declared first so diagnostics are
// registered before they are used.
pub mod diagnostics;

pub mod cfg;
pub mod dep_graph;
pub mod hir;
pub mod infer;
pub mod lint;

pub mod middle {
    pub mod astconv_util;
    pub mod expr_use_visitor; // STAGE0: increase glitch immunity
    pub mod const_val;
    pub mod const_qualif;
    pub mod cstore;
    pub mod dataflow;
    pub mod dead;
    pub mod dependency_format;
    pub mod effect;
    pub mod entry;
    pub mod free_region;
    pub mod intrinsicck;
    pub mod lang_items;
    pub mod liveness;
    pub mod mem_categorization;
    pub mod privacy;
    pub mod reachable;
    pub mod region;
    pub mod recursion_limit;
    pub mod resolve_lifetime;
    pub mod stability;
    pub mod weak_lang_items;
}

pub mod mir {
    mod cache;
    pub mod repr;
    pub mod tcx;
    pub mod visit;
    pub mod transform;
    pub mod traversal;
    pub mod mir_map;
}

pub mod session;
pub mod traits;
pub mod ty;

pub mod util {
    pub use rustc_back::sha2;

    pub mod common;
    pub mod ppaux;
    pub mod nodemap;
    pub mod num;
    pub mod fs;
}

// A private module so that macro-expanded idents like
// `::rustc::lint::Lint` will also work in `rustc` itself.
//
// `libstd` uses the same trick.
#[doc(hidden)]
mod rustc {
    pub use lint;
}

// FIXME(#27438): right now the unit tests of librustc don't refer to any actual
//                functions generated in librustc_data_structures (all
//                references are through generic functions), but statics are
//                referenced from time to time. Due to this bug we won't
//                actually correctly link in the statics unless we also
//                reference a function, so be sure to reference a dummy
//                function.
#[test]
fn noop() {
    rustc_data_structures::__noop_fix_for_27438();
}


// Build the diagnostics array at the end so that the metadata includes error use sites.
__build_diagnostic_array! { librustc, DIAGNOSTICS }
