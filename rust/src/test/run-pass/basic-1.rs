// -*- rust -*-

use std;
import comm::Chan;
import comm::chan;
import comm::port;
import comm::send;
import comm::recv;
import task;

fn a(c: Chan<int>) { send(c, 10); }

fn main() {
    let p = port();
    let ch = chan(p);
    task::spawn(|| a(ch) );
    task::spawn(|| a(ch) );
    let mut n: int = 0;
    n = recv(p);
    n = recv(p);
    //    debug!{"Finished."};
}

fn b(c: Chan<int>) {
    //    debug!{"task b0"};
    //    debug!{"task b1"};
    //    debug!{"task b2"};
    //    debug!{"task b3"};
    //    debug!{"task b4"};
    //    debug!{"task b5"};
    send(c, 10);
}
