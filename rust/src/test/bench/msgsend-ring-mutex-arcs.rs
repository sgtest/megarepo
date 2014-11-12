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
// independent.
// This is like msgsend-ring-pipes but adapted to use Arcs.

// This also serves as a pipes test, because Arcs are implemented with pipes.

// no-pretty-expanded FIXME #15189
// ignore-lexer-test FIXME #15679

use std::os;
use std::sync::{Arc, Future, Mutex};
use std::time::Duration;
use std::uint;

// A poor man's pipe.
type pipe = Arc<Mutex<Vec<uint>>>;

fn send(p: &pipe, msg: uint) {
    let mut arr = p.lock();
    arr.push(msg);
    arr.cond.signal();
}
fn recv(p: &pipe) -> uint {
    let mut arr = p.lock();
    while arr.is_empty() {
        arr.cond.wait();
    }
    arr.pop().unwrap()
}

fn init() -> (pipe,pipe) {
    let m = Arc::new(Mutex::new(Vec::new()));
    ((&m).clone(), m)
}


fn thread_ring(i: uint, count: uint, num_chan: pipe, num_port: pipe) {
    let mut num_chan = Some(num_chan);
    let mut num_port = Some(num_port);
    // Send/Receive lots of messages.
    for j in range(0u, count) {
        //println!("task %?, iter %?", i, j);
        let num_chan2 = num_chan.take().unwrap();
        let num_port2 = num_port.take().unwrap();
        send(&num_chan2, i * j);
        num_chan = Some(num_chan2);
        let _n = recv(&num_port2);
        //log(error, _n);
        num_port = Some(num_port2);
    };
}

fn main() {
    let args = os::args();
    let args = if os::getenv("RUST_BENCH").is_some() {
        vec!("".to_string(), "100".to_string(), "10000".to_string())
    } else if args.len() <= 1u {
        vec!("".to_string(), "10".to_string(), "100".to_string())
    } else {
        args.clone().into_iter().collect()
    };

    let num_tasks = from_str::<uint>(args[1].as_slice()).unwrap();
    let msg_per_task = from_str::<uint>(args[2].as_slice()).unwrap();

    let (mut num_chan, num_port) = init();

    let mut p = Some((num_chan, num_port));
    let dur = Duration::span(|| {
        let (mut num_chan, num_port) = p.take().unwrap();

        // create the ring
        let mut futures = Vec::new();

        for i in range(1u, num_tasks) {
            //println!("spawning %?", i);
            let (new_chan, num_port) = init();
            let num_chan_2 = num_chan.clone();
            let new_future = Future::spawn(proc() {
                thread_ring(i, msg_per_task, num_chan_2, num_port)
            });
            futures.push(new_future);
            num_chan = new_chan;
        };

        // do our iteration
        thread_ring(0, msg_per_task, num_chan, num_port);

        // synchronize
        for f in futures.iter_mut() {
            f.get()
        }
    });

    // all done, report stats.
    let num_msgs = num_tasks * msg_per_task;
    let rate = (num_msgs as f64) / (dur.num_milliseconds() as f64);

    println!("Sent {} messages in {} ms", num_msgs, dur.num_milliseconds());
    println!("  {} messages / second", rate / 1000.0);
    println!("  {} μs / message", 1000000. / rate / 1000.0);
}
