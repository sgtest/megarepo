// Issue #763

use std;
import task;
import comm::chan;
import comm::send;
import comm;
import comm::port;
import comm::recv;

enum request { quit, close(chan<bool>), }

type ctx = chan<request>;

fn request_task(c: chan<ctx>) {
    let p = port();
    send(c, chan(p));
    let mut req: request;
    req = recv(p);
    // Need to drop req before receiving it again
    req = recv(p);
}

fn new_cx() -> ctx {
    let p = port();
    let ch = chan(p);
    let t = task::spawn(|| request_task(ch) );
    let mut cx: ctx;
    cx = recv(p);
    return cx;
}

fn main() {
    let cx = new_cx();

    let p = port::<bool>();
    send(cx, close(chan(p)));
    send(cx, quit);
}
