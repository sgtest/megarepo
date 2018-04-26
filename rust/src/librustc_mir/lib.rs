// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!

Rust MIR: a lowered representation of Rust. Also: an experiment!

*/

#![feature(slice_patterns)]
#![feature(slice_sort_by_cached_key)]
#![feature(from_ref)]
#![feature(box_patterns)]
#![feature(box_syntax)]
#![feature(catch_expr)]
#![feature(crate_visibility_modifier)]
#![feature(const_fn)]
#![feature(core_intrinsics)]
#![feature(decl_macro)]
#![feature(dyn_trait)]
#![feature(fs_read_write)]
#![feature(macro_vis_matcher)]
#![feature(exhaustive_patterns)]
#![feature(range_contains)]
#![feature(rustc_diagnostic_macros)]
#![feature(nonzero)]
#![feature(inclusive_range_fields)]
#![feature(crate_visibility_modifier)]
#![feature(never_type)]
#![cfg_attr(stage0, feature(try_trait))]

extern crate arena;
#[macro_use]
extern crate bitflags;
#[macro_use] extern crate log;
extern crate graphviz as dot;
#[macro_use]
extern crate rustc;
#[macro_use] extern crate rustc_data_structures;
extern crate serialize as rustc_serialize;
extern crate rustc_errors;
#[macro_use]
extern crate syntax;
extern crate syntax_pos;
extern crate rustc_target;
extern crate rustc_const_math;
extern crate core; // for NonZero
extern crate log_settings;
extern crate rustc_apfloat;
extern crate byteorder;

#[cfg(stage0)]
macro_rules! do_catch {
  ($t:expr) => { (|| ::std::ops::Try::from_ok($t) )() }
}

#[cfg(not(stage0))]
macro_rules! do_catch {
  ($t:expr) => { do catch { $t } }
}

mod diagnostics;

mod borrow_check;
mod build;
mod dataflow;
mod hair;
mod shim;
pub mod transform;
pub mod util;
pub mod interpret;
pub mod monomorphize;

pub use hair::pattern::check_crate as matchck_crate;
use rustc::ty::maps::Providers;

pub fn provide(providers: &mut Providers) {
    borrow_check::provide(providers);
    shim::provide(providers);
    transform::provide(providers);
    providers.const_eval = interpret::const_eval_provider;
    providers.check_match = hair::pattern::check_match;
}

__build_diagnostic_array! { librustc_mir, DIAGNOSTICS }
