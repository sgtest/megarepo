

// error-pattern:explicit failure
use std;
import option::*;

fn foo(s: str) { }

fn main() {
    let i =
        alt some::<int>(3) { none::<int>. { fail } some::<int>(_) { fail } };
    foo(i);
}
