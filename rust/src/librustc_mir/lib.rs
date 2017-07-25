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

#![crate_name = "rustc_mir"]
#![crate_type = "rlib"]
#![crate_type = "dylib"]
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
#[macro_use]
#[no_link]
extern crate rustc_bitflags;
#[macro_use]
extern crate syntax;
extern crate syntax_pos;
extern crate rustc_const_math;
extern crate rustc_const_eval;
extern crate core; // for NonZero

pub mod diagnostics;

mod build;
pub mod dataflow;
mod hair;
mod shim;
pub mod transform;
pub mod util;

use rustc::ty::maps::Providers;

pub fn provide(providers: &mut Providers) {
    shim::provide(providers);
    transform::provide(providers);
}
