


// -*- rust -*-
fn len(v: [const int]) -> uint {
    let mut i = 0u;
    for x: int in v { i += 1u; }
    ret i;
}

fn main() {
    let v0 = [1, 2, 3, 4, 5];
    log(debug, len(v0));
    let v1 = [mutable 1, 2, 3, 4, 5];
    log(debug, len(v1));
}
