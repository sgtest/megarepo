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

#![feature(duration, duration_span)]

use std::sync::mpsc::{channel, Sender, Receiver};
use std::env;
use std::thread;
use std::time::Duration;

enum request {
    get_count,
    bytes(usize),
    stop
}

fn server(requests: &Receiver<request>, responses: &Sender<usize>) {
    let mut count: usize = 0;
    let mut done = false;
    while !done {
        match requests.recv() {
          Ok(request::get_count) => { responses.send(count.clone()); }
          Ok(request::bytes(b)) => {
            //println!("server: received {} bytes", b);
            count += b;
          }
          Err(..) => { done = true; }
          _ => { }
        }
    }
    responses.send(count).unwrap();
    //println!("server exiting");
}

fn run(args: &[String]) {
    let (to_parent, from_child) = channel();

    let size = args[1].parse::<usize>().unwrap();
    let workers = args[2].parse::<usize>().unwrap();
    let num_bytes = 100;
    let mut result = None;
    let mut to_parent = Some(to_parent);
    let dur = Duration::span(|| {
        let to_parent = to_parent.take().unwrap();
        let mut worker_results = Vec::new();
        let from_parent = if workers == 1 {
            let (to_child, from_parent) = channel();
            worker_results.push(thread::spawn(move|| {
                for _ in 0..size / workers {
                    //println!("worker {}: sending {} bytes", i, num_bytes);
                    to_child.send(request::bytes(num_bytes));
                }
                //println!("worker {} exiting", i);
            }));
            from_parent
        } else {
            let (to_child, from_parent) = channel();
            for _ in 0..workers {
                let to_child = to_child.clone();
                worker_results.push(thread::spawn(move|| {
                    for _ in 0..size / workers {
                        //println!("worker {}: sending {} bytes", i, num_bytes);
                        to_child.send(request::bytes(num_bytes));
                    }
                    //println!("worker {} exiting", i);
                }));
            }
            from_parent
        };
        thread::spawn(move|| {
            server(&from_parent, &to_parent);
        });

        for r in worker_results {
            let _ = r.join();
        }

        //println!("sending stop message");
        //to_child.send(stop);
        //move_out(to_child);
        result = Some(from_child.recv().unwrap());
    });
    let result = result.unwrap();
    print!("Count is {}\n", result);
    print!("Test took {:?}\n", dur);
    let thruput = ((size / workers * workers) as f64) / (dur.as_secs() as f64);
    print!("Throughput={} per sec\n", thruput);
    assert_eq!(result, num_bytes * size);
}

fn main() {
    let args = env::args();
    let args = if env::var_os("RUST_BENCH").is_some() {
        vec!("".to_string(), "1000000".to_string(), "8".to_string())
    } else if args.len() <= 1 {
        vec!("".to_string(), "10000".to_string(), "4".to_string())
    } else {
        args.map(|x| x.to_string()).collect()
    };

    println!("{:?}", args);
    run(&args);
}
