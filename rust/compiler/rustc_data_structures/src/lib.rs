//! Various data structures used by the Rust compiler. The intention
//! is that code in here should be not be *specific* to rustc, so that
//! it can be easily unit tested and so forth.
//!
//! # Note
//!
//! This API is completely unstable and subject to change.

#![doc(html_root_url = "https://doc.rust-lang.org/nightly/nightly-rustc/")]
#![feature(array_windows)]
#![feature(associated_type_bounds)]
#![feature(auto_traits)]
#![feature(control_flow_enum)]
#![feature(core_intrinsics)]
#![feature(extend_one)]
#![feature(generator_trait)]
#![feature(generators)]
#![feature(let_else)]
#![feature(hash_raw_entry)]
#![feature(hasher_prefixfree_extras)]
#![feature(maybe_uninit_uninit_array)]
#![feature(min_specialization)]
#![feature(never_type)]
#![feature(type_alias_impl_trait)]
#![feature(new_uninit)]
#![feature(once_cell)]
#![feature(rustc_attrs)]
#![feature(test)]
#![feature(thread_id_value)]
#![feature(vec_into_raw_parts)]
#![allow(rustc::default_hash_types)]
#![allow(rustc::potential_query_instability)]

#[macro_use]
extern crate tracing;
#[macro_use]
extern crate cfg_if;
#[macro_use]
extern crate rustc_macros;

pub use rustc_index::static_assert_size;

#[inline(never)]
#[cold]
pub fn cold_path<F: FnOnce() -> R, R>(f: F) -> R {
    f()
}

#[macro_export]
macro_rules! likely {
    ($e:expr) => {
        match $e {
            #[allow(unused_unsafe)]
            e => unsafe { std::intrinsics::likely(e) },
        }
    };
}

#[macro_export]
macro_rules! unlikely {
    ($e:expr) => {
        match $e {
            #[allow(unused_unsafe)]
            e => unsafe { std::intrinsics::unlikely(e) },
        }
    };
}

pub mod base_n;
pub mod binary_search_util;
pub mod captures;
pub mod flock;
pub mod functor;
pub mod fx;
pub mod graph;
pub mod intern;
pub mod jobserver;
pub mod macros;
pub mod map_in_place;
pub mod obligation_forest;
pub mod owning_ref;
pub mod sip128;
pub mod small_c_str;
pub mod small_str;
pub mod snapshot_map;
pub mod stable_map;
pub mod svh;
pub use ena::snapshot_vec;
pub mod memmap;
pub mod sorted_map;
pub mod stable_set;
#[macro_use]
pub mod stable_hasher;
mod atomic_ref;
pub mod fingerprint;
pub mod profiling;
pub mod sharded;
pub mod stack;
pub mod sync;
pub mod thin_vec;
pub mod tiny_list;
pub mod transitive_relation;
pub mod vec_linked_list;
pub mod vec_map;
pub mod work_queue;
pub use atomic_ref::AtomicRef;
pub mod frozen;
pub mod sso;
pub mod steal;
pub mod tagged_ptr;
pub mod temp_dir;
pub mod unhash;

pub use ena::undo_log;
pub use ena::unify;

use std::ops::{Generator, GeneratorState};
use std::pin::Pin;

pub struct OnDrop<F: Fn()>(pub F);

impl<F: Fn()> OnDrop<F> {
    /// Forgets the function which prevents it from running.
    /// Ensure that the function owns no memory, otherwise it will be leaked.
    #[inline]
    pub fn disable(self) {
        std::mem::forget(self);
    }
}

impl<F: Fn()> Drop for OnDrop<F> {
    #[inline]
    fn drop(&mut self) {
        (self.0)();
    }
}

struct IterFromGenerator<G>(G);

impl<G: Generator<Return = ()> + Unpin> Iterator for IterFromGenerator<G> {
    type Item = G::Yield;

    fn next(&mut self) -> Option<Self::Item> {
        match Pin::new(&mut self.0).resume(()) {
            GeneratorState::Yielded(n) => Some(n),
            GeneratorState::Complete(_) => None,
        }
    }
}

/// An adapter for turning a generator closure into an iterator, similar to `iter::from_fn`.
pub fn iter_from_generator<G: Generator<Return = ()> + Unpin>(
    generator: G,
) -> impl Iterator<Item = G::Yield> {
    IterFromGenerator(generator)
}

// See comments in src/librustc_middle/lib.rs
#[doc(hidden)]
pub fn __noop_fix_for_27438() {}
