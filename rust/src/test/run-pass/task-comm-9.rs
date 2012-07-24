use std;
import task;
import comm;

fn main() { test00(); }

fn test00_start(c: comm::chan<int>, number_of_messages: int) {
    let mut i: int = 0;
    while i < number_of_messages { comm::send(c, i + 0); i += 1; }
}

fn test00() {
    let r: int = 0;
    let mut sum: int = 0;
    let p = comm::port();
    let number_of_messages: int = 10;
    let ch = comm::chan(p);

    let mut result = none;
    do task::task().future_result(|-r| { result = some(r); }).spawn {
        test00_start(ch, number_of_messages);
    }

    let mut i: int = 0;
    while i < number_of_messages {
        sum += comm::recv(p);
        log(debug, r);
        i += 1;
    }

    future::get(option::unwrap(result));

    assert (sum == number_of_messages * (number_of_messages - 1) / 2);
}
