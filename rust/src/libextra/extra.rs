// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!

Rust extras.

The `extra` crate is a set of useful modules for a variety of
purposes, including collections, numerics, I/O, serialization,
and concurrency.

Rust extras are part of the standard Rust distribution.

*/

#[link(name = "extra",
       vers = "0.8",
       uuid = "122bed0b-c19b-4b82-b0b7-7ae8aead7297",
       url = "https://github.com/mozilla/rust/tree/master/src/libextra")];

#[doc(html_logo_url = "http://www.rust-lang.org/logos/rust-logo-128x128-blk.png",
      html_favicon_url = "http://www.rust-lang.org/favicon.ico",
      passes = "strip-hidden")];

#[comment = "Rust extras"];
#[license = "MIT/ASL2"];
#[crate_type = "lib"];

#[deny(non_camel_case_types)];
#[deny(missing_doc)];

use std::str::{StrSlice, OwnedStr};

pub use std::os;

// Utility modules

pub mod c_vec;
pub mod io_util;
pub mod rc;

// Concurrency

pub mod sync;
pub mod arc;
pub mod comm;
pub mod future;
pub mod task_pool;
pub mod flatpipes;

// Collections

pub mod container;
pub mod bitv;
pub mod list;
pub mod ringbuf;
pub mod priority_queue;
pub mod smallintmap;

pub mod sort;

pub mod dlist;
pub mod treemap;

// Crypto
#[path="crypto/cryptoutil.rs"]
mod cryptoutil;
#[path="crypto/digest.rs"]
pub mod digest;
#[path="crypto/md5.rs"]
pub mod md5;
#[path="crypto/sha1.rs"]
pub mod sha1;
#[path="crypto/sha2.rs"]
pub mod sha2;

// And ... other stuff

pub mod url;
pub mod ebml;
pub mod getopts;
pub mod json;
pub mod md4;
pub mod tempfile;
pub mod glob;
pub mod term;
pub mod time;
pub mod arena;
pub mod base64;
pub mod rl;
pub mod workcache;
pub mod enum_set;
#[path="num/bigint.rs"]
pub mod bigint;
#[path="num/rational.rs"]
pub mod rational;
#[path="num/complex.rs"]
pub mod complex;
pub mod stats;
pub mod semver;
pub mod fileinput;
pub mod flate;
pub mod hex;
pub mod uuid;


#[cfg(unicode)]
mod unicode;

#[path="terminfo/terminfo.rs"]
pub mod terminfo;

// Compiler support modules

pub mod test;
pub mod serialize;

// A curious inner-module that's not exported that contains the binding
// 'extra' so that macro-expanded references to extra::serialize and such
// can be resolved within libextra.
#[doc(hidden)]
pub mod extra {
    pub use serialize;
    pub use test;

    // For bootstrapping.
    pub use std::clone;
    pub use std::condition;
    pub use std::cmp;
    pub use std::sys;
    pub use std::unstable;
    pub use std::str;
    pub use std::os;
}
