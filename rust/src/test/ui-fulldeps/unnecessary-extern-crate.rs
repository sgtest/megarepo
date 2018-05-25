// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// compile-flags: --edition 2018

#![deny(unnecessary_extern_crates)]
#![feature(alloc, test, libc)]

extern crate alloc;
//~^ ERROR `extern crate` is unnecessary in the new edition
//~| HELP remove
extern crate alloc as x;
//~^ ERROR `extern crate` is unnecessary in the new edition
//~| HELP use `use`

#[macro_use]
extern crate test;
pub extern crate test as y;
//~^ ERROR `extern crate` is unnecessary in the new edition
//~| HELP use `pub use`
pub extern crate libc;
//~^ ERROR `extern crate` is unnecessary in the new edition
//~| HELP use `pub use`


mod foo {
    extern crate alloc;
    //~^ ERROR `extern crate` is unnecessary in the new edition
    //~| HELP use `use`
    extern crate alloc as x;
    //~^ ERROR `extern crate` is unnecessary in the new edition
    //~| HELP use `use`
    pub extern crate test;
    //~^ ERROR `extern crate` is unnecessary in the new edition
    //~| HELP use `pub use`
    pub extern crate test as y;
    //~^ ERROR `extern crate` is unnecessary in the new edition
    //~| HELP use `pub use`
    mod bar {
        extern crate alloc;
        //~^ ERROR `extern crate` is unnecessary in the new edition
        //~| HELP use `use`
        extern crate alloc as x;
        //~^ ERROR `extern crate` is unnecessary in the new edition
        //~| HELP use `use`
    }
}


fn main() {}
