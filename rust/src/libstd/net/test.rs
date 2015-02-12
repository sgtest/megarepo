// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use prelude::v1::*;

use env;
use net::{SocketAddr, IpAddr};
use sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT, Ordering};

pub fn next_test_ip4() -> SocketAddr {
    static PORT: AtomicUsize = ATOMIC_USIZE_INIT;
    SocketAddr::new(IpAddr::new_v4(127, 0, 0, 1),
                    PORT.fetch_add(1, Ordering::SeqCst) as u16 + base_port())
}

pub fn next_test_ip6() -> SocketAddr {
    static PORT: AtomicUsize = ATOMIC_USIZE_INIT;
    SocketAddr::new(IpAddr::new_v6(0, 0, 0, 0, 0, 0, 0, 1),
                    PORT.fetch_add(1, Ordering::SeqCst) as u16 + base_port())
}

// The bots run multiple builds at the same time, and these builds
// all want to use ports. This function figures out which workspace
// it is running in and assigns a port range based on it.
fn base_port() -> u16 {
    let cwd = env::current_dir().unwrap();
    let dirs = ["32-opt", "32-nopt", "64-opt", "64-nopt", "64-opt-vg",
                "all-opt", "snap3", "dist"];
    dirs.iter().enumerate().find(|&(i, dir)| {
        cwd.as_str().unwrap().contains(dir)
    }).map(|p| p.0).unwrap_or(0) as u16 * 1000 + 19600
}
