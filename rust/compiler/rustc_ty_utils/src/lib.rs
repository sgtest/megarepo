//! Various checks
//!
//! # Note
//!
//! This API is completely unstable and subject to change.

#![doc(html_root_url = "https://doc.rust-lang.org/nightly/nightly-rustc/")]
#![feature(control_flow_enum)]
#![feature(let_else)]
#![feature(never_type)]
#![feature(box_patterns)]
#![recursion_limit = "256"]

#[macro_use]
extern crate rustc_middle;
#[macro_use]
extern crate tracing;

use rustc_middle::ty::query::Providers;

mod assoc;
mod common_traits;
mod consts;
pub mod instance;
mod needs_drop;
pub mod representability;
mod ty;

pub fn provide(providers: &mut Providers) {
    assoc::provide(providers);
    common_traits::provide(providers);
    consts::provide(providers);
    needs_drop::provide(providers);
    ty::provide(providers);
    instance::provide(providers);
}
