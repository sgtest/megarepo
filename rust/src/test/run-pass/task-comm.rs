use std;

use task::task;
use comm::Chan;
use comm::Port;
use comm::send;
use comm::recv;

fn main() {
    test00();
    // test01();
    test02();
    test04();
    test05();
    test06();
}

fn test00_start(ch: Chan<int>, message: int, count: int) {
    debug!("Starting test00_start");
    let mut i: int = 0;
    while i < count {
        debug!("Sending Message");
        send(ch, message + 0);
        i = i + 1;
    }
    debug!("Ending test00_start");
}

fn test00() {
    let number_of_tasks: int = 1;
    let number_of_messages: int = 4;
    debug!("Creating tasks");

    let po = Port();
    let ch = Chan(po);

    let mut i: int = 0;

    let mut results = ~[];
    while i < number_of_tasks {
        i = i + 1;
        do task::task().future_result(|+r| {
            vec::push(results, r);
        }).spawn |copy i| {
            test00_start(ch, i, number_of_messages);
        }
    }
    let mut sum: int = 0;
    for results.each |r| {
        i = 0;
        while i < number_of_messages { sum += recv(po); i = i + 1; }
    }

    for results.each |r| { future::get(&r); }

    debug!("Completed: Final number is: ");
    assert (sum ==
                number_of_messages *
                    (number_of_tasks * number_of_tasks + number_of_tasks) /
                    2);
}

fn test01() {
    let p = Port();
    debug!("Reading from a port that is never written to.");
    let value: int = recv(p);
    log(debug, value);
}

fn test02() {
    let p = Port();
    let c = Chan(p);
    debug!("Writing to a local task channel.");
    send(c, 42);
    debug!("Reading from a local task port.");
    let value: int = recv(p);
    log(debug, value);
}

fn test04_start() {
    debug!("Started task");
    let mut i: int = 1024 * 1024;
    while i > 0 { i = i - 1; }
    debug!("Finished task");
}

fn test04() {
    debug!("Spawning lots of tasks.");
    let mut i: int = 4;
    while i > 0 { i = i - 1; task::spawn(|| test04_start() ); }
    debug!("Finishing up.");
}

fn test05_start(ch: Chan<int>) {
    send(ch, 10);
    send(ch, 20);
    send(ch, 30);
    send(ch, 30);
    send(ch, 30);
}

fn test05() {
    let po = comm::Port();
    let ch = Chan(po);
    task::spawn(|| test05_start(ch) );
    let mut value: int;
    value = recv(po);
    value = recv(po);
    value = recv(po);
    log(debug, value);
}

fn test06_start(&&task_number: int) {
    debug!("Started task.");
    let mut i: int = 0;
    while i < 1000000 { i = i + 1; }
    debug!("Finished task.");
}

fn test06() {
    let number_of_tasks: int = 4;
    debug!("Creating tasks");

    let mut i: int = 0;

    let mut results = ~[];
    while i < number_of_tasks {
        i = i + 1;
        do task::task().future_result(|+r| {
            vec::push(results, r);
        }).spawn |copy i| {
            test06_start(i);
        };
    }


    for results.each |r| { future::get(&r); }
}










