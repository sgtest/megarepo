use std;
import comm;

fn main() {
    let c = {
        let p = comm::port();
        comm::chan(p)
    };
    comm::send(c, "coffee");
}