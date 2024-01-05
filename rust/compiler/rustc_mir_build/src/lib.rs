//! Construction of MIR from HIR.
//!
//! This crate also contains the match exhaustiveness and usefulness checking.
#![feature(assert_matches)]
#![feature(associated_type_bounds)]
#![feature(box_patterns)]
#![feature(if_let_guard)]
#![feature(let_chains)]
#![feature(min_specialization)]
#![feature(try_blocks)]
#![recursion_limit = "256"]

#[macro_use]
extern crate tracing;
#[macro_use]
extern crate rustc_middle;

mod build;
mod check_unsafety;
mod errors;
pub mod lints;
mod thir;

use rustc_middle::query::Providers;

rustc_fluent_macro::fluent_messages! { "../messages.ftl" }

pub fn provide(providers: &mut Providers) {
    providers.check_match = thir::pattern::check_match;
    providers.lit_to_const = thir::constant::lit_to_const;
    providers.mir_built = build::mir_built;
    providers.closure_saved_names_of_captured_variables =
        build::closure_saved_names_of_captured_variables;
    providers.check_unsafety = check_unsafety::check_unsafety;
    providers.thir_body = thir::cx::thir_body;
    providers.thir_tree = thir::print::thir_tree;
    providers.thir_flat = thir::print::thir_flat;
}
