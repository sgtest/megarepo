fn getbig(i: int) {
    if i != 0 {
        getbig(i - 1);
    }
}

fn main() {
    getbig(100000);
}