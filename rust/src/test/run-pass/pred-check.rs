


// -*- rust -*-
// xfail-stage0
pred f(q: int) -> bool { ret true; }

fn main() { let x = 0; check (f(x)); }