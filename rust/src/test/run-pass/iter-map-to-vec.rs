fn inc(x: &uint) -> uint { *x + 1u }

fn main() {
    assert [1u, 3u]/_.map_to_vec(inc) == ~[2u, 4u];
    assert [1u, 2u, 3u]/_.map_to_vec(inc) == ~[2u, 3u, 4u];
    assert None.map_to_vec(inc) == ~[];
    assert Some(1u).map_to_vec(inc) == ~[2u];
    assert Some(2u).map_to_vec(inc) == ~[3u];
}
