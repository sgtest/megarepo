
use std;
import option;

fn foo<T>(y: option<T>) {
    let mut x: int;
    let mut rs: ~[int] = ~[];
    /* tests that x doesn't get put in the precondition for the
       entire if expression */

    if true {
    } else { alt y { none::<T> { x = 17; } _ { x = 42; } } rs += ~[x]; }
    return;
}

fn main() { debug!{"hello"}; foo::<int>(some::<int>(5)); }
