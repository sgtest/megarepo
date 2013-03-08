// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

mod bar {
    #[abi = "cdecl"]
    #[nolink]
    pub extern {}
}

mod zed {
    #[abi = "cdecl"]
    #[nolink]
    pub extern {}
}

mod libc {
    #[abi = "cdecl"]
    #[nolink]
    pub extern {
        pub fn write(fd: int, buf: *u8, count: ::core::libc::size_t)
                  -> ::core::libc::ssize_t;
    }
}

mod baz {
    #[abi = "cdecl"]
    #[nolink]
    pub extern {}
}

pub fn main() { }
