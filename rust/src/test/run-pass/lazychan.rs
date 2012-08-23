// -*- rust -*-

use std;
import task;
import comm::*;

fn main() {
    let p = port();
    let ch = chan(p);
    let mut y: int;

    task::spawn(|| child(ch) );
    y = recv(p);
    debug!("received 1");
    log(debug, y);
    assert (y == 10);

    task::spawn(|| child(ch) );
    y = recv(p);
    debug!("received 2");
    log(debug, y);
    assert (y == 10);
}

fn child(c: Chan<int>) { send(c, 10); }
