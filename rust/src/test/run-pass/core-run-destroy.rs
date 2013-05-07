// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// xfail-fast
// compile-flags:--test

// NB: These tests kill child processes. Valgrind sees these children as leaking
// memory, which makes for some *confusing* logs. That's why these are here
// instead of in core.

use core::run;
use core::run::*;

#[test]
fn test_destroy_once() {
    let mut p = run::start_program("echo", []);
    p.destroy(); // this shouldn't crash (and nor should the destructor)
}

#[test]
fn test_destroy_twice() {
    let mut p = run::start_program("echo", []);
    p.destroy(); // this shouldnt crash...
    p.destroy(); // ...and nor should this (and nor should the destructor)
}

fn test_destroy_actually_kills(force: bool) {

    #[cfg(unix)]
    static BLOCK_COMMAND: &'static str = "cat";

    #[cfg(windows)]
    static BLOCK_COMMAND: &'static str = "cmd";

    #[cfg(unix)]
    fn process_exists(pid: libc::pid_t) -> bool {
        run::program_output("ps", [~"-p", pid.to_str()]).out.contains(pid.to_str())
    }

    #[cfg(windows)]
    fn process_exists(pid: libc::pid_t) -> bool {

        use core::libc::types::os::arch::extra::DWORD;
        use core::libc::funcs::extra::kernel32::{CloseHandle, GetExitCodeProcess, OpenProcess};
        use core::libc::consts::os::extra::{FALSE, PROCESS_QUERY_INFORMATION, STILL_ACTIVE };

        unsafe {
            let proc = OpenProcess(PROCESS_QUERY_INFORMATION, FALSE, pid as DWORD);
            if proc.is_null() {
                return false;
            }
            // proc will be non-null if the process is alive, or if it died recently
            let mut status = 0;
            GetExitCodeProcess(proc, &mut status);
            CloseHandle(proc);
            return status == STILL_ACTIVE;
        }
    }

    // this program will stay alive indefinitely trying to read from stdin
    let mut p = run::start_program(BLOCK_COMMAND, []);

    assert!(process_exists(p.get_id()));

    if force {
        p.force_destroy();
    } else {
        p.destroy();
    }

    assert!(!process_exists(p.get_id()));
}

#[test]
fn test_unforced_destroy_actually_kills() {
    test_destroy_actually_kills(false);
}

#[test]
fn test_forced_destroy_actually_kills() {
    test_destroy_actually_kills(true);
}
