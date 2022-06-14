/*!

Rust MIR: a lowered representation of Rust.

*/

#![feature(assert_matches)]
#![feature(box_patterns)]
#![feature(control_flow_enum)]
#![feature(decl_macro)]
#![feature(exact_size_is_empty)]
#![feature(let_chains)]
#![feature(let_else)]
#![feature(map_try_insert)]
#![feature(min_specialization)]
#![feature(slice_ptr_get)]
#![feature(option_get_or_insert_default)]
#![feature(never_type)]
#![feature(trait_alias)]
#![feature(trusted_len)]
#![feature(trusted_step)]
#![feature(try_blocks)]
#![feature(yeet_expr)]
#![recursion_limit = "256"]
#![allow(rustc::potential_query_instability)]

#[macro_use]
extern crate tracing;
#[macro_use]
extern crate rustc_middle;

pub mod const_eval;
pub mod interpret;
pub mod transform;
pub mod util;

use rustc_middle::ty;
use rustc_middle::ty::query::Providers;

pub fn provide(providers: &mut Providers) {
    const_eval::provide(providers);
    providers.eval_to_const_value_raw = const_eval::eval_to_const_value_raw_provider;
    providers.eval_to_allocation_raw = const_eval::eval_to_allocation_raw_provider;
    providers.const_caller_location = const_eval::const_caller_location;
    providers.try_destructure_const = |tcx, val| const_eval::try_destructure_const(tcx, val);
    providers.eval_to_valtree = |tcx, param_env_and_value| {
        let (param_env, raw) = param_env_and_value.into_parts();
        const_eval::eval_to_valtree(tcx, param_env, raw)
    };
    providers.try_destructure_mir_constant = |tcx, param_env_and_value| {
        let (param_env, value) = param_env_and_value.into_parts();
        const_eval::try_destructure_mir_constant(tcx, param_env, value).ok()
    };
    providers.valtree_to_const_val = |tcx, (ty, valtree)| {
        const_eval::valtree_to_const_value(tcx, ty::ParamEnv::empty().and(ty), valtree)
    };
    providers.deref_mir_constant = |tcx, param_env_and_value| {
        let (param_env, value) = param_env_and_value.into_parts();
        const_eval::deref_mir_constant(tcx, param_env, value)
    };
}
