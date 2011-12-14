use std;
import task;
import comm;
import comm::chan;
import comm::send;
import comm::recv;

fn main() { log "===== WITHOUT THREADS ====="; test00(); }

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
    let number_of_tasks: int = 16;
    let number_of_messages: int = 4;

    log "Creating tasks";

    let po = comm::port();
    let ch = chan(po);

    let i: int = 0;

    // Create and spawn tasks...
    let tasks = [];
    while i < number_of_tasks {
        tasks += [task::spawn_joinable(
            (ch, i, number_of_messages), test00_start)];
        i = i + 1;
    }

    // Read from spawned tasks...
    let sum = 0;
    for t in tasks {
        i = 0;
        while i < number_of_messages {
            let value = recv(po);
            sum += value;
            i = i + 1;
        }
    }

    // Join spawned tasks...
    for t in tasks { task::join(t); }

    log "Completed: Final number is: ";
    log_err sum;
    // assert (sum == (((number_of_tasks * (number_of_tasks - 1)) / 2) *
    //       number_of_messages));
    assert (sum == 480);
}
