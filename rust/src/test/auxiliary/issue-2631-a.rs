// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[link(name = "req")];
#[crate_type = "lib"];
#[legacy_exports];

extern mod std;

use dvec::*;
use dvec::DVec;
use std::map::HashMap;

type header_map = HashMap<~str, @DVec<@~str>>;

// the unused ty param is necessary so this gets monomorphized
fn request<T: Copy>(req: header_map) {
  let _x = copy *(copy *req.get(~"METHOD"))[0u];
}
