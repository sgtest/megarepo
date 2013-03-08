// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*
  Make sure we can spawn tasks that take different types of
  parameters. This is based on a test case for #520 provided by Rob
  Arnold.
 */

use core::comm::*;

type ctx = Chan<int>;

fn iotask(cx: &ctx, ip: ~str) {
    fail_unless!((ip == ~"localhost"));
}

pub fn main() {
    let (p, ch) = stream::<int>();
    task::spawn(|| iotask(&ch, ~"localhost") );
}
