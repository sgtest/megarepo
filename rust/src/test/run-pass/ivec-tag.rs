use std;

import task;
import comm;
import comm::chan;
import comm::port;
import comm::send;
import comm::recv;

fn producer(c: chan<[u8]>) {
    send(c,
         [1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 7u8, 8u8, 9u8, 10u8, 11u8, 12u8,
          13u8]);
}

fn main() {
    let p: port<[u8]> = port();
    let prod = task::spawn(chan(p), producer);

    let data: [u8] = recv(p);
}
