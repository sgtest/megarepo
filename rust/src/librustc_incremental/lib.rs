// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Support for serializing the dep-graph and reloading it.

#![doc(html_logo_url = "https://www.rust-lang.org/logos/rust-logo-128x128-blk-v2.png",
      html_favicon_url = "https://doc.rust-lang.org/favicon.ico",
      html_root_url = "https://doc.rust-lang.org/nightly/")]
#![deny(warnings)]

#![feature(conservative_impl_trait)]
#![feature(fs_read_write)]
#![feature(i128_type)]
#![feature(inclusive_range_syntax)]
#![feature(specialization)]

extern crate graphviz;
#[macro_use] extern crate rustc;
extern crate rustc_data_structures;
extern crate serialize as rustc_serialize;
extern crate rand;

#[macro_use] extern crate log;
extern crate syntax;
extern crate syntax_pos;

mod assert_dep_graph;
pub mod assert_module_sources;
mod persist;

pub use assert_dep_graph::assert_dep_graph;
pub use persist::dep_graph_tcx_init;
pub use persist::load_dep_graph;
pub use persist::load_query_result_cache;
pub use persist::save_dep_graph;
pub use persist::save_trans_partition;
pub use persist::save_work_products;
pub use persist::in_incr_comp_dir;
pub use persist::prepare_session_directory;
pub use persist::finalize_session_directory;
pub use persist::delete_workproduct_files;
pub use persist::garbage_collect_session_directories;
