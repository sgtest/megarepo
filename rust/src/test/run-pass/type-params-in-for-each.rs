

fn range(lo: uint, hi: uint, it: fn(uint)) {
    let mut lo_ = lo;
    while lo_ < hi { it(lo_); lo_ += 1u; }
}

fn create_index<T>(index: ~[{a: T, b: uint}], hash_fn: native fn(T) -> uint) {
    range(0u, 256u, {|_i| let bucket: ~[T] = ~[]; })
}

fn main() { }
