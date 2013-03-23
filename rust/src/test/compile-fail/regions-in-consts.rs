// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

static c_x: &'blk int = &22; //~ ERROR Illegal lifetime 'blk: only 'static is allowed here
static c_y: &int = &22; //~ ERROR Illegal anonymous lifetime: only 'static is allowed here
static c_z: &'static int = &22;

fn main() {
}
