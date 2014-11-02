// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// aux-build:use_from_trait_xc.rs

extern crate use_from_trait_xc;

use use_from_trait_xc::Trait::foo;
//~^ ERROR `foo` is not directly importable

use use_from_trait_xc::Foo::new;
//~^ ERROR `new` is not directly importable

use use_from_trait_xc::Bar::new as bnew;
//~^ ERROR `bnew` is not directly importable

use use_from_trait_xc::Baz::new as baznew;
//~^ ERROR `baznew` is not directly importable

fn main() {}
