use std;
import comm;

fn main() {
    let p = comm::port();
    let c = comm::chan(p);
    comm::send(c, ~"coffee");
}