use std;
import option::*;

fn main() {
    let i: int =
        alt some::<int>(3) { none::<int>. { fail } some::<int>(_) { 5 } };
    log i;
}
