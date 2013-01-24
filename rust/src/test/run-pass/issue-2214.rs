// xfail-fast

// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::cast;
use core::libc::{c_double, c_int};
use core::f64::*;

fn to_c_int(v: &mut int) -> &mut c_int {
    unsafe {
        cast::reinterpret_cast(&v)
    }
}

fn lgamma(n: c_double, value: &mut int) -> c_double {
    unsafe {
        return m::lgamma(n, to_c_int(value));
    }
}

#[link_name = "m"]
#[abi = "cdecl"]
extern mod m {
    #[cfg(unix)]
    #[link_name="lgamma_r"] pub fn lgamma(n: c_double, sign: &mut c_int)
      -> c_double;
    #[cfg(windows)]
    #[link_name="__lgamma_r"] pub fn lgamma(n: c_double,
                                            sign: &mut c_int) -> c_double;

}

fn main() {
  let mut y: int = 5;
  let x: &mut int = &mut y;
  assert (lgamma(1.0 as c_double, x) == 0.0 as c_double);
}
