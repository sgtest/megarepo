/*!

Rust MIR: a lowered representation of Rust.

*/

#![feature(nll)]
#![feature(in_band_lifetimes)]
#![feature(bool_to_option)]
#![feature(box_patterns)]
#![feature(box_syntax)]
#![feature(const_if_match)]
#![feature(const_fn)]
#![feature(const_panic)]
#![feature(crate_visibility_modifier)]
#![feature(drain_filter)]
#![feature(exhaustive_patterns)]
#![feature(iter_order_by)]
#![feature(never_type)]
#![feature(specialization)]
#![feature(trusted_len)]
#![feature(try_blocks)]
#![feature(associated_type_bounds)]
#![feature(range_is_empty)]
#![feature(stmt_expr_attributes)]
#![feature(trait_alias)]
#![feature(option_expect_none)]
#![recursion_limit = "256"]

#[macro_use]
extern crate log;
#[macro_use]
extern crate rustc;

mod borrow_check;
pub mod const_eval;
pub mod dataflow;
pub mod interpret;
pub mod monomorphize;
mod shim;
pub mod transform;
pub mod util;

use rustc::ty::query::Providers;

pub fn provide(providers: &mut Providers<'_>) {
    borrow_check::provide(providers);
    const_eval::provide(providers);
    shim::provide(providers);
    transform::provide(providers);
    monomorphize::partitioning::provide(providers);
    providers.const_eval_validated = const_eval::const_eval_validated_provider;
    providers.const_eval_raw = const_eval::const_eval_raw_provider;
    providers.const_caller_location = const_eval::const_caller_location;
    providers.const_field = |tcx, param_env_and_value| {
        let (param_env, (value, field)) = param_env_and_value.into_parts();
        const_eval::const_field(tcx, param_env, None, field, value)
    };
    providers.destructure_const = |tcx, param_env_and_value| {
        let (param_env, value) = param_env_and_value.into_parts();
        const_eval::destructure_const(tcx, param_env, value)
    };
}
