#[doc = "Operations and constants for `u8`"];

const min_value: u8 = 0u8;
const max_value: u8 = 0u8 - 1u8;

pure fn min(x: u8, y: u8) -> u8 { if x < y { x } else { y } }
pure fn max(x: u8, y: u8) -> u8 { if x > y { x } else { y } }

pure fn add(x: u8, y: u8) -> u8 { ret x + y; }
pure fn sub(x: u8, y: u8) -> u8 { ret x - y; }
pure fn mul(x: u8, y: u8) -> u8 { ret x * y; }
pure fn div(x: u8, y: u8) -> u8 { ret x / y; }
pure fn rem(x: u8, y: u8) -> u8 { ret x % y; }

pure fn lt(x: u8, y: u8) -> bool { ret x < y; }
pure fn le(x: u8, y: u8) -> bool { ret x <= y; }
pure fn eq(x: u8, y: u8) -> bool { ret x == y; }
pure fn ne(x: u8, y: u8) -> bool { ret x != y; }
pure fn ge(x: u8, y: u8) -> bool { ret x >= y; }
pure fn gt(x: u8, y: u8) -> bool { ret x > y; }

pure fn is_ascii(x: u8) -> bool { ret 0u8 == x & 128u8; }

#[doc = "Iterate over the range [`lo`..`hi`)"]
fn range(lo: u8, hi: u8, it: fn(u8)) {
    let mut i = lo;
    while i < hi { it(i); i += 1u8; }
}

#[doc = "Computes the bitwise complement"]
pure fn compl(i: u8) -> u8 {
    max_value ^ i
}

// Local Variables:
// mode: rust;
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
