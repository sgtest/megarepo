


// -*- rust -*-
type t = int;

fn nothing() { }

fn putstr(s: ~str) { }

fn putint(i: int) {
    let mut i: int = 33;
    while i < 36 { putstr(~"hi"); i = i + 1; }
}

fn zerg(i: int) -> int { ret i; }

fn foo(x: int) -> int {
    let mut y: t = x + 2;
    putstr(~"hello");
    while y < 10 { putint(y); if y * 3 == 4 { y = y + 2; nothing(); } }
    let mut z: t;
    z = 0x55;
    foo(z);
    ret 0;
}

fn main() {
    let x: int = 2 + 2;
    log(debug, x);
    debug!{"hello, world"};
    log(debug, 10);
}
