// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! # The Rust core allocation library
//!
//! This is the lowest level library through which allocation in Rust can be
//! performed.
//!
//! This library, like libcore, is not intended for general usage, but rather as
//! a building block of other libraries. The types and interfaces in this
//! library are reexported through the [standard library](../std/index.html),
//! and should not be used through this library.
//!
//! Currently, there are four major definitions in this library.
//!
//! ## Boxed values
//!
//! The [`Box`](boxed/index.html) type is the core owned pointer type in Rust.
//! There can only be one owner of a `Box`, and the owner can decide to mutate
//! the contents, which live on the heap.
//!
//! This type can be sent among tasks efficiently as the size of a `Box` value
//! is the same as that of a pointer. Tree-like data structures are often built
//! with boxes because each node often has only one owner, the parent.
//!
//! ## Reference counted pointers
//!
//! The [`Rc`](rc/index.html) type is a non-threadsafe reference-counted pointer
//! type intended for sharing memory within a task. An `Rc` pointer wraps a
//! type, `T`, and only allows access to `&T`, a shared reference.
//!
//! This type is useful when inherited mutability (such as using `Box`) is too
//! constraining for an application, and is often paired with the `Cell` or
//! `RefCell` types in order to allow mutation.
//!
//! ## Atomically reference counted pointers
//!
//! The [`Arc`](arc/index.html) type is the threadsafe equivalent of the `Rc`
//! type. It provides all the same functionality of `Rc`, except it requires
//! that the contained type `T` is shareable. Additionally, `Arc<T>` is itself
//! sendable while `Rc<T>` is not.
//!
//! This types allows for shared access to the contained data, and is often
//! paired with synchronization primitives such as mutexes to allow mutation of
//! shared resources.
//!
//! ## Heap interfaces
//!
//! The [`heap`](heap/index.html) module defines the low-level interface to the
//! default global allocator. It is not compatible with the libc allocator API.

#![crate_name = "alloc"]
#![experimental]
#![license = "MIT/ASL2"]
#![crate_type = "rlib"]
#![doc(html_logo_url = "http://www.rust-lang.org/logos/rust-logo-128x128-blk-v2.png",
       html_favicon_url = "http://www.rust-lang.org/favicon.ico",
       html_root_url = "http://doc.rust-lang.org/nightly/")]

#![no_std]
#![feature(lang_items, phase, unsafe_destructor)]

#[phase(plugin, link)]
extern crate core;
extern crate libc;

// Allow testing this library

#[cfg(test)] #[phase(plugin, link)] extern crate std;
#[cfg(test)] #[phase(plugin, link)] extern crate log;

// The deprecated name of the boxed module

#[deprecated = "use boxed instead"]
#[cfg(not(test))]
pub use boxed as owned;

// Heaps provided for low-level allocation strategies

pub mod heap;

// Primitive types using the heaps above

#[cfg(not(test))]
pub mod boxed;
pub mod arc;
pub mod rc;

/// Common out-of-memory routine
#[cold]
#[inline(never)]
pub fn oom() -> ! {
    // FIXME(#14674): This really needs to do something other than just abort
    //                here, but any printing done must be *guaranteed* to not
    //                allocate.
    unsafe { core::intrinsics::abort() }
}

// FIXME(#14344): When linking liballoc with libstd, this library will be linked
//                as an rlib (it only exists as an rlib). It turns out that an
//                optimized standard library doesn't actually use *any* symbols
//                from this library. Everything is inlined and optimized away.
//                This means that linkers will actually omit the object for this
//                file, even though it may be needed in the future.
//
//                To get around this for now, we define a dummy symbol which
//                will never get inlined so the stdlib can call it. The stdlib's
//                reference to this symbol will cause this library's object file
//                to get linked in to libstd successfully (the linker won't
//                optimize it out).
#[doc(hidden)]
pub fn fixme_14344_be_sure_to_link_to_collections() {}

#[cfg(not(test))]
#[doc(hidden)]
mod std {
    pub use core::fmt;
    pub use core::option;
}
