


// -*- rust -*-
fn my_err(s: ~str) -> ! { log(error, s); fail; }

fn okay(i: uint) -> int {
    if i == 3u { my_err(~"I don't like three"); } else { return 42; }
}

fn main() { okay(4u); }
