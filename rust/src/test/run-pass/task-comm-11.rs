use std;
import comm;
import task;

fn start(c: comm::chan<comm::chan<int>>) {
    let p: comm::port<int> = comm::port();
    comm::send(c, comm::chan(p));
}

fn main() {
    let p = comm::port();
    let child = task::spawn(comm::chan(p), start);
    let c = comm::recv(p);
}
