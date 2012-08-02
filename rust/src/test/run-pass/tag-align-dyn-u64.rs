// xfail-test

tag a_tag<A> {
    a_tag(A);
}

type t_rec = {
    c8: u8,
    t: a_tag<u64>
};

fn mk_rec() -> t_rec {
    return { c8:0u8, t:a_tag(0u64) };
}

fn is_8_byte_aligned(&&u: a_tag<u64>) -> bool {
    let p = ptr::addr_of(u) as uint;
    return (p & 7u) == 0u;
}

fn main() {
    let x = mk_rec();
    assert is_8_byte_aligned(x.t);
}
