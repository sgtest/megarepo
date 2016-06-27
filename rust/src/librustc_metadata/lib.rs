// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![crate_name = "rustc_metadata"]
#![unstable(feature = "rustc_private", issue = "27812")]
#![crate_type = "dylib"]
#![crate_type = "rlib"]
#![doc(html_logo_url = "https://www.rust-lang.org/logos/rust-logo-128x128-blk-v2.png",
       html_favicon_url = "https://doc.rust-lang.org/favicon.ico",
       html_root_url = "https://doc.rust-lang.org/nightly/")]
#![cfg_attr(not(stage0), deny(warnings))]

#![feature(box_patterns)]
#![feature(enumset)]
#![feature(quote)]
#![feature(rustc_diagnostic_macros)]
#![feature(rustc_private)]
#![feature(staged_api)]
#![feature(question_mark)]

#[macro_use] extern crate log;
#[macro_use] extern crate syntax;
#[macro_use] #[no_link] extern crate rustc_bitflags;
extern crate syntax_pos;
extern crate flate;
extern crate rbml;
extern crate serialize as rustc_serialize; // used by deriving
extern crate rustc_errors as errors;

#[macro_use]
extern crate rustc;
extern crate rustc_back;
extern crate rustc_llvm;
extern crate rustc_const_math;

pub use rustc::middle;

#[macro_use]
mod macros;

pub mod diagnostics;

pub mod astencode;
pub mod common;
pub mod def_key;
pub mod tyencode;
pub mod tydecode;
pub mod encoder;
pub mod decoder;
pub mod creader;
pub mod csearch;
pub mod cstore;
pub mod index;
pub mod loader;
pub mod macro_import;
pub mod tls_context;
