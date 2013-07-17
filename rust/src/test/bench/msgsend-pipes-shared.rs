// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// A port of the simplistic benchmark from
//
//    http://github.com/PaulKeeble/ScalaVErlangAgents
//
// I *think* it's the same, more or less.

// This version uses pipes with a shared send endpoint. It should have
// different scalability characteristics compared to the select
// version.

extern mod extra;

use std::comm::{Port, Chan, SharedChan};
use std::comm;
use std::io::{Writer, WriterUtil};
use std::io;
use std::os;
use std::task;
use std::ptr;
use std::uint;
use std::vec;

fn move_out<T>(x: T) {}

enum request {
    get_count,
    bytes(uint),
    stop
}

fn server(requests: &Port<request>, responses: &comm::Chan<uint>) {
    let mut count = 0u;
    let mut done = false;
    while !done {
        match requests.try_recv() {
          Some(get_count) => { responses.send(count.clone()); }
          Some(bytes(b)) => {
            //error!("server: received %? bytes", b);
            count += b;
          }
          None => { done = true; }
          _ => { }
        }
    }
    responses.send(count);
    //error!("server exiting");
}

fn run(args: &[~str]) {
    let (from_child, to_parent) = comm::stream();
    let (from_parent, to_child) = comm::stream();

    let to_child = SharedChan::new(to_child);

    let size = uint::from_str(args[1]).get();
    let workers = uint::from_str(args[2]).get();
    let num_bytes = 100;
    let start = extra::time::precise_time_s();
    let mut worker_results = ~[];
    for uint::range(0, workers) |_i| {
        let to_child = to_child.clone();
        let mut builder = task::task();
        builder.future_result(|r| worker_results.push(r));
        do builder.spawn {
            for uint::range(0, size / workers) |_i| {
                //error!("worker %?: sending %? bytes", i, num_bytes);
                to_child.send(bytes(num_bytes));
            }
            //error!("worker %? exiting", i);
        }
    }
    do task::spawn || {
        server(&from_parent, &to_parent);
    }

    for worker_results.iter().advance |r| {
        r.recv();
    }

    //error!("sending stop message");
    to_child.send(stop);
    move_out(to_child);
    let result = from_child.recv();
    let end = extra::time::precise_time_s();
    let elapsed = end - start;
    io::stdout().write_str(fmt!("Count is %?\n", result));
    io::stdout().write_str(fmt!("Test took %? seconds\n", elapsed));
    let thruput = ((size / workers * workers) as float) / (elapsed as float);
    io::stdout().write_str(fmt!("Throughput=%f per sec\n", thruput));
    assert_eq!(result, num_bytes * size);
}

fn main() {
    let args = os::args();
    let args = if os::getenv(~"RUST_BENCH").is_some() {
        ~[~"", ~"1000000", ~"10000"]
    } else if args.len() <= 1u {
        ~[~"", ~"10000", ~"4"]
    } else {
        args.clone()
    };

    info!("%?", args);
    run(args);
}
