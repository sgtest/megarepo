// Copyright 2018 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(step_trait)]

use std::iter::Step;

#[cfg(target_pointer_width = "16")]
fn main() {
    assert!(Step::steps_between(&0u32, &::std::u32::MAX).is_none());
}

#[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
fn main() {
    assert!(Step::steps_between(&0u32, &::std::u32::MAX).is_some());
}
