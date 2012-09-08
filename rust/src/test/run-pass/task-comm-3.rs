use std;
use pipes::Chan;
use pipes::send;
use pipes::recv;

fn main() { debug!("===== WITHOUT THREADS ====="); test00(); }

fn test00_start(ch: Chan<int>, message: int, count: int) {
    debug!("Starting test00_start");
    let mut i: int = 0;
    while i < count {
        debug!("Sending Message");
        ch.send(message + 0);
        i = i + 1;
    }
    debug!("Ending test00_start");
}

fn test00() {
    let number_of_tasks: int = 16;
    let number_of_messages: int = 4;

    debug!("Creating tasks");

    let po = pipes::PortSet();

    let mut i: int = 0;

    // Create and spawn tasks...
    let mut results = ~[];
    while i < number_of_tasks {
        let ch = po.chan();        
        do task::task().future_result(|+r| {
            vec::push(results, r);
        }).spawn |copy i| {
            test00_start(ch, i, number_of_messages)
        }
        i = i + 1;
    }

    // Read from spawned tasks...
    let mut sum = 0;
    for results.each |r| {
        i = 0;
        while i < number_of_messages {
            let value = po.recv();
            sum += value;
            i = i + 1;
        }
    }

    // Join spawned tasks...
    for results.each |r| { future::get(&r); }

    debug!("Completed: Final number is: ");
    log(error, sum);
    // assert (sum == (((number_of_tasks * (number_of_tasks - 1)) / 2) *
    //       number_of_messages));
    assert (sum == 480);
}
