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

#![deny(warnings)]

#![feature(box_patterns)]
#![feature(box_syntax)]
#![feature(i128_type)]
#![feature(rustc_diagnostic_macros)]
#![feature(placement_in_syntax)]
#![feature(collection_placement)]
#![feature(nonzero)]

#[macro_use] extern crate log;
extern crate graphviz as dot;
#[macro_use]
extern crate rustc;
extern crate rustc_data_structures;
extern crate rustc_errors;
#[macro_use]
#[no_link]
extern crate rustc_bitflags;
#[macro_use]
extern crate syntax;
extern crate syntax_pos;
extern crate rustc_const_math;
extern crate rustc_const_eval;
extern crate core; // for NonZero

mod diagnostics;

mod borrow_check;
mod build;
mod dataflow;
mod hair;
mod shim;
pub mod transform;
pub mod util;

use rustc::ty::maps::Providers;

pub fn provide(providers: &mut Providers) {
    borrow_check::provide(providers);
    shim::provide(providers);
    transform::provide(providers);
}

__build_diagnostic_array! { librustc_mir, DIAGNOSTICS }
