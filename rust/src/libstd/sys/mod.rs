// Copyright 2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Platform-dependent platform abstraction
//!
//! The `std::sys` module is the abstracted interface through which
//! `std` talks to the underlying operating system. It has different
//! implementations for different operating system families, today
//! just Unix and Windows, and initial support for Redox.
//!
//! The centralization of platform-specific code in this module is
//! enforced by the "platform abstraction layer" tidy script in
//! `tools/tidy/src/pal.rs`.
//!
//! This module is closely related to the platform-independent system
//! integration code in `std::sys_common`. See that module's
//! documentation for details.
//!
//! In the future it would be desirable for the independent
//! implementations of this module to be extracted to their own crates
//! that `std` can link to, thus enabling their implementation
//! out-of-tree via crate replacement. Though due to the complex
//! inter-dependencies within `std` that will be a challenging goal to
//! achieve.

#![allow(missing_debug_implementations)]

pub use self::imp::*;

#[cfg(unix)]
#[path = "unix/mod.rs"]
mod imp;

#[cfg(windows)]
#[path = "windows/mod.rs"]
mod imp;

#[cfg(target_os = "redox")]
#[path = "redox/mod.rs"]
mod imp;

#[cfg(all(target_arch = "wasm32", not(target_os = "emscripten")))]
#[path = "wasm/mod.rs"]
mod imp;

// Import essential modules from both platforms when documenting.

#[cfg(all(dox, not(unix)))]
use os::linux as platform;

#[cfg(all(dox, not(any(unix, target_os = "redox"))))]
#[path = "unix/ext/mod.rs"]
pub mod unix_ext;

#[cfg(all(dox, any(unix, target_os = "redox")))]
pub use self::ext as unix_ext;


#[cfg(all(dox, not(windows)))]
#[macro_use]
#[path = "windows/compat.rs"]
mod compat;

#[cfg(all(dox, not(windows)))]
#[path = "windows/c.rs"]
mod c;

#[cfg(all(dox, not(windows)))]
#[path = "windows/ext/mod.rs"]
pub mod windows_ext;

#[cfg(all(dox, windows))]
pub use self::ext as windows_ext;
