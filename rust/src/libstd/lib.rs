// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! # The Rust Standard Library
//!
//! The Rust Standard Library is the foundation of portable Rust
//! software, a set of minimal and battle-tested shared abstractions
//! for the [broader Rust ecosystem](https://crates.io). It offers
//! core types, like [`Vec`](vec/index.html)
//! and [`Option`](option/index.html), library-defined [operations on
//! language primitives](#primitives), [standard macros](#macros),
//! [I/O](io/index.html) and [multithreading](thread/index.html), among
//! [many other
//! things](#what-is-in-the-standard-library-documentation?).
//!
//! `std` is available to all Rust crates by default, just as if each
//! one contained an `extern crate std` import at the [crate
//! root][book-crate-root]. Therefore the standard library can be
//! accessed in [`use`][book-use] statements through the path `std`,
//! as in [`use std::env`](env/index.html), or in expressions
//! through the absolute path `::std`, as in
//! [`::std::env::args()`](env/fn.args.html).
//!
//! [book-crate-root]: ../book/crates-and-modules.html#basic-terminology:-crates-and-modules
//! [book-use]: ../book/crates-and-modules.html#importing-modules-with-use
//!
//! # How to read this documentation
//!
//! If you already know the name of what you are looking for the
//! fastest way to find it is to use the <a href="#"
//! onclick="focusSearchBar();">search bar</a> at the top of the page.
//!
//! Otherwise, you may want to jump to one of these useful sections:
//!
//! * [`std::*` modules](#modules)
//! * [Primitive types](#primitives)
//! * [Standard macros](#macros)
//! * [The Rust Prelude](prelude/index.html)
//!
//! If this is your first time, the documentation for the standard
//! library is written to be casually perused. Clicking on interesting
//! things should generally lead you to interesting places. Still,
//! there are important bits you don't want to miss, so read on for a
//! tour of the standard library and its documentation!
//!
//! Once you are familiar with the contents of the standard library
//! you may begin to find the verbosity of the prose distracting. At
//! this stage in your development you may want to press the **[-]**
//! button near the top of the page to collapse it into a more
//! skimmable view.
//!
//! While you are looking at that **[-]** button also notice the
//! **[src]** button. Rust's API documentation comes with the source
//! code and you are encouraged to read it. The standard library
//! source is generally high quality and a peek behind the curtains is
//! often enlightening.
//!
//! # What is in the standard library documentation?
//!
//! First of all, The Rust Standard Library is divided into a number
//! of focused modules, [all listed further down this page](#modules).
//! These modules are the bedrock upon which all of Rust is forged,
//! and they have mighty names like [`std::slice`](slice/index.html)
//! and [`std::cmp`](cmp/index.html). Modules' documentation typically
//! includes an overview of the module along with examples, and are
//! a smart place to start familiarizing yourself with the library.
//!
//! Second, implicit methods on [primitive
//! types](../book/primitive-types.html) are documented here. This can
//! be a source of confusion for two reasons:
//!
//! 1. While primitives are implemented by the compiler, the standard
//!    library implements methods directly on the primitive types (and
//!    it is the only library that does so), which are [documented in
//!    the section on primitives](#primitives).
//! 2. The standard library exports many modules *with the same name
//!    as primitive types*. These define additional items related
//!    to the primitive type, but not the all-important methods.
//!
//! So for example there is a [page for the primitive type
//! `i32`](primitive.i32.html) that lists all the methods that can be
//! called on 32-bit integers (very useful), and there is a [page for
//! the module `std::i32`](i32/index.html) that documents the constant
//! values `MIN` and `MAX` (rarely useful).
//!
//! Note the documentation for the primitives
//! [`str`](primitive.str.html) and [`[T]`](primitive.slice.html)
//! (also called 'slice'). Many method calls on
//! [`String`](string/struct.String.html) and
//! [`Vec`](vec/struct.Vec.html) are actually calls to methods on
//! `str` and `[T]` respectively, via [deref
//! coercions](../book/deref-coercions.html).
//!
//! Third, the standard library defines [The Rust
//! Prelude](prelude/index.html), a small collection of items - mostly
//! traits - that are imported into every module of every crate. The
//! traits in the prelude are pervasive, making the prelude
//! documentation a good entry point to learning about the library.
//!
//! And finally, the standard library exports a number of standard
//! macros, and [lists them on this page](#macros) (technically, not
//! all of the standard macros are defined by the standard library -
//! some are defined by the compiler - but they are documented here
//! the same). Like the prelude, the standard macros are imported by
//! default into all crates.
//!
//! # A Tour of The Rust Standard Library
//!
//! The rest of this crate documentation is dedicated to pointing
//! out notable features of The Rust Standard Library.
//!
//! ## Containers and collections
//!
//! The [`option`](option/index.html) and
//! [`result`](result/index.html) modules define optional and
//! error-handling types, `Option` and `Result`. The
//! [`iter`](iter/index.html) module defines Rust's iterator trait,
//! [`Iterator`](iter/trait.Iterator.html), which works with the `for`
//! loop to access collections.
//!
//! The standard library exposes 3 common ways to deal with contiguous
//! regions of memory:
//!
//! * [`Vec<T>`](vec/index.html) - A heap-allocated *vector* that is
//! resizable at runtime.
//! * [`[T; n]`](primitive.array.html) - An inline *array* with a
//! fixed size at compile time.
//! * [`[T]`](primitive.slice.html) - A dynamically sized *slice* into
//! any other kind of contiguous storage, whether heap-allocated or
//! not.
//!
//! Slices can only be handled through some kind of *pointer*, and as
//! such come in many flavours such as:
//!
//! * `&[T]` - *shared slice*
//! * `&mut [T]` - *mutable slice*
//! * [`Box<[T]>`](boxed/index.html) - *owned slice*
//!
//! `str`, a UTF-8 string slice, is a primitive type, and the standard
//! library defines [many methods for it](primitive.str.html). Rust
//! `str`s are typically accessed as immutable references: `&str`. Use
//! the owned `String` type defined in [`string`](string/index.html)
//! for building and mutating strings.
//!
//! For converting to strings use the [`format!`](fmt/index.html)
//! macro, and for converting from strings use the
//! [`FromStr`](str/trait.FromStr.html) trait.
//!
//! Data may be shared by placing it in a reference-counted box or the
//! [`Rc`](rc/index.html) type, and if further contained in a [`Cell`
//! or `RefCell`](cell/index.html), may be mutated as well as shared.
//! Likewise, in a concurrent setting it is common to pair an
//! atomically-reference-counted box, [`Arc`](sync/struct.Arc.html),
//! with a [`Mutex`](sync/struct.Mutex.html) to get the same effect.
//!
//! The [`collections`](collections/index.html) module defines maps,
//! sets, linked lists and other typical collection types, including
//! the common [`HashMap`](collections/struct.HashMap.html).
//!
//! ## Platform abstractions and I/O
//!
//! Besides basic data types, the standard library is largely concerned
//! with abstracting over differences in common platforms, most notably
//! Windows and Unix derivatives.
//!
//! Common types of I/O, including [files](fs/struct.File.html),
//! [TCP](net/struct.TcpStream.html),
//! [UDP](net/struct.UdpSocket.html), are defined in the
//! [`io`](io/index.html), [`fs`](fs/index.html), and
//! [`net`](net/index.html) modules.
//!
//! The [`thread`](thread/index.html) module contains Rust's threading
//! abstractions. [`sync`](sync/index.html) contains further
//! primitive shared memory types, including
//! [`atomic`](sync/atomic/index.html) and
//! [`mpsc`](sync/mpsc/index.html), which contains the channel types
//! for message passing.
//!

// Do not remove on snapshot creation. Needed for bootstrap. (Issue #22364)
#![cfg_attr(stage0, feature(custom_attribute))]
#![crate_name = "std"]
#![stable(feature = "rust1", since = "1.0.0")]
#![staged_api]
#![crate_type = "rlib"]
#![crate_type = "dylib"]
#![doc(html_logo_url = "http://www.rust-lang.org/logos/rust-logo-128x128-blk-v2.png",
       html_favicon_url = "https://doc.rust-lang.org/favicon.ico",
       html_root_url = "http://doc.rust-lang.org/nightly/",
       html_playground_url = "http://play.rust-lang.org/",
       test(no_crate_inject, attr(deny(warnings))),
       test(attr(allow(dead_code, deprecated, unused_variables, unused_mut))))]

#![feature(alloc)]
#![feature(allow_internal_unstable)]
#![feature(associated_consts)]
#![feature(borrow_state)]
#![feature(box_raw)]
#![feature(box_syntax)]
#![feature(char_from_unchecked)]
#![feature(char_internals)]
#![feature(clone_from_slice)]
#![feature(collections)]
#![feature(collections_bound)]
#![feature(const_fn)]
#![feature(core)]
#![feature(core_float)]
#![feature(core_intrinsics)]
#![feature(core_prelude)]
#![feature(core_simd)]
#![feature(drain)]
#![feature(fnbox)]
#![feature(heap_api)]
#![feature(int_error_internals)]
#![feature(into_cow)]
#![feature(iter_order)]
#![feature(lang_items)]
#![feature(libc)]
#![feature(linkage, thread_local, asm)]
#![feature(macro_reexport)]
#![feature(slice_concat_ext)]
#![feature(no_std)]
#![feature(oom)]
#![feature(optin_builtin_traits)]
#![feature(placement_in_syntax)]
#![feature(rand)]
#![feature(raw)]
#![feature(reflect_marker)]
#![feature(slice_bytes)]
#![feature(slice_patterns)]
#![feature(staged_api)]
#![feature(str_char)]
#![feature(str_internals)]
#![feature(unboxed_closures)]
#![feature(unicode)]
#![feature(unique)]
#![feature(unsafe_no_drop_flag, filling_drop)]
#![feature(vec_push_all)]
#![feature(vec_resize)]
#![feature(wrapping)]
#![feature(zero_one)]
#![cfg_attr(windows, feature(str_utf16))]
#![cfg_attr(test, feature(float_from_str_radix, range_inclusive, float_extras, hash_default))]
#![cfg_attr(test, feature(test, rustc_private, float_consts))]
#![cfg_attr(target_env = "msvc", feature(link_args))]

// Don't link to std. We are std.
#![no_std]

#![allow(trivial_casts)]
#![deny(missing_docs)]

#[cfg(test)] extern crate test;
#[cfg(test)] #[macro_use] extern crate log;

#[macro_use]
#[macro_reexport(assert, assert_eq, debug_assert, debug_assert_eq,
    unreachable, unimplemented, write, writeln)]
extern crate core;

#[macro_use]
#[macro_reexport(vec, format)]
extern crate collections as core_collections;

#[allow(deprecated)] extern crate rand as core_rand;
extern crate alloc;
extern crate rustc_unicode;
extern crate libc;

#[macro_use] #[no_link] extern crate rustc_bitflags;

// Make std testable by not duplicating lang items and other globals. See #2912
#[cfg(test)] extern crate std as realstd;

// NB: These reexports are in the order they should be listed in rustdoc

pub use core::any;
pub use core::cell;
pub use core::clone;
pub use core::cmp;
pub use core::convert;
pub use core::default;
pub use core::hash;
pub use core::intrinsics;
pub use core::iter;
pub use core::marker;
pub use core::mem;
pub use core::ops;
pub use core::ptr;
pub use core::raw;
pub use core::simd;
pub use core::result;
pub use core::option;
pub mod error;

pub use alloc::boxed;
pub use alloc::rc;

pub use core_collections::borrow;
pub use core_collections::fmt;
pub use core_collections::slice;
pub use core_collections::str;
pub use core_collections::string;
#[stable(feature = "rust1", since = "1.0.0")]
pub use core_collections::vec;

pub use rustc_unicode::char;

/* Exported macros */

#[macro_use]
mod macros;

mod rtdeps;

/* The Prelude. */

pub mod prelude;


/* Primitive types */

// NB: slice and str are primitive types too, but their module docs + primitive doc pages
// are inlined from the public re-exports of core_collections::{slice, str} above.

#[path = "num/float_macros.rs"]
#[macro_use]
mod float_macros;

#[path = "num/int_macros.rs"]
#[macro_use]
mod int_macros;

#[path = "num/uint_macros.rs"]
#[macro_use]
mod uint_macros;

#[path = "num/isize.rs"]  pub mod isize;
#[path = "num/i8.rs"]   pub mod i8;
#[path = "num/i16.rs"]  pub mod i16;
#[path = "num/i32.rs"]  pub mod i32;
#[path = "num/i64.rs"]  pub mod i64;

#[path = "num/usize.rs"] pub mod usize;
#[path = "num/u8.rs"]   pub mod u8;
#[path = "num/u16.rs"]  pub mod u16;
#[path = "num/u32.rs"]  pub mod u32;
#[path = "num/u64.rs"]  pub mod u64;

#[path = "num/f32.rs"]   pub mod f32;
#[path = "num/f64.rs"]   pub mod f64;

pub mod ascii;

pub mod thunk;

/* Common traits */

pub mod num;

/* Runtime and platform support */

#[macro_use]
pub mod thread;

pub mod collections;
pub mod dynamic_lib;
pub mod env;
pub mod ffi;
pub mod fs;
pub mod io;
pub mod net;
pub mod os;
pub mod path;
pub mod process;
pub mod sync;
pub mod time;

#[macro_use]
#[path = "sys/common/mod.rs"] mod sys_common;

#[cfg(unix)]
#[path = "sys/unix/mod.rs"] mod sys;
#[cfg(windows)]
#[path = "sys/windows/mod.rs"] mod sys;

pub mod rt;
mod panicking;
mod rand;

// Some external utilities of the standard library rely on randomness (aka
// rustc_back::TempDir and tests) and need a way to get at the OS rng we've got
// here. This module is not at all intended for stabilization as-is, however,
// but it may be stabilized long-term. As a result we're exposing a hidden,
// unstable module so we can get our build working.
#[doc(hidden)]
#[unstable(feature = "rand")]
pub mod __rand {
    pub use rand::{thread_rng, ThreadRng, Rng};
}

// Include a number of private modules that exist solely to provide
// the rustdoc documentation for primitive types. Using `include!`
// because rustdoc only looks for these modules at the crate level.
include!("primitive_docs.rs");

// The expansion of --test has a few references to `::std::$foo` so this module
// is necessary to get things to compile.
#[cfg(test)]
mod std {
    pub use option;
    pub use realstd::env;
}
