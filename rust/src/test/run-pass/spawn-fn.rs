// -*- rust -*-

extern mod std;
use task::yield;

fn x(s: ~str, n: int) {
    log(debug, s);
    log(debug, n);
}

fn main() {
    task::spawn(|| x(~"hello from first spawned fn", 65) );
    task::spawn(|| x(~"hello from second spawned fn", 66) );
    task::spawn(|| x(~"hello from third spawned fn", 67) );
    let mut i: int = 30;
    while i > 0 { i = i - 1; debug!("parent sleeping"); yield(); }
}
