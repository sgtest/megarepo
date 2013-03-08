// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// A raw test of vector appending performance.

extern mod std;
use core::dvec::DVec;
use core::io::WriterUtil;

fn collect_raw(num: uint) -> ~[uint] {
    let mut result = ~[];
    for uint::range(0u, num) |i| {
        result.push(i);
    }
    return result;
}

fn collect_dvec(num: uint) -> ~[uint] {
    let result = DVec();
    for uint::range(0u, num) |i| {
        result.push(i);
    }
    return dvec::unwrap(result);
}

fn main() {
    let args = os::args();
    let args = if os::getenv(~"RUST_BENCH").is_some() {
        ~[~"", ~"50000000"]
    } else if args.len() <= 1u {
        ~[~"", ~"100000"]
    } else {
        args
    };
    let max = uint::from_str(args[1]).get();
    let start = std::time::precise_time_s();
    let raw_v = collect_raw(max);
    let mid = std::time::precise_time_s();
    let dvec_v = collect_dvec(max);
    let end = std::time::precise_time_s();

    // check each vector
    fail_unless!(raw_v.len() == max);
    for raw_v.eachi |i, v| { fail_unless!(i == *v); }
    fail_unless!(dvec_v.len() == max);
    for dvec_v.eachi |i, v| { fail_unless!(i == *v); }

    let raw = mid - start;
    let dvec = end - mid;

    let maxf = max as float;
    let rawf = raw as float;
    let dvecf = dvec as float;
    
    io::stdout().write_str(fmt!("Raw     : %? seconds\n", raw));
    io::stdout().write_str(fmt!("        : %f op/sec\n", maxf/rawf));
    io::stdout().write_str(fmt!("\n"));
    io::stdout().write_str(fmt!("Dvec    : %? seconds\n", dvec));
    io::stdout().write_str(fmt!("        : %f op/sec\n", maxf/dvecf));
    io::stdout().write_str(fmt!("\n"));
    
    if dvec < raw {
        io::stdout().write_str(fmt!("Dvec is %f%% faster than raw\n",
                                    (rawf - dvecf) / rawf * 100.0));
    } else {
        io::stdout().write_str(fmt!("Raw is %f%% faster than dvec\n",
                                    (dvecf - rawf) / dvecf * 100.0));
    }
}
