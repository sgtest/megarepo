
iter range(uint lo, uint hi) -> uint {
    auto lo_ = lo;
    while (lo_ < hi) {
        put lo_;
        lo_ += 1u;
    }
}

fn create_index[T](vec[tup(T, uint)] index, fn(&T) -> uint hash_fn) {
    for each (uint i in range(0u, 256u)) {
        let vec[T] bucket = vec();
    }
}

fn main() {}

