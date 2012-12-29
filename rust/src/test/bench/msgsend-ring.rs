// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// This test creates a bunch of tasks that simultaneously send to each
// other in a ring. The messages should all be basically
// independent. It's designed to hammer the global kernel lock, so
// that things will look really good once we get that lock out of the
// message path.

use core::oldcomm::*;
use core::oldcomm;

extern mod std;
use std::time;
use std::future;

fn thread_ring(i: uint,
               count: uint,
               num_chan: oldcomm::Chan<uint>,
               num_port: oldcomm::Port<uint>) {
    // Send/Receive lots of messages.
    for uint::range(0u, count) |j| {
        num_chan.send(i * j);
        num_port.recv();
    };
}

fn main() {
    let args = os::args();
    let args = if os::getenv(~"RUST_BENCH").is_some() {
        ~[~"", ~"100", ~"10000"]
    } else if args.len() <= 1u {
        ~[~"", ~"100", ~"1000"]
    } else {
        args
    };        

    let num_tasks = uint::from_str(args[1]).get();
    let msg_per_task = uint::from_str(args[2]).get();

    let num_port = Port();
    let mut num_chan = Chan(&num_port);

    let start = time::precise_time_s();

    // create the ring
    let mut futures = ~[];

    for uint::range(1u, num_tasks) |i| {
        let get_chan = Port();
        let get_chan_chan = Chan(&get_chan);

        let new_future = do future::spawn
            |copy num_chan, move get_chan_chan| {
            let p = Port();
            get_chan_chan.send(Chan(&p));
            thread_ring(i, msg_per_task, num_chan,  p)
        };
        futures.push(move new_future);
        
        num_chan = get_chan.recv();
    };

    // do our iteration
    thread_ring(0u, msg_per_task, num_chan, num_port);

    // synchronize
    for futures.each |f| { f.get() };

    let stop = time::precise_time_s();

    // all done, report stats.
    let num_msgs = num_tasks * msg_per_task;
    let elapsed = (stop - start);
    let rate = (num_msgs as float) / elapsed;

    io::println(fmt!("Sent %? messages in %? seconds",
                     num_msgs, elapsed));
    io::println(fmt!("  %? messages / second", rate));
    io::println(fmt!("  %? μs / message", 1000000. / rate));
}
