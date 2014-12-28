// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Macros used by the runtime.
//!
//! These macros call functions which are only accessible in the `rt` module, so
//! they aren't defined anywhere outside of the `rt` module.

#![macro_escape]

// NOTE(stage0): Remove cfg after a snapshot
#[cfg(not(stage0))]
macro_rules! rterrln {
    ($fmt:expr $($arg:tt)*) => ( {
        ::rt::util::dumb_print(format_args!(concat!($fmt, "\n") $($arg)*))
    } )
}

// NOTE(stage0): Remove macro after a snapshot
#[cfg(stage0)]
macro_rules! rterrln {
    ($fmt:expr $($arg:tt)*) => ( {
        format_args!(::rt::util::dumb_print, concat!($fmt, "\n") $($arg)*)
    } )
}

// Some basic logging. Enabled by passing `--cfg rtdebug` to the libstd build.
macro_rules! rtdebug {
    ($($arg:tt)*) => ( {
        if cfg!(rtdebug) {
            rterrln!($($arg)*)
        }
    })
}

macro_rules! rtassert {
    ( $arg:expr ) => ( {
        if ::rt::util::ENFORCE_SANITY {
            if !$arg {
                rtabort!(" assertion failed: {}", stringify!($arg));
            }
        }
    } )
}

// NOTE(stage0): Remove cfg after a snapshot
#[cfg(not(stage0))]
macro_rules! rtabort {
    ($($arg:tt)*) => (::rt::util::abort(format_args!($($arg)*)))
}

// NOTE(stage0): Remove macro after a snapshot
#[cfg(stage0)]
macro_rules! rtabort {
    ($($arg:tt)*) => (format_args!(::rt::util::abort, $($arg)*))
}
