// xfail-test
// compile-flags:--stack-growth
fn getbig(i: int) {
    if i != 0 {
        getbig(i - 1);
    } else {
        fail;
    }
}

fn main() {
    getbig(10000000);
}