// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// error-pattern:explicit failure

// This time we're testing that the stack limits are restored
// correctly after calling into the C stack and unwinding.
// See the hack in upcall_call_shim_on_c_stack where it messes
// with the stack limit.

extern mod std;

extern mod rustrt {
    #[legacy_exports];
    fn last_os_error() -> ~str;
}

fn getbig_call_c_and_fail(i: int) {
    if i != 0 {
        getbig_call_c_and_fail(i - 1);
    } else {
        unsafe {
            rustrt::last_os_error();
            fail;
        }
    }
}

struct and_then_get_big_again {
  x:int,
}

impl and_then_get_big_again : Drop {
    fn finalize(&self) {
        fn getbig(i: int) {
            if i != 0 {
                getbig(i - 1);
            }
        }
        getbig(10000);
    }
}

fn and_then_get_big_again(x:int) -> and_then_get_big_again {
    and_then_get_big_again {
        x: x
    }
}

fn main() {
    do task::spawn {
        let r = and_then_get_big_again(4);
        getbig_call_c_and_fail(10000);
    };
}
