// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
#![crate_name="inherited_stability"]
#![crate_type = "lib"]
#![unstable(feature = "test_feature")]
#![feature(staged_api)]
#![staged_api]

pub fn unstable() {}

#[stable(feature = "rust1", since = "1.0.0")]
pub fn stable() {}

#[stable(feature = "rust1", since = "1.0.0")]
pub mod stable_mod {
    pub fn unstable() {}

    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn stable() {}
}

#[unstable(feature = "test_feature")]
pub mod unstable_mod {
    #[stable(feature = "test_feature", since = "1.0.0")]
    #[deprecated(since = "1.0.0")]
    pub fn deprecated() {}

    pub fn unstable() {}
}

#[stable(feature = "rust1", since = "1.0.0")]
pub trait Stable {
    fn unstable(&self);

    #[stable(feature = "rust1", since = "1.0.0")]
    fn stable(&self);
}

impl Stable for usize {
    fn unstable(&self) {}
    fn stable(&self) {}
}

pub enum Unstable {
    UnstableVariant,
    #[stable(feature = "rust1", since = "1.0.0")]
    StableVariant
}
