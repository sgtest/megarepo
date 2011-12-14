// error-pattern:'member' is glob-imported from multiple different modules
// issue #482

use std;
// expecting swap to be defined in vec
import vec::*;
import alternate_supplier::*;

mod alternate_supplier {
    fn member() { }
}

fn main() { member() }
