use std;
import str::*;
import uint::*;

fn main() unsafe {
    let a: uint = 1u;
    let b: uint = 4u;
    check (le(a, b));
    let c <- a;
    log(debug, str::unsafe::safe_slice("kitties", c, b));
}
