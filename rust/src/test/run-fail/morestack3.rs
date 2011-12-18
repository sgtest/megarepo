// error-pattern:explicit failure

// Just testing unwinding

use std;

native mod rustrt {
    fn set_min_stack(size: uint);
}

fn getbig_and_fail(&&i: int) {
    let r = and_then_get_big_again(@0);
    if i != 0 {
        getbig_and_fail(i - 1);
    } else {
        fail;
    }
}

resource and_then_get_big_again(_i: @int) {
    fn getbig(i: int) {
        if i != 0 {
            getbig(i - 1);
        }
    }
    getbig(100);
}

fn main() {
    rustrt::set_min_stack(1024u);
    task::spawn(400, getbig_and_fail);
}