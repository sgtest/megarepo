#![feature(box_patterns)]
#![feature(decl_macro)]
#![feature(error_iter)]
#![feature(internal_output_capture)]
#![feature(lazy_cell)]
#![feature(let_chains)]
#![feature(thread_spawn_unchecked)]
#![feature(try_blocks)]
#![recursion_limit = "256"]
#![deny(rustc::untranslatable_diagnostic)]
#![deny(rustc::diagnostic_outside_of_impl)]

#[macro_use]
extern crate tracing;

mod callbacks;
mod errors;
pub mod interface;
mod passes;
mod proc_macro_decls;
mod queries;
pub mod util;

pub use callbacks::setup_callbacks;
pub use interface::{run_compiler, Config};
pub use passes::DEFAULT_QUERY_PROVIDERS;
pub use queries::Queries;

#[cfg(test)]
mod tests;

rustc_fluent_macro::fluent_messages! { "../messages.ftl" }
