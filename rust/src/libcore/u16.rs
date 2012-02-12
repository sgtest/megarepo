#[doc = "Operations and constants for `u16`"];

const min_value: u16 = 0u16;
const max_value: u16 = 0u16 - 1u16;

pure fn add(x: u16, y: u16) -> u16 { x + y }
pure fn sub(x: u16, y: u16) -> u16 { x - y }
pure fn mul(x: u16, y: u16) -> u16 { x * y }
pure fn div(x: u16, y: u16) -> u16 { x / y }
pure fn rem(x: u16, y: u16) -> u16 { x % y }

pure fn lt(x: u16, y: u16) -> bool { x < y }
pure fn le(x: u16, y: u16) -> bool { x <= y }
pure fn eq(x: u16, y: u16) -> bool { x == y }
pure fn ne(x: u16, y: u16) -> bool { x != y }
pure fn ge(x: u16, y: u16) -> bool { x >= y }
pure fn gt(x: u16, y: u16) -> bool { x > y }

pure fn positive(x: u16) -> bool { x > 0u16 }
pure fn negative(x: u16) -> bool { x < 0u16 }
pure fn nonpositive(x: u16) -> bool { x <= 0u16 }
pure fn nonnegative(x: u16) -> bool { x >= 0u16 }

#[doc = "Iterate over the range [`lo`..`hi`)"]
fn range(lo: u16, hi: u16, it: fn(u16)) {
    let i = lo;
    while i < hi { it(i); i += 1u16; }
}
