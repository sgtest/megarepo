fn force(f: fn() -> int) -> int { ret f(); }
fn main() {
    fn f() -> int { ret 7; }
    assert (force(f) == 7);
    let g = bind force(f);
    assert (g() == 7);
}
