#![feature(assert_matches)]
#![feature(core_intrinsics)]
#![feature(hash_raw_entry)]
#![feature(let_else)]
#![feature(min_specialization)]
#![feature(extern_types)]
#![allow(rustc::potential_query_instability)]

#[macro_use]
extern crate tracing;
#[macro_use]
extern crate rustc_data_structures;
#[macro_use]
extern crate rustc_macros;

pub mod cache;
pub mod dep_graph;
pub mod ich;
pub mod query;
