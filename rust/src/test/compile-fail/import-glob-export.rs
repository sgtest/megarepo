
// error-pattern:unresolved name

use m1::*;

mod m1 {
    export f1;
    fn f1() { }
    fn f2() { }
}

fn main() { f2(); }
