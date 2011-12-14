use std;

import task;
import task::task;
import comm;
import comm::chan;
import comm::port;
import comm::send;
import comm::recv;

fn main() {
    test00();
    // test01();
    test02();
    test03();
    test04();
    test05();
    test06();
}

fn test00_start(&&args: (chan<int>, int, int)) {
    let (ch, message, count) = args;
    log "Starting test00_start";
    let i: int = 0;
    while i < count {
        log "Sending Message";
        send(ch, message + 0);
        i = i + 1;
    }
    log "Ending test00_start";
}

fn test00() {
    let number_of_tasks: int = 1;
    let number_of_messages: int = 4;
    log "Creating tasks";

    let po = port();
    let ch = chan(po);

    let i: int = 0;

    let tasks = [];
    while i < number_of_tasks {
        i = i + 1;
        tasks += [task::spawn_joinable(
            (ch, i, number_of_messages), test00_start)];
    }
    let sum: int = 0;
    for t in tasks {
        i = 0;
        while i < number_of_messages { sum += recv(po); i = i + 1; }
    }

    for t in tasks { task::join(t); }

    log "Completed: Final number is: ";
    assert (sum ==
                number_of_messages *
                    (number_of_tasks * number_of_tasks + number_of_tasks) /
                    2);
}

fn test01() {
    let p = port();
    log "Reading from a port that is never written to.";
    let value: int = recv(p);
    log value;
}

fn test02() {
    let p = port();
    let c = chan(p);
    log "Writing to a local task channel.";
    send(c, 42);
    log "Reading from a local task port.";
    let value: int = recv(p);
    log value;
}

obj vector(mutable x: int, y: int) {
    fn length() -> int { x = x + 2; ret x + y; }
}

fn test03() {
    log "Creating object ...";
    let v: vector = vector(1, 2);
    log "created object ...";
    let t: vector = v;
    log v.length();
}

fn test04_start(&&_args: ()) {
    log "Started task";
    let i: int = 1024 * 1024;
    while i > 0 { i = i - 1; }
    log "Finished task";
}

fn test04() {
    log "Spawning lots of tasks.";
    let i: int = 4;
    while i > 0 { i = i - 1; task::spawn((), test04_start); }
    log "Finishing up.";
}

fn test05_start(ch: chan<int>) {
    send(ch, 10);
    send(ch, 20);
    send(ch, 30);
    send(ch, 30);
    send(ch, 30);
}

fn test05() {
    let po = comm::port();
    let ch = chan(po);
    task::spawn(ch, test05_start);
    let value: int;
    value = recv(po);
    value = recv(po);
    value = recv(po);
    log value;
}

fn test06_start(&&task_number: int) {
    log "Started task.";
    let i: int = 0;
    while i < 1000000 { i = i + 1; }
    log "Finished task.";
}

fn test06() {
    let number_of_tasks: int = 4;
    log "Creating tasks";

    let i: int = 0;

    let tasks = [];
    while i < number_of_tasks {
        i = i + 1;
        tasks += [task::spawn_joinable(copy i, test06_start)];
    }


    for t in tasks { task::join(t); }
}










