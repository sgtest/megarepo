// xfail-stage0

fn main() { let a = ~[1, 2, 3, 4, 5]; let b = [a, a]; b += b; }