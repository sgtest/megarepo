// xfail-fast feature doesn't work

#[feature(simd)];

#[simd]
struct RGBA {
    r: f32,
    g: f32,
    b: f32,
    a: f32
}

pub fn main() {}
