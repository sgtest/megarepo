// xfail-win32
use std;
import task;
import comm;
import uint;

fn die() {
    fail;
}

fn iloop() {
    task::spawn(|| die() );
    let p = comm::port::<()>();
    let c = comm::chan(p);
    loop {
        // Sending and receiving here because these actions yield,
        // at which point our child can kill us
        comm::send(c, ());
        comm::recv(p);
    }
}

fn main() {
    for uint::range(0u, 16u) |_i| {
        task::spawn_unlinked(|| iloop() );
    }
}
