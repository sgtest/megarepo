// This crate is intentionally empty and a re-export of `rustc_driver_impl` to allow the code in
// `rustc_driver_impl` to be compiled in parallel with other crates.

#![allow(internal_features)]
#![feature(rustdoc_internals)]
#![doc(rust_logo)]

pub use rustc_driver_impl::*;
