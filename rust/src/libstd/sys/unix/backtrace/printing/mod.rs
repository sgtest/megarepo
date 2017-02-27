// Copyright 2014-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

pub use self::imp::{foreach_symbol_fileline, resolve_symname};

#[cfg(any(target_os = "macos", target_os = "ios",
          target_os = "emscripten"))]
#[path = "dladdr.rs"]
mod imp;

#[cfg(not(any(target_os = "macos", target_os = "ios",
              target_os = "emscripten")))]
mod imp {
    pub use sys_common::gnu::libbacktrace::{foreach_symbol_fileline, resolve_symname};
}
