enum blah { a(int, int, uint), b(int, int), c, }

fn or_alt(q: blah) -> int {
    alt q { a(x, y, _) | b(x, y) => { return x + y; } c => { return 0; } }
}

fn main() {
    assert (or_alt(c) == 0);
    assert (or_alt(a(10, 100, 0u)) == 110);
    assert (or_alt(b(20, 200)) == 220);
}
