// -*- rust -*-

use std;
import comm;
import comm::chan;
import comm::send;
import comm::recv;
import task;

fn main() {
    let p = comm::port();
    let ch = comm::chan(p);
    let t = task::spawn(|| child(ch) );
    let y = recv(p);
    error!{"received"};
    log(error, y);
    assert (y == 10);
}

fn child(c: chan<int>) {
    error!{"sending"};
    send(c, 10);
    error!{"value sent"};
}
