// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Infrastructure for compiler plugins.
//!
//! Plugins are Rust libraries which extend the behavior of `rustc`
//! in various ways.
//!
//! Plugin authors will use the `Registry` type re-exported by
//! this module, along with its methods.  The rest of the module
//! is for use by `rustc` itself.
//!
//! To define a plugin, build a dylib crate with a
//! `#[plugin_registrar]` function:
//!
//! ```rust,ignore
//! #![crate_name = "myplugin"]
//! #![crate_type = "dylib"]
//! #![feature(plugin_registrar)]
//!
//! extern crate rustc;
//!
//! use rustc_plugin::Registry;
//!
//! #[plugin_registrar]
//! pub fn plugin_registrar(reg: &mut Registry) {
//!     reg.register_macro("mymacro", expand_mymacro);
//! }
//!
//! fn expand_mymacro(...) {  // details elided
//! ```
//!
//! WARNING: We currently don't check that the registrar function
//! has the appropriate type!
//!
//! To use a plugin while compiling another crate:
//!
//! ```rust
//! #![feature(plugin)]
//! #![plugin(myplugin)]
//! ```
//!
//! See the [`plugin` feature](../../unstable-book/language-features/plugin.html) of
//! the Unstable Book for more examples.

#![crate_name = "rustc_plugin"]
#![crate_type = "dylib"]
#![crate_type = "rlib"]
#![doc(html_logo_url = "https://www.rust-lang.org/logos/rust-logo-128x128-blk-v2.png",
       html_favicon_url = "https://doc.rust-lang.org/favicon.ico",
       html_root_url = "https://doc.rust-lang.org/nightly/")]
#![deny(warnings)]

#![feature(rustc_diagnostic_macros)]

#![cfg_attr(stage0, unstable(feature = "rustc_private", issue = "27812"))]
#![cfg_attr(stage0, feature(rustc_private))]
#![cfg_attr(stage0, feature(staged_api))]

#[macro_use] extern crate syntax;

extern crate rustc;
extern crate rustc_back;
extern crate rustc_metadata;
extern crate syntax_pos;
extern crate rustc_errors as errors;

pub use self::registry::Registry;

pub mod diagnostics;
pub mod registry;
pub mod load;
pub mod build;
