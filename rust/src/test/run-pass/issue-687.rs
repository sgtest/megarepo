use std;
import vec;
import task;
import comm;
import comm::chan;
import comm::port;
import comm::recv;
import comm::send;

enum msg { closed, received(~[u8]), }

fn producer(c: chan<~[u8]>) {
    send(c, ~[1u8, 2u8, 3u8, 4u8]);
    let empty: ~[u8] = ~[];
    send(c, empty);
}

fn packager(cb: chan<chan<~[u8]>>, msg: chan<msg>) {
    let p: port<~[u8]> = port();
    send(cb, chan(p));
    loop {
        debug!{"waiting for bytes"};
        let data = recv(p);
        debug!{"got bytes"};
        if vec::len(data) == 0u {
            debug!{"got empty bytes, quitting"};
            break;
        }
        debug!{"sending non-empty buffer of length"};
        log(debug, vec::len(data));
        send(msg, received(data));
        debug!{"sent non-empty buffer"};
    }
    debug!{"sending closed message"};
    send(msg, closed);
    debug!{"sent closed message"};
}

fn main() {
    let p: port<msg> = port();
    let ch = chan(p);
    let recv_reader: port<chan<~[u8]>> = port();
    let recv_reader_chan = chan(recv_reader);
    let pack = task::spawn(|| packager(recv_reader_chan, ch) );

    let source_chan: chan<~[u8]> = recv(recv_reader);
    let prod = task::spawn(|| producer(source_chan) );

    loop {
        let msg = recv(p);
        alt msg {
          closed => { debug!{"Got close message"}; break; }
          received(data) => {
            debug!{"Got data. Length is:"};
            log(debug, vec::len::<u8>(data));
          }
        }
    }
}
