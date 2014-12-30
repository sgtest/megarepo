// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![macro_escape]

// NOTE(stage0): Remove cfg after a snapshot
#[cfg(not(stage0))]
/// Entry point of task panic, for details, see std::macros
#[macro_export]
macro_rules! panic {
    () => (
        panic!("explicit panic")
    );
    ($msg:expr) => ({
        static _MSG_FILE_LINE: (&'static str, &'static str, uint) = ($msg, file!(), line!());
        ::core::panicking::panic(&_MSG_FILE_LINE)
    });
    ($fmt:expr, $($arg:tt)*) => ({
        // The leading _'s are to avoid dead code warnings if this is
        // used inside a dead function. Just `#[allow(dead_code)]` is
        // insufficient, since the user may have
        // `#[forbid(dead_code)]` and which cannot be overridden.
        static _FILE_LINE: (&'static str, uint) = (file!(), line!());
        ::core::panicking::panic_fmt(format_args!($fmt, $($arg)*), &_FILE_LINE)
    });
}

// NOTE(stage0): Remove macro after a snapshot
#[cfg(stage0)]
/// Entry point of task panic, for details, see std::macros
#[macro_export]
macro_rules! panic {
    () => (
        panic!("{}", "explicit panic")
    );
    ($msg:expr) => ({
        static _MSG_FILE_LINE: (&'static str, &'static str, uint) = ($msg, file!(), line!());
        ::core::panicking::panic(&_MSG_FILE_LINE)
    });
    ($fmt:expr, $($arg:tt)*) => ({
        // a closure can't have return type !, so we need a full
        // function to pass to format_args!, *and* we need the
        // file and line numbers right here; so an inner bare fn
        // is our only choice.
        //
        // LLVM doesn't tend to inline this, presumably because begin_unwind_fmt
        // is #[cold] and #[inline(never)] and because this is flagged as cold
        // as returning !. We really do want this to be inlined, however,
        // because it's just a tiny wrapper. Small wins (156K to 149K in size)
        // were seen when forcing this to be inlined, and that number just goes
        // up with the number of calls to panic!()
        //
        // The leading _'s are to avoid dead code warnings if this is
        // used inside a dead function. Just `#[allow(dead_code)]` is
        // insufficient, since the user may have
        // `#[forbid(dead_code)]` and which cannot be overridden.
        #[inline(always)]
        fn _run_fmt(fmt: &::std::fmt::Arguments) -> ! {
            static _FILE_LINE: (&'static str, uint) = (file!(), line!());
            ::core::panicking::panic_fmt(fmt, &_FILE_LINE)
        }
        format_args!(_run_fmt, $fmt, $($arg)*)
    });
}

/// Runtime assertion, for details see std::macros
#[macro_export]
macro_rules! assert {
    ($cond:expr) => (
        if !$cond {
            panic!(concat!("assertion failed: ", stringify!($cond)))
        }
    );
    ($cond:expr, $($arg:tt)*) => (
        if !$cond {
            panic!($($arg)*)
        }
    );
}

/// Runtime assertion for equality, for details see std::macros
#[macro_export]
macro_rules! assert_eq {
    ($cond1:expr, $cond2:expr) => ({
        let c1 = $cond1;
        let c2 = $cond2;
        if c1 != c2 || c2 != c1 {
            panic!("expressions not equal, left: {}, right: {}", c1, c2);
        }
    })
}

/// Runtime assertion for equality, only without `--cfg ndebug`
#[macro_export]
macro_rules! debug_assert_eq {
    ($($a:tt)*) => ({
        if cfg!(not(ndebug)) {
            assert_eq!($($a)*);
        }
    })
}

/// Runtime assertion, disableable at compile time with `--cfg ndebug`
#[macro_export]
macro_rules! debug_assert {
    ($($arg:tt)*) => (if cfg!(not(ndebug)) { assert!($($arg)*); })
}

/// Short circuiting evaluation on Err
#[macro_export]
macro_rules! try {
    ($e:expr) => (match $e { Ok(e) => e, Err(e) => return Err(e) })
}

// NOTE(stage0): Remove cfg after a snapshot
#[cfg(not(stage0))]
/// Writing a formatted string into a writer
#[macro_export]
macro_rules! write {
    ($dst:expr, $($arg:tt)*) => ((&mut *$dst).write_fmt(format_args!($($arg)*)))
}

// NOTE(stage0): Remove macro after a snapshot
#[cfg(stage0)]
/// Writing a formatted string into a writer
#[macro_export]
macro_rules! write {
    ($dst:expr, $($arg:tt)*) => ({
        let dst = &mut *$dst;
        format_args!(|args| { dst.write_fmt(args) }, $($arg)*)
    })
}

/// Writing a formatted string plus a newline into a writer
#[macro_export]
macro_rules! writeln {
    ($dst:expr, $fmt:expr $($arg:tt)*) => (
        write!($dst, concat!($fmt, "\n") $($arg)*)
    )
}

#[macro_export]
macro_rules! unreachable { () => (panic!("unreachable code")) }

