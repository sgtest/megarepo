


// -*- rust -*-
fn main() {
    let a: ~str = ~"hello";
    let b: ~str = ~"world";
    let s: ~str = a + b;
    log(debug, copy s);
    assert (s[9] == 'd' as u8);
}
